//! Floating desktop stage for Wayland compositors with wlr-layer-shell (COSMIC, KDE,
//! Sway, Hyprland...). Pixels go through shared memory, so no GPU or webview is needed.
#![cfg(target_os = "linux")]

use familiard_application::{walk_within, Failure, Player, Pointer, Stage};
use smithay_client_toolkit::{
    compositor::{CompositorHandler, CompositorState, Region},
    delegate_dispatch2, delegate_registry,
    output::{OutputHandler, OutputState},
    reexports::{
        calloop::{
            timer::{TimeoutAction, Timer},
            EventLoop,
        },
        calloop_wayland_source::WaylandSource,
    },
    registry::{ProvidesRegistryState, RegistryState},
    registry_handlers,
    seat::{
        pointer::{PointerEvent, PointerEventKind, PointerHandler, BTN_LEFT},
        Capability, SeatHandler, SeatState,
    },
    shell::{
        wlr_layer::{
            Anchor, KeyboardInteractivity, Layer, LayerShell, LayerShellHandler, LayerSurface,
            LayerSurfaceConfigure,
        },
        WaylandSurface,
    },
    shm::{
        slot::{Buffer, SlotPool},
        Shm, ShmHandler,
    },
};
use std::time::{Duration, Instant};
use wayland_client::{
    globals::registry_queue_init,
    protocol::{wl_output, wl_pointer, wl_seat, wl_shm, wl_surface},
    Connection, QueueHandle,
};

pub struct WaylandStage {
    at: (i32, i32),
}

impl WaylandStage {
    pub fn new(at: (i32, i32)) -> Self {
        Self { at }
    }
}

impl Stage for WaylandStage {
    fn perform(self: Box<Self>, player: Player) -> Result<(), Failure> {
        let conn = Connection::connect_to_env()?;
        let (globals, queue) = registry_queue_init(&conn)?;
        let qh = queue.handle();
        let compositor = CompositorState::bind(&globals, &qh)?;
        let layer_shell = LayerShell::bind(&globals, &qh)
            .map_err(|e| format!("this compositor has no wlr-layer-shell: {e}"))?;
        let shm = Shm::bind(&globals, &qh)?;

        let (width, height) = player.bounds();
        let surface = compositor.create_surface(&qh);
        let layer = open_layer(&layer_shell, surface, None, (width, height), self.at, &qh);

        let pool = SlotPool::new((width * height * 4 * 2) as usize, &shm)?;
        let mut state = Theatre {
            registry: RegistryState::new(&globals),
            seats: SeatState::new(&globals, &qh),
            outputs: OutputState::new(&globals, &qh),
            compositor,
            layer_shell,
            shm,
            pool,
            layer,
            pointer: None,
            output: None,
            buffer: None,
            player,
            box_size: (width, height),
            position: self.at,
            grabbed_at: None,
            configured: false,
            failed: None,
        };

        let mut event_loop: EventLoop<Theatre> = EventLoop::try_new()?;
        WaylandSource::new(conn, queue).insert(event_loop.handle()).map_err(|e| e.to_string())?;
        let mut last = Instant::now();
        let first = state.player.next_frame_in();
        event_loop
            .handle()
            .insert_source(Timer::from_duration(first), move |_, _, state: &mut Theatre| {
                let now = Instant::now();
                let elapsed = now - last;
                last = now;
                if state.player.tick(elapsed).is_some() && state.configured {
                    state.draw();
                }
                state.walk();
                let wait = state.player.next_frame_in().max(Duration::from_millis(1));
                TimeoutAction::ToDuration(wait)
            })
            .map_err(|e| e.to_string())?;

        loop {
            event_loop.dispatch(None, &mut state)?;
            if let Some(failure) = state.failed.take() {
                return Err(failure.into());
            }
            if state.player.leaving() {
                return Ok(());
            }
        }
    }
}

struct Theatre {
    registry: RegistryState,
    seats: SeatState,
    outputs: OutputState,
    compositor: CompositorState,
    layer_shell: LayerShell,
    shm: Shm,
    pool: SlotPool,
    layer: LayerSurface,
    pointer: Option<wl_pointer::WlPointer>,
    output: Option<wl_output::WlOutput>,
    buffer: Option<Buffer>,
    player: Player,
    box_size: (u32, u32),
    position: (i32, i32),
    grabbed_at: Option<(f64, f64)>,
    configured: bool,
    failed: Option<String>,
}

impl Theatre {
    fn draw(&mut self) {
        let (width, height) = self.box_size;
        let stride = width as i32 * 4;
        let created = self.pool.create_buffer(width as i32, height as i32, stride, wl_shm::Format::Argb8888);
        let (buffer, canvas) = match created {
            Ok(created) => created,
            Err(error) => {
                self.failed = Some(format!("cannot create a frame buffer: {error}"));
                return;
            }
        };
        let offset = self.player.paint(canvas);
        // Only the sprite takes clicks: the caption lets them through to what is below.
        let sprite = self.player.sprite();
        let region = Region::new(&self.compositor).ok();
        if let Some(region) = &region {
            for y in 0..sprite.height() {
                for (start, end) in sprite.solid_spans(y) {
                    region.add(
                        offset.0 as i32 + start as i32,
                        offset.1 as i32 + y as i32,
                        (end - start) as i32,
                        1,
                    );
                }
            }
        }
        let surface = self.layer.wl_surface();
        surface.set_input_region(region.as_ref().map(|r| r.wl_region()));
        surface.damage_buffer(0, 0, width as i32, height as i32);
        if buffer.attach_to(surface).is_err() {
            self.failed = Some("cannot attach the frame buffer".into());
            return;
        }
        self.layer.commit();
        self.buffer = Some(buffer);
    }

    fn drag_by(&mut self, dx: f64, dy: f64, qh: &QueueHandle<Self>) {
        let screens: Vec<(wl_output::WlOutput, Screen)> = self
            .outputs
            .outputs()
            .filter_map(|output| {
                let info = self.outputs.info(&output)?;
                let (x, y) = info.logical_position?;
                let (width, height) = info.logical_size?;
                Some((output, Screen { x, y, width, height }))
            })
            .collect();
        let rectangles: Vec<Screen> = screens.iter().map(|(_, screen)| *screen).collect();
        let current = screens.iter().position(|(output, _)| Some(output) == self.output.as_ref());
        let wanted = (self.position.0 + dx.round() as i32, self.position.1 + dy.round() as i32);
        let (screen, position) = place(wanted, self.box_size, &rectangles, current);
        self.position = position;
        match screen.filter(|index| Some(*index) != current) {
            Some(index) => self.move_to_output(screens[index].0.clone(), qh),
            None => {
                self.layer.set_margin(self.position.1, 0, 0, self.position.0);
                self.layer.commit();
            }
        }
    }

    /// Takes the step the pet asks for, inside the output it is on. It uses the same margin
    /// a drag does, but a walk never crosses to another output.
    fn walk(&mut self) {
        if !self.configured {
            return;
        }
        let Some(step) = self.player.step() else { return };
        let screen = self.output.as_ref().and_then(|output| self.outputs.info(output));
        let width = screen.and_then(|info| info.logical_size).map(|(width, _)| width);
        let moved = match width {
            Some(width) => {
                let x = walk_within(self.position.0, step, self.box_size.0, (0, width));
                let moved = x - self.position.0;
                self.position.0 = x;
                self.layer.set_margin(self.position.1, 0, 0, self.position.0);
                self.layer.commit();
                moved
            }
            None => 0,
        };
        self.player.walked(moved);
    }

    /// A layer surface cannot change output, so a new one takes over under the pointer.
    fn move_to_output(&mut self, output: wl_output::WlOutput, qh: &QueueHandle<Self>) {
        let surface = self.compositor.create_surface(qh);
        self.layer = open_layer(&self.layer_shell, surface, Some(&output), self.box_size, self.position, qh);
        self.output = Some(output);
        self.configured = false;
    }
}

fn open_layer(
    layer_shell: &LayerShell,
    surface: wl_surface::WlSurface,
    output: Option<&wl_output::WlOutput>,
    size: (u32, u32),
    position: (i32, i32),
    qh: &QueueHandle<Theatre>,
) -> LayerSurface {
    let layer = layer_shell.create_layer_surface(qh, surface, Layer::Overlay, Some("familiard"), output);
    layer.set_anchor(Anchor::TOP | Anchor::LEFT);
    layer.set_keyboard_interactivity(KeyboardInteractivity::None);
    layer.set_exclusive_zone(-1);
    layer.set_size(size.0, size.1);
    layer.set_margin(position.1, 0, 0, position.0);
    layer.commit();
    layer
}

/// A monitor in the desktop's shared coordinate space.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Screen {
    x: i32,
    y: i32,
    width: i32,
    height: i32,
}

impl Screen {
    fn contains(&self, point: (i32, i32)) -> bool {
        (self.x..self.x + self.width).contains(&point.0) && (self.y..self.y + self.height).contains(&point.1)
    }

    /// Past its edges a surface simply stops being visible, so positions stay inside.
    fn clamp(&self, position: (i32, i32), box_size: (u32, u32)) -> (i32, i32) {
        (
            position.0.min(self.width - box_size.0 as i32).max(0),
            position.1.min(self.height - box_size.1 as i32).max(0),
        )
    }
}

/// Decides where a dragged pet goes. `wanted` is relative to the `current` screen. The
/// screen under the pet's center wins; over a gap between screens it stays on `current`.
/// Returns the chosen screen and the position relative to it.
fn place(
    wanted: (i32, i32),
    box_size: (u32, u32),
    screens: &[Screen],
    current: Option<usize>,
) -> (Option<usize>, (i32, i32)) {
    let Some(here) = current.and_then(|index| screens.get(index)) else {
        return (current, (wanted.0.max(0), wanted.1.max(0)));
    };
    let global = (here.x + wanted.0, here.y + wanted.1);
    let center = (global.0 + box_size.0 as i32 / 2, global.1 + box_size.1 as i32 / 2);
    match screens.iter().position(|screen| screen.contains(center)) {
        Some(index) => {
            let there = screens[index];
            (Some(index), there.clamp((global.0 - there.x, global.1 - there.y), box_size))
        }
        None => (current, here.clamp(wanted, box_size)),
    }
}

impl CompositorHandler for Theatre {
    fn scale_factor_changed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, _: i32) {}
    fn transform_changed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, _: wl_output::Transform) {}
    fn frame(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, _: u32) {}
    fn surface_enter(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, output: &wl_output::WlOutput) {
        self.output = Some(output.clone());
    }
    fn surface_leave(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, _: &wl_output::WlOutput) {}
}

impl OutputHandler for Theatre {
    fn output_state(&mut self) -> &mut OutputState {
        &mut self.outputs
    }
    fn new_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
    fn update_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
    fn output_destroyed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
}

impl LayerShellHandler for Theatre {
    fn closed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &LayerSurface) {
        self.failed = Some("the compositor closed the pet".into());
    }
    fn configure(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &LayerSurface, _: LayerSurfaceConfigure, _: u32) {
        if !self.configured {
            self.configured = true;
            self.draw();
        }
    }
}

impl SeatHandler for Theatre {
    fn seat_state(&mut self) -> &mut SeatState {
        &mut self.seats
    }
    fn new_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat) {}
    fn new_capability(&mut self, _: &Connection, qh: &QueueHandle<Self>, seat: wl_seat::WlSeat, capability: Capability) {
        if capability == Capability::Pointer && self.pointer.is_none() {
            self.pointer = self.seats.get_pointer(qh, &seat).ok();
        }
    }
    fn remove_capability(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat, capability: Capability) {
        if capability == Capability::Pointer {
            if let Some(pointer) = self.pointer.take() {
                pointer.release();
            }
        }
    }
    fn remove_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat) {}
}

impl PointerHandler for Theatre {
    fn pointer_frame(&mut self, _: &Connection, qh: &QueueHandle<Self>, _: &wl_pointer::WlPointer, events: &[PointerEvent]) {
        let mut changed = false;
        for event in events {
            if &event.surface != self.layer.wl_surface() {
                continue;
            }
            match event.kind {
                PointerEventKind::Enter { .. } => changed |= self.player.pointer(Pointer::Entered),
                PointerEventKind::Press { button: BTN_LEFT, .. } => {
                    self.grabbed_at = Some(event.position);
                    changed |= self.player.pointer(Pointer::Pressed);
                }
                PointerEventKind::Release { button: BTN_LEFT, .. } => {
                    self.grabbed_at = None;
                    changed |= self.player.pointer(Pointer::Released);
                }
                // While a button is held the compositor keeps sending motion here, so
                // leaving means the grab was lost. Dropping it keeps the pet from
                // following a pointer with no button pressed.
                PointerEventKind::Leave { .. } => {
                    self.grabbed_at = None;
                    changed |= self.player.pointer(Pointer::Left);
                }
                PointerEventKind::Motion { .. } => {
                    // The surface moves under the pointer, so the offset from the grab
                    // point is always the distance still to travel.
                    if let Some(grab) = self.grabbed_at {
                        let (dx, dy) = (event.position.0 - grab.0, event.position.1 - grab.1);
                        changed |= self.player.pointer(Pointer::Dragged((dx.abs() + dy.abs()).round() as u32));
                        self.drag_by(dx, dy, qh);
                    }
                }
                _ => {}
            }
        }
        // Waking up or looking at the pointer should not wait for the next timer.
        if changed && self.configured {
            self.draw();
        }
    }
}

impl ShmHandler for Theatre {
    fn shm_state(&mut self) -> &mut Shm {
        &mut self.shm
    }
}

impl ProvidesRegistryState for Theatre {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry
    }
    registry_handlers![OutputState, SeatState];
}

delegate_registry!(Theatre);
delegate_dispatch2!(Theatre);

#[cfg(test)]
mod tests {
    use super::*;

    const LEFT: Screen = Screen { x: 0, y: 0, width: 1920, height: 1080 };
    const RIGHT: Screen = Screen { x: 1920, y: 0, width: 1920, height: 1080 };
    const BOX: (u32, u32) = (120, 180);

    #[test]
    fn dragging_stops_at_the_outer_edges() {
        let screens = [LEFT, RIGHT];
        assert_eq!(place((-500, -500), BOX, &screens, Some(0)), (Some(0), (0, 0)));
        assert_eq!(place((5000, 5000), BOX, &screens, Some(1)), (Some(1), (1800, 900)));
    }

    #[test]
    fn stays_on_the_screen_until_its_center_crosses() {
        let screens = [LEFT, RIGHT];
        assert_eq!(place((1800, 300), BOX, &screens, Some(0)), (Some(0), (1800, 300)));
        assert_eq!(place((1859, 300), BOX, &screens, Some(0)), (Some(0), (1800, 300)));
    }

    #[test]
    fn crossing_converts_to_the_coordinates_of_the_new_screen() {
        let screens = [LEFT, RIGHT];
        assert_eq!(place((1860, 300), BOX, &screens, Some(0)), (Some(1), (0, 300)));
        assert_eq!(place((1900, 300), BOX, &screens, Some(0)), (Some(1), (0, 300)));
        assert_eq!(place((-50, 300), BOX, &screens, Some(1)), (Some(1), (0, 300)));
        assert_eq!(place((-100, 300), BOX, &screens, Some(1)), (Some(0), (1800, 300)));
        assert_eq!(place((400, 300), BOX, &screens, Some(0)), (Some(0), (400, 300)));
    }

    #[test]
    fn screens_that_do_not_touch_or_differ_in_height_are_handled() {
        let short = Screen { x: 1920, y: 200, width: 800, height: 600 };
        let screens = [LEFT, short];
        assert_eq!(place((1900, 150), BOX, &screens, Some(0)), (Some(1), (0, 0)));
        assert_eq!(place((1900, 0), BOX, &screens, Some(0)), (Some(0), (1800, 0)));
    }

    #[test]
    fn an_unknown_screen_only_limits_the_top_left() {
        assert_eq!(place((5000, -1), BOX, &[], None), (None, (5000, 0)));
    }
}
