//! Floating desktop stage for Windows, using a layered window. Windows composes it with
//! per-pixel alpha and hit-tests it the same way, so clicks on transparent pixels reach
//! whatever is below without any work here.
#![cfg(windows)]

use familiard_application::{walk_within, Failure, Player, Pointer, Stage};
use std::cell::RefCell;
use std::ptr::{null, null_mut};
use std::time::{Duration, Instant};
use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, SIZE, WPARAM};
use windows_sys::Win32::Graphics::Gdi::{
    CreateCompatibleDC, CreateDIBSection, GetDC, GetMonitorInfoW, MonitorFromWindow, ReleaseDC, SelectObject,
    AC_SRC_ALPHA, AC_SRC_OVER, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, BLENDFUNCTION, DIB_RGB_COLORS, HDC, MONITORINFO,
    MONITOR_DEFAULTTONEAREST,
};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::HiDpi::{SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{TrackMouseEvent, TME_LEAVE, TME_NONCLIENT, TRACKMOUSEEVENT};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DispatchMessageW, GetMessageW, GetWindowRect, LoadCursorW, PostQuitMessage,
    RegisterClassW, SetTimer, SetWindowPos, ShowWindow, TranslateMessage, UpdateLayeredWindow, HTCAPTION, IDC_ARROW,
    MSG, SWP_NOACTIVATE, SWP_NOSIZE, SWP_NOZORDER, SW_SHOWNOACTIVATE, ULW_ALPHA, WM_DESTROY, WM_NCHITTEST,
    WM_NCLBUTTONDOWN, WM_NCMOUSELEAVE, WM_NCMOUSEMOVE, WM_TIMER, WNDCLASSW, WS_EX_LAYERED, WS_EX_NOACTIVATE,
    WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP,
};

const CLASS_NAME: &str = "familiard";
const TIMER_ID: usize = 1;

pub struct WindowsStage {
    at: (i32, i32),
}

impl WindowsStage {
    pub fn new(at: (i32, i32)) -> Self {
        Self { at }
    }
}

thread_local! {
    /// The window procedure takes no context of ours, and it only ever runs on this thread.
    static THEATRE: RefCell<Option<Theatre>> = const { RefCell::new(None) };
}

struct Theatre {
    player: Player,
    last: Instant,
    box_size: (u32, u32),
    surface: HDC,
    pixels: *mut u8,
    /// Whether the pointer is over the pet, since Windows only says when it moves there.
    hovering: bool,
    /// Windows is running its drag loop, which sends leave messages of its own.
    held: bool,
}

impl Theatre {
    fn present(&self, window: HWND) {
        let (width, height) = self.box_size;
        let length = width as usize * height as usize * 4;
        // SAFETY: `pixels` is the bitmap of `box_size` that lives as long as the process.
        let canvas = unsafe { std::slice::from_raw_parts_mut(self.pixels, length) };
        self.player.paint(canvas);
        let size = SIZE { cx: width as i32, cy: height as i32 };
        let origin = POINT { x: 0, y: 0 };
        let blend = BLENDFUNCTION {
            BlendOp: AC_SRC_OVER as u8,
            BlendFlags: 0,
            SourceConstantAlpha: 255,
            AlphaFormat: AC_SRC_ALPHA as u8,
        };
        // SAFETY: every pointer refers to a live local, and a null destination keeps the
        // window where it is, which is what lets it be dragged.
        unsafe {
            UpdateLayeredWindow(window, null_mut(), null(), &size, self.surface, &origin, 0, &blend, ULW_ALPHA);
        }
    }

    fn schedule(&self, window: HWND) {
        let wait = self.player.next_frame_in().max(Duration::from_millis(1));
        // SAFETY: replaces the timer of a window owned by this thread.
        unsafe {
            SetTimer(window, TIMER_ID, wait.as_millis() as u32, None);
        }
    }

    fn tick(&mut self, window: HWND) {
        let now = Instant::now();
        let elapsed = now - self.last;
        self.last = now;
        let changed = self.player.tick(elapsed).is_some();
        if self.player.leaving() {
            // SAFETY: ends the message loop of this thread, which ends the stage.
            unsafe {
                PostQuitMessage(0);
            }
            return;
        }
        if changed {
            self.present(window);
        }
        self.walk(window);
        self.schedule(window);
    }

    /// Takes the step the pet asks for, inside the work area of its monitor. `SetWindowPos`
    /// is what the drag Windows runs for us ends up doing to the window.
    fn walk(&mut self, window: HWND) {
        let Some(step) = self.player.step() else { return };
        let moved = move_inside_monitor(window, step, self.box_size.0).unwrap_or(0);
        self.player.walked(moved);
    }

    fn feel(&mut self, window: HWND, event: Pointer) {
        if self.player.pointer(event) {
            self.present(window);
        }
        self.schedule(window);
    }
}

/// Moves the window sideways, keeping it in its monitor's work area. Returns how far it went.
fn move_inside_monitor(window: HWND, step: i32, width: u32) -> Option<i32> {
    // SAFETY: every pointer refers to a live local, for a window owned by this thread.
    unsafe {
        let mut rect: RECT = std::mem::zeroed();
        if GetWindowRect(window, &mut rect) == 0 {
            return None;
        }
        let monitor = MonitorFromWindow(window, MONITOR_DEFAULTTONEAREST);
        let mut info: MONITORINFO = std::mem::zeroed();
        info.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
        if GetMonitorInfoW(monitor, &mut info) == 0 {
            return None;
        }
        let x = walk_within(rect.left, step, width, (info.rcWork.left, info.rcWork.right));
        let flags = SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE;
        if SetWindowPos(window, null_mut(), x, rect.top, 0, 0, flags) == 0 {
            return None;
        }
        Some(x - rect.left)
    }
}

fn window_corner(window: HWND) -> Option<(i32, i32)> {
    // SAFETY: the pointer refers to a live local, for a window owned by this thread.
    unsafe {
        let mut rect: RECT = std::mem::zeroed();
        (GetWindowRect(window, &mut rect) != 0).then_some((rect.left, rect.top))
    }
}

/// Runs on the stage, unless a message arrived in the middle of another one: a lost mouse
/// event is harmless, and a panic here could not unwind through Windows.
fn with_theatre(action: impl FnOnce(&mut Theatre)) {
    THEATRE.with(|theatre| {
        if let Ok(mut theatre) = theatre.try_borrow_mut() {
            if let Some(theatre) = theatre.as_mut() {
                action(theatre);
            }
        }
    });
}

unsafe extern "system" fn window_procedure(window: HWND, message: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match message {
        WM_TIMER => {
            with_theatre(|theatre| theatre.tick(window));
            0
        }
        WM_NCMOUSEMOVE => {
            with_theatre(|theatre| {
                if !theatre.hovering && !theatre.held {
                    theatre.hovering = true;
                    let mut track = TRACKMOUSEEVENT {
                        cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32,
                        dwFlags: TME_LEAVE | TME_NONCLIENT,
                        hwndTrack: window,
                        dwHoverTime: 0,
                    };
                    TrackMouseEvent(&mut track);
                    theatre.feel(window, Pointer::Entered);
                }
            });
            DefWindowProcW(window, message, wparam, lparam)
        }
        WM_NCMOUSELEAVE => {
            with_theatre(|theatre| {
                theatre.hovering = false;
                if !theatre.held {
                    theatre.feel(window, Pointer::Left);
                }
            });
            DefWindowProcW(window, message, wparam, lparam)
        }
        // On a title bar, the default handling runs the whole drag and returns at the release,
        // while timers keep arriving here. So nothing may stay borrowed across it.
        WM_NCLBUTTONDOWN => {
            let from = window_corner(window);
            with_theatre(|theatre| {
                theatre.held = true;
                theatre.feel(window, Pointer::Pressed);
            });
            let result = DefWindowProcW(window, message, wparam, lparam);
            let to = window_corner(window);
            with_theatre(|theatre| {
                theatre.held = false;
                theatre.hovering = false;
                if let (Some(from), Some(to)) = (from, to) {
                    theatre.player.pointer(Pointer::Dragged(to.0 - from.0, to.1 - from.1));
                }
                theatre.feel(window, Pointer::Released);
            });
            result
        }
        // Everywhere the pet is solid counts as a title bar, so Windows does the dragging.
        // Transparent pixels never get here.
        WM_NCHITTEST => HTCAPTION as LRESULT,
        WM_DESTROY => {
            PostQuitMessage(0);
            0
        }
        _ => DefWindowProcW(window, message, wparam, lparam),
    }
}

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}

impl Stage for WindowsStage {
    fn perform(self: Box<Self>, player: Player) -> Result<(), Failure> {
        let (width, height) = player.bounds();
        let class_name = wide(CLASS_NAME);
        // SAFETY: plain Win32 calls with live arguments, all on this thread. Results that
        // can fail are checked before use.
        unsafe {
            // Pixel for pixel on any scale setting, instead of stretched and blurred.
            SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
            let instance = GetModuleHandleW(null());
            let class = WNDCLASSW {
                style: 0,
                lpfnWndProc: Some(window_procedure),
                cbClsExtra: 0,
                cbWndExtra: 0,
                hInstance: instance,
                hIcon: null_mut(),
                hCursor: LoadCursorW(null_mut(), IDC_ARROW),
                hbrBackground: null_mut(),
                lpszMenuName: null(),
                lpszClassName: class_name.as_ptr(),
            };
            if RegisterClassW(&class) == 0 {
                return Err("cannot register the window class".into());
            }
            let window = CreateWindowExW(
                WS_EX_LAYERED | WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
                class_name.as_ptr(),
                class_name.as_ptr(),
                WS_POPUP,
                self.at.0,
                self.at.1,
                width as i32,
                height as i32,
                null_mut(),
                null_mut(),
                instance,
                null(),
            );
            if window.is_null() {
                return Err("cannot create the window".into());
            }

            let screen = GetDC(null_mut());
            let surface = CreateCompatibleDC(screen);
            ReleaseDC(null_mut(), screen);
            let mut info: BITMAPINFO = std::mem::zeroed();
            info.bmiHeader = BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: width as i32,
                biHeight: -(height as i32), // negative: rows run top to bottom
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB,
                biSizeImage: 0,
                biXPelsPerMeter: 0,
                biYPelsPerMeter: 0,
                biClrUsed: 0,
                biClrImportant: 0,
            };
            let mut bits = null_mut();
            let bitmap = CreateDIBSection(surface, &info, DIB_RGB_COLORS, &mut bits, null_mut(), 0);
            if surface.is_null() || bitmap.is_null() || bits.is_null() {
                return Err("cannot create the frame buffer".into());
            }
            SelectObject(surface, bitmap);

            let theatre = Theatre {
                player,
                last: Instant::now(),
                box_size: (width, height),
                surface,
                pixels: bits.cast(),
                hovering: false,
                held: false,
            };
            theatre.present(window);
            theatre.schedule(window);
            THEATRE.with(|slot| *slot.borrow_mut() = Some(theatre));
            ShowWindow(window, SW_SHOWNOACTIVATE);

            let mut message: MSG = std::mem::zeroed();
            while GetMessageW(&mut message, null_mut(), 0, 0) > 0 {
                TranslateMessage(&message);
                DispatchMessageW(&message);
            }
        }
        Ok(())
    }
}
