//! Floating desktop stage for Wayland compositors with wlr-layer-shell (COSMIC, KDE,
//! Sway, Hyprland...). Pixels go through shared memory, so no GPU or webview is needed.
#![cfg(target_os = "linux")]

use familiard_application::{Failure, Player, Stage};
use familiard_domain::Sprite;
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
        let layer = layer_shell.create_layer_surface(&qh, surface, Layer::Overlay, Some("familiard"), None);
        layer.set_anchor(Anchor::TOP | Anchor::LEFT);
        layer.set_keyboard_interactivity(KeyboardInteractivity::None);
        layer.set_exclusive_zone(-1);
        layer.set_size(width, height);
        layer.set_margin(self.at.1, 0, 0, self.at.0);
        layer.commit();

        let pool = SlotPool::new((width * height * 4 * 2) as usize, &shm)?;
        let mut state = Theatre {
            registry: RegistryState::new(&globals),
            seats: SeatState::new(&globals, &qh),
            outputs: OutputState::new(&globals, &qh),
            compositor,
            shm,
            pool,
            layer,
            pointer: None,
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
                let wait = state.player.next_frame_in().max(Duration::from_millis(1));
                TimeoutAction::ToDuration(wait)
            })
            .map_err(|e| e.to_string())?;

        loop {
            event_loop.dispatch(None, &mut state)?;
            if let Some(failure) = state.failed.take() {
                return Err(failure.into());
            }
        }
    }
}

struct Theatre {
    registry: RegistryState,
    seats: SeatState,
    outputs: OutputState,
    compositor: CompositorState,
    shm: Shm,
    pool: SlotPool,
    layer: LayerSurface,
    pointer: Option<wl_pointer::WlPointer>,
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
        let sprite = self.player.sprite();
        let offset = paint(canvas, (width, height), sprite);
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

    fn drag_by(&mut self, dx: f64, dy: f64) {
        self.position.0 = (self.position.0 + dx.round() as i32).max(0);
        self.position.1 = (self.position.1 + dy.round() as i32).max(0);
        self.layer.set_margin(self.position.1, 0, 0, self.position.0);
        self.layer.commit();
    }
}

/// Clears the canvas and paints the sprite bottom-centered as premultiplied ARGB, the
/// byte order Wayland expects on little-endian machines. Returns the sprite's offset.
fn paint(canvas: &mut [u8], box_size: (u32, u32), sprite: &Sprite) -> (u32, u32) {
    canvas.fill(0);
    let (width, height) = box_size;
    let left = width.saturating_sub(sprite.width()) / 2;
    let top = height.saturating_sub(sprite.height());
    for (y, row) in sprite.rgba().chunks_exact(sprite.width() as usize * 4).enumerate() {
        let start = ((top as usize + y) * width as usize + left as usize) * 4;
        for (pixel, out) in row.chunks_exact(4).zip(canvas[start..].chunks_exact_mut(4)) {
            let alpha = pixel[3] as u32;
            let scale = |channel: u8| ((channel as u32 * alpha + 127) / 255) as u8;
            out.copy_from_slice(&[scale(pixel[2]), scale(pixel[1]), scale(pixel[0]), pixel[3]]);
        }
    }
    (left, top)
}

impl CompositorHandler for Theatre {
    fn scale_factor_changed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, _: i32) {}
    fn transform_changed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, _: wl_output::Transform) {}
    fn frame(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, _: u32) {}
    fn surface_enter(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, _: &wl_output::WlOutput) {}
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
    fn pointer_frame(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_pointer::WlPointer, events: &[PointerEvent]) {
        for event in events {
            if &event.surface != self.layer.wl_surface() {
                continue;
            }
            match event.kind {
                PointerEventKind::Press { button: BTN_LEFT, .. } => self.grabbed_at = Some(event.position),
                PointerEventKind::Release { button: BTN_LEFT, .. } => self.grabbed_at = None,
                PointerEventKind::Motion { .. } => {
                    // The surface moves under the pointer, so the offset from the grab
                    // point is always the distance still to travel.
                    if let Some(grab) = self.grabbed_at {
                        self.drag_by(event.position.0 - grab.0, event.position.1 - grab.1);
                    }
                }
                _ => {}
            }
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

    #[test]
    fn paints_premultiplied_bgra_at_the_bottom_center() {
        let sprite = Sprite::new(1, 1, vec![200, 100, 50, 128]).unwrap();
        let mut canvas = vec![7u8; 3 * 2 * 4];
        let offset = paint(&mut canvas, (3, 2), &sprite);
        assert_eq!(offset, (1, 1));
        let at = (3 + 1) * 4;
        assert_eq!(&canvas[at..at + 4], &[25, 50, 100, 128]);
        assert!(canvas[..at].iter().all(|byte| *byte == 0));
    }
}
