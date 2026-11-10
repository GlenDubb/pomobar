//! Small Win32 helpers: colours, strings, DPI maths, message parameter unpacking.

use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, RECT};
use windows::Win32::Graphics::Direct2D::Common::{D2D1_COLOR_F, D2D_RECT_F};
use windows::Win32::Graphics::Dwm::{
    DwmSetWindowAttribute, DWMWA_BORDER_COLOR, DWMWA_CAPTION_COLOR, DWMWA_TEXT_COLOR,
    DWMWA_USE_IMMERSIVE_DARK_MODE,
};
use windows::Win32::Graphics::Gdi::{GetMonitorInfoW, MonitorFromWindow, MONITORINFO, MONITOR_DEFAULTTONEAREST};
use windows::Win32::System::SystemInformation::{GetLocalTime, GetTickCount64};
use windows::Win32::UI::HiDpi::AdjustWindowRectExForDpi;
use windows::Win32::UI::WindowsAndMessaging::{WINDOW_EX_STYLE, WINDOW_STYLE};

use crate::timer::Mode;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rgb(pub u8, pub u8, pub u8);

impl Rgb {
    pub fn d2d(self, a: f32) -> D2D1_COLOR_F {
        D2D1_COLOR_F {
            r: f32::from(self.0) / 255.0,
            g: f32::from(self.1) / 255.0,
            b: f32::from(self.2) / 255.0,
            a,
        }
    }

    pub fn colorref(self) -> COLORREF {
        COLORREF(u32::from(self.0) | (u32::from(self.1) << 8) | (u32::from(self.2) << 16))
    }
}

pub const WHITE: Rgb = Rgb(255, 255, 255);
/// Badge colour when the timer isn't counting down.
pub const IDLE_GREY: Rgb = Rgb(0x5F, 0x63, 0x68);

/// Background colour for each mode. All have at least 5:1 contrast with white text.
pub fn mode_color(mode: Mode) -> Rgb {
    match mode {
        Mode::Focus => Rgb(0xC4, 0x40, 0x34),
        Mode::ShortBreak => Rgb(0x26, 0x7A, 0x5E),
        Mode::LongBreak => Rgb(0x3A, 0x5C, 0xA8),
    }
}

pub fn white(a: f32) -> D2D1_COLOR_F {
    WHITE.d2d(a)
}

pub fn black(a: f32) -> D2D1_COLOR_F {
    Rgb(0, 0, 0).d2d(a)
}

pub fn rect(left: f32, top: f32, right: f32, bottom: f32) -> D2D_RECT_F {
    D2D_RECT_F { left, top, right, bottom }
}

pub fn offset(r: D2D_RECT_F, dx: f32, dy: f32) -> D2D_RECT_F {
    rect(r.left + dx, r.top + dy, r.right + dx, r.bottom + dy)
}

pub fn contains(r: &D2D_RECT_F, x: f32, y: f32) -> bool {
    x >= r.left && x < r.right && y >= r.top && y < r.bottom
}

/// Copy into a fixed-size UTF-16 buffer (e.g. a tooltip field), truncating and
/// always leaving a terminating null.
pub fn fill_wstr(dst: &mut [u16], s: &str) {
    if dst.is_empty() {
        return;
    }
    let mut n = 0;
    for c in s.encode_utf16() {
        if n + 1 >= dst.len() {
            break;
        }
        dst[n] = c;
        n += 1;
    }
    dst[n] = 0;
}

pub fn loword(v: usize) -> u16 {
    (v & 0xFFFF) as u16
}

pub fn hiword(v: usize) -> u16 {
    ((v >> 16) & 0xFFFF) as u16
}

/// Signed client coordinates packed into an LPARAM (GET_X_LPARAM / GET_Y_LPARAM).
pub fn point_from_lparam(l: LPARAM) -> (i32, i32) {
    let x = (l.0 & 0xFFFF) as u16 as i16 as i32;
    let y = ((l.0 >> 16) & 0xFFFF) as u16 as i16 as i32;
    (x, y)
}

/// Milliseconds since boot. Keeps counting while the PC sleeps, so a session
/// that "ends" during sleep is finished when the PC wakes up.
pub fn now_ms() -> u64 {
    unsafe { GetTickCount64() }
}

/// Local date as YYYY-MM-DD.
pub fn today() -> String {
    let t = unsafe { GetLocalTime() };
    format!("{:04}-{:02}-{:02}", t.wYear, t.wMonth, t.wDay)
}

pub fn scale(v: f32, dpi: u32) -> i32 {
    (v * dpi as f32 / 96.0).round() as i32
}

/// Outer window size (pixels) for a client area of the given size in DIPs.
pub fn window_size(w: f32, h: f32, style: WINDOW_STYLE, ex: WINDOW_EX_STYLE, dpi: u32) -> (i32, i32) {
    let mut rc = RECT {
        left: 0,
        top: 0,
        right: scale(w, dpi),
        bottom: scale(h, dpi),
    };
    unsafe {
        let _ = AdjustWindowRectExForDpi(&mut rc, style, false, ex, dpi);
    }
    (rc.right - rc.left, rc.bottom - rc.top)
}

/// Work area (screen minus taskbar) of the monitor a window is on.
pub fn work_area(hwnd: HWND) -> RECT {
    unsafe {
        let mon = MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST);
        let mut mi = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        let _ = GetMonitorInfoW(mon, &mut mi);
        mi.rcWork
    }
}

/// Colour the title bar to match the window (Windows 11), with a dark title bar
/// as the fallback on Windows 10. Unsupported attributes are silently ignored.
pub fn set_title_bar_color(hwnd: HWND, bg: Rgb) {
    unsafe {
        let dark: i32 = 1;
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_USE_IMMERSIVE_DARK_MODE,
            &dark as *const i32 as *const _,
            std::mem::size_of::<i32>() as u32,
        );
        let caption = bg.colorref();
        let text = WHITE.colorref();
        for (attr, value) in [
            (DWMWA_CAPTION_COLOR, &caption),
            (DWMWA_BORDER_COLOR, &caption),
            (DWMWA_TEXT_COLOR, &text),
        ] {
            let _ = DwmSetWindowAttribute(
                hwnd,
                attr,
                value as *const COLORREF as *const _,
                std::mem::size_of::<COLORREF>() as u32,
            );
        }
    }
}
