//! Floating desktop stage for Windows, using a layered window. Windows composes it with
//! per-pixel alpha and hit-tests it the same way, so clicks on transparent pixels reach
//! whatever is below without any work here.
#![cfg(windows)]

use familiard_application::{Failure, Player, Stage};
use std::cell::RefCell;
use std::ptr::{null, null_mut};
use std::time::{Duration, Instant};
use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, SIZE, WPARAM};
use windows_sys::Win32::Graphics::Gdi::{
    CreateCompatibleDC, CreateDIBSection, GetDC, ReleaseDC, SelectObject, AC_SRC_ALPHA, AC_SRC_OVER, BITMAPINFO,
    BITMAPINFOHEADER, BI_RGB, BLENDFUNCTION, DIB_RGB_COLORS, HDC,
};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::HiDpi::{SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DispatchMessageW, GetMessageW, LoadCursorW, PostQuitMessage, RegisterClassW,
    SetTimer, ShowWindow, TranslateMessage, UpdateLayeredWindow, HTCAPTION, IDC_ARROW, MSG, SW_SHOWNOACTIVATE,
    ULW_ALPHA, WM_DESTROY, WM_NCHITTEST, WM_TIMER, WNDCLASSW, WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW,
    WS_EX_TOPMOST, WS_POPUP,
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
        self.schedule(window);
    }
}

unsafe extern "system" fn window_procedure(window: HWND, message: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match message {
        WM_TIMER => {
            THEATRE.with(|theatre| {
                if let Some(theatre) = theatre.borrow_mut().as_mut() {
                    theatre.tick(window);
                }
            });
            0
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

            let theatre =
                Theatre { player, last: Instant::now(), box_size: (width, height), surface, pixels: bits.cast() };
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
