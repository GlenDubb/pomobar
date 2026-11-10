//! Mini mode: when the main window is minimised, a small always-on-top pill
//! shows the full `mm:ss` just above the taskbar.
//!
//! - Click it to bring the main window back; the round button starts/pauses.
//! - Drag it anywhere; let go near a screen edge and it snaps to that edge.
//!   Its position is remembered.
//! - Right-click for the same menu as the tray icon.
//! - It never takes focus, and it hides while a full-screen app or
//!   presentation is running.

use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows::Win32::Graphics::Direct2D::Common::D2D_RECT_F;
use windows::Win32::Foundation::D2DERR_RECREATE_TARGET;
use windows::Win32::Graphics::Dwm::{DwmSetWindowAttribute, DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, EndPaint, GetMonitorInfoW, InvalidateRect, MonitorFromPoint, MonitorFromWindow, HBRUSH,
    MONITORINFO, MONITOR_DEFAULTTONEAREST, MONITOR_DEFAULTTONULL, PAINTSTRUCT,
};
use windows::Win32::UI::HiDpi::GetDpiForWindow;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    ReleaseCapture, SetCapture, TrackMouseEvent, TME_LEAVE, TRACKMOUSEEVENT,
};
use windows::Win32::UI::Shell::{
    SHQueryUserNotificationState, QUNS_BUSY, QUNS_PRESENTATION_MODE, QUNS_RUNNING_D3D_FULL_SCREEN,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, GetCursorPos, GetWindowLongPtrW, GetWindowRect,
    IsWindowVisible, KillTimer, LoadCursorW, RegisterClassExW, SetCursor, SetTimer, SetWindowLongPtrW,
    SetWindowPos, ShowWindow, CREATESTRUCTW, GWLP_USERDATA, HCURSOR, HTCLIENT, HWND_TOPMOST, IDC_HAND,
    MA_NOACTIVATE, SWP_NOACTIVATE, SWP_NOSIZE, SWP_NOZORDER, SW_HIDE, SW_SHOWNOACTIVATE, WM_DESTROY,
    WM_DPICHANGED, WM_ERASEBKGND, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MOUSEACTIVATE, WM_MOUSEMOVE,
    WM_NCCREATE, WM_PAINT, WM_RBUTTONUP, WM_SETCURSOR, WM_SIZE, WM_TIMER, WNDCLASSEXW, WS_EX_NOACTIVATE,
    WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP,
};
use windows::Win32::UI::Controls::WM_MOUSELEAVE;

use super::app::App;
use super::gfx::Surface;
use super::util::{contains, hiword, loword, mode_color, now_ms, rect, scale, white, IDLE_GREY};
use crate::timer::{format_mmss, Phase};

const CLASS: PCWSTR = w!("WilliamsAutomation.Pomobar.Mini");
const WIDTH: f32 = 140.0;
const HEIGHT: f32 = 44.0;
/// Gap between the pill and the screen edge / taskbar, in DIPs.
const MARGIN: f32 = 10.0;
/// Let go this close to an edge (DIPs) and the pill snaps to it.
const SNAP: f32 = 32.0;
const TIMER_WATCH: usize = 1;

pub struct Mini {
    hwnd: HWND,
    surface: Option<Surface>,
    dpi: u32,
    hover_button: bool,
    tracking: bool,
    drag: Option<Drag>,
    digit_w: f32,
    colon_w: f32,
}

#[derive(Clone, Copy)]
struct Drag {
    cursor: POINT,
    window: POINT,
    moved: bool,
    on_button: bool,
}

pub fn register(hinst: HINSTANCE, cursor: HCURSOR) {
    let wc = WNDCLASSEXW {
        cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
        lpfnWndProc: Some(proc_),
        hInstance: hinst,
        hCursor: cursor,
        hbrBackground: HBRUSH::default(),
        lpszClassName: CLASS,
        ..Default::default()
    };
    unsafe {
        RegisterClassExW(&wc);
    }
}

fn button_rect() -> D2D_RECT_F {
    rect(WIDTH - 40.0, 6.0, WIDTH - 8.0, HEIGHT - 6.0)
}

pub fn is_shown(app: &App) -> bool {
    app.mini.try_borrow().map(|m| m.is_some()).unwrap_or(false)
}

/// Show the pill (does nothing if it's already showing).
pub fn show(app: &App) {
    if is_shown(app) {
        return;
    }
    unsafe {
        let ex = WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE;
        let Ok(hwnd) = CreateWindowExW(
            ex,
            CLASS,
            w!("Pomobar"),
            WS_POPUP,
            0,
            0,
            140,
            44,
            None,
            None,
            Some(app.hinst),
            Some(app as *const App as *const _),
        ) else {
            return;
        };
        let pref = DWMWCP_ROUND;
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_WINDOW_CORNER_PREFERENCE,
            &pref as *const _ as *const _,
            std::mem::size_of_val(&pref) as u32,
        );
        let t = &app.gfx.text.mini_time;
        let digit_w = (0..10).map(|d| app.gfx.text_width(t, &d.to_string())).fold(0.0, f32::max);
        let colon_w = app.gfx.text_width(t, ":");
        *app.mini.borrow_mut() = Some(Mini {
            hwnd,
            surface: None,
            dpi: 96,
            hover_button: false,
            tracking: false,
            drag: None,
            digit_w,
            colon_w,
        });

        // Where it was last time if that's still on a screen; otherwise bottom-left
        // of the main window's monitor, just above the taskbar.
        let saved = app.mini_pos().filter(|&(x, y)| {
            !MonitorFromPoint(POINT { x: x + 20, y: y + 10 }, MONITOR_DEFAULTTONULL).is_invalid()
        });
        let (x, y) = match saved {
            Some(p) => p,
            None => {
                let wa = work_area_of(MonitorFromWindow(app.hwnd(), MONITOR_DEFAULTTONEAREST));
                let dpi = GetDpiForWindow(app.hwnd()).max(96);
                (wa.left + scale(MARGIN, dpi), wa.bottom - scale(MARGIN + HEIGHT, dpi))
            }
        };
        let _ = SetWindowPos(hwnd, None, x, y, 0, 0, SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE);
        resize_for_dpi(app, hwnd);
        clamp_into_work_area(app, hwnd, false);
        SetTimer(Some(hwnd), TIMER_WATCH, 1000, None);
        if !fullscreen_app_running() {
            let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
        }
    }
}

pub fn hide(app: &App) {
    let hwnd = app.mini.try_borrow().ok().and_then(|m| m.as_ref().map(|m| m.hwnd));
    if let Some(hwnd) = hwnd {
        unsafe {
            let _ = DestroyWindow(hwnd);
        }
    }
    if let Ok(mut m) = app.mini.try_borrow_mut() {
        *m = None;
    }
}

pub fn invalidate(app: &App) {
    if let Some(hwnd) = app.mini.try_borrow().ok().and_then(|m| m.as_ref().map(|m| m.hwnd)) {
        unsafe {
            let _ = InvalidateRect(Some(hwnd), None, false);
        }
    }
}

fn fullscreen_app_running() -> bool {
    matches!(
        unsafe { SHQueryUserNotificationState() },
        Ok(s) if s == QUNS_BUSY || s == QUNS_RUNNING_D3D_FULL_SCREEN || s == QUNS_PRESENTATION_MODE
    )
}

fn work_area_of(mon: windows::Win32::Graphics::Gdi::HMONITOR) -> RECT {
    let mut mi = MONITORINFO {
        cbSize: std::mem::size_of::<MONITORINFO>() as u32,
        ..Default::default()
    };
    unsafe {
        let _ = GetMonitorInfoW(mon, &mut mi);
    }
    mi.rcWork
}

/// Size the window for its monitor's DPI.
unsafe fn resize_for_dpi(app: &App, hwnd: HWND) {
    let dpi = GetDpiForWindow(hwnd).max(96);
    if let Ok(mut m) = app.mini.try_borrow_mut() {
        if let Some(m) = m.as_mut() {
            m.dpi = dpi;
            if let Some(s) = &m.surface {
                s.set_dpi(dpi);
            }
        }
    }
    let mut r = RECT::default();
    let _ = GetWindowRect(hwnd, &mut r);
    let _ = SetWindowPos(
        hwnd,
        Some(HWND_TOPMOST),
        r.left,
        r.top,
        scale(WIDTH, dpi),
        scale(HEIGHT, dpi),
        SWP_NOACTIVATE,
    );
}

/// Keep the pill fully on screen (and out of the taskbar). With `snap`, also
/// pull it flush against any edge it was dropped near.
unsafe fn clamp_into_work_area(app: &App, hwnd: HWND, snap: bool) {
    let mut r = RECT::default();
    let _ = GetWindowRect(hwnd, &mut r);
    let wa = work_area_of(MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST));
    let dpi = GetDpiForWindow(hwnd).max(96);
    let (w, h) = (r.right - r.left, r.bottom - r.top);
    let margin = scale(MARGIN, dpi);
    let reach = scale(SNAP, dpi);
    let (mut x, mut y) = (r.left, r.top);
    if snap {
        if x - wa.left < reach {
            x = wa.left + margin;
        }
        if wa.right - (x + w) < reach {
            x = wa.right - w - margin;
        }
        if y - wa.top < reach {
            y = wa.top + margin;
        }
        if wa.bottom - (y + h) < reach {
            y = wa.bottom - h - margin;
        }
    }
    x = x.clamp(wa.left, (wa.right - w).max(wa.left));
    y = y.clamp(wa.top, (wa.bottom - h).max(wa.top));
    if (x, y) != (r.left, r.top) {
        let _ = SetWindowPos(hwnd, None, x, y, 0, 0, SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE);
    }
    if snap {
        app.set_mini_pos(x, y);
    }
}

fn paint(app: &App, hwnd: HWND) {
    let Ok(mut guard) = app.mini.try_borrow_mut() else { return };
    let Some(m) = guard.as_mut() else { return };
    if m.surface.is_none() {
        m.surface = Surface::new(&app.gfx, hwnd, m.dpi).ok();
    }
    let Some(s) = m.surface.as_ref() else { return };
    let t = app.timer();
    let running = t.phase() == Phase::Running;
    let bg = if running { mode_color(t.mode()) } else { IDLE_GREY };
    let text = &app.gfx.text;
    unsafe { s.rt.BeginDraw() };
    s.clear(bg.d2d(1.0));
    if !running {
        // A stripe in the session's colour, so you can still tell focus from break.
        s.fill_round(rect(0.0, 0.0, 5.0, HEIGHT), 0.0, mode_color(t.mode()).d2d(1.0));
    }

    let time = format_mmss(t.remaining_ms(now_ms()));
    let cell = |c: char| if c == ':' { m.colon_w } else { m.digit_w };
    let total: f32 = time.chars().map(cell).sum();
    let area = WIDTH - 44.0;
    let mut x = 6.0 + (area - total) / 2.0;
    for c in time.chars() {
        let w = cell(c);
        s.text(&c.to_string(), &text.mini_time, rect(x, 0.0, x + w, HEIGHT), white(1.0));
        x += w;
    }

    let b = button_rect();
    if m.hover_button {
        s.fill_round(b, 16.0, white(0.2));
    }
    let glyph = if running { "\u{E769}" } else { "\u{E768}" };
    s.text(glyph, &text.glyph, b, white(0.95));

    if let Err(e) = unsafe { s.rt.EndDraw(None, None) } {
        if e.code() == D2DERR_RECREATE_TARGET {
            m.surface = None;
        }
    }
}

fn dips(app: &App, lparam: LPARAM) -> (f32, f32) {
    let dpi = app.mini.try_borrow().ok().and_then(|m| m.as_ref().map(|m| m.dpi)).unwrap_or(96);
    let (x, y) = super::util::point_from_lparam(lparam);
    let k = 96.0 / dpi as f32;
    (x as f32 * k, y as f32 * k)
}

fn with_mini<R>(app: &App, f: impl FnOnce(&mut Mini) -> R) -> Option<R> {
    app.mini.try_borrow_mut().ok()?.as_mut().map(f)
}

extern "system" fn proc_(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    unsafe {
        if msg == WM_NCCREATE {
            let cs = &*(lparam.0 as *const CREATESTRUCTW);
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, cs.lpCreateParams as isize);
        }
        let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const App;
        let Some(app) = ptr.as_ref() else {
            return DefWindowProcW(hwnd, msg, wparam, lparam);
        };
        match msg {
            WM_PAINT => {
                let mut ps = PAINTSTRUCT::default();
                BeginPaint(hwnd, &mut ps);
                paint(app, hwnd);
                let _ = EndPaint(hwnd, &ps);
                LRESULT(0)
            }
            WM_ERASEBKGND => LRESULT(1),
            WM_SIZE => {
                with_mini(app, |m| {
                    if let Some(s) = &m.surface {
                        s.resize(u32::from(loword(lparam.0 as usize)), u32::from(hiword(lparam.0 as usize)));
                    }
                });
                LRESULT(0)
            }
            // Clicking the pill must never pull focus away from what you're doing.
            WM_MOUSEACTIVATE => LRESULT(MA_NOACTIVATE as isize),
            WM_SETCURSOR if u32::from(loword(lparam.0 as usize)) == HTCLIENT => {
                if let Ok(c) = LoadCursorW(None, IDC_HAND) {
                    SetCursor(Some(c));
                }
                LRESULT(1)
            }
            WM_LBUTTONDOWN => {
                let (x, y) = dips(app, lparam);
                let mut cursor = POINT::default();
                let _ = GetCursorPos(&mut cursor);
                let mut r = RECT::default();
                let _ = GetWindowRect(hwnd, &mut r);
                with_mini(app, |m| {
                    m.drag = Some(Drag {
                        cursor,
                        window: POINT { x: r.left, y: r.top },
                        moved: false,
                        on_button: contains(&button_rect(), x, y),
                    })
                });
                SetCapture(hwnd);
                LRESULT(0)
            }
            WM_MOUSEMOVE => {
                let (x, y) = dips(app, lparam);
                let mut cursor = POINT::default();
                let _ = GetCursorPos(&mut cursor);
                let dpi = GetDpiForWindow(hwnd).max(96);
                let mut move_to = None;
                let mut start_tracking = false;
                let hover_changed = with_mini(app, |m| {
                    if let Some(d) = m.drag.as_mut() {
                        let (dx, dy) = (cursor.x - d.cursor.x, cursor.y - d.cursor.y);
                        if d.moved || dx.abs() + dy.abs() > scale(4.0, dpi) {
                            d.moved = true;
                            move_to = Some((d.window.x + dx, d.window.y + dy));
                        }
                    }
                    if !m.tracking {
                        m.tracking = true;
                        start_tracking = true;
                    }
                    let over = contains(&button_rect(), x, y);
                    let changed = over != m.hover_button;
                    m.hover_button = over;
                    changed
                })
                .unwrap_or(false);
                if start_tracking {
                    let mut tme = TRACKMOUSEEVENT {
                        cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32,
                        dwFlags: TME_LEAVE,
                        hwndTrack: hwnd,
                        dwHoverTime: 0,
                    };
                    let _ = TrackMouseEvent(&mut tme);
                }
                if let Some((nx, ny)) = move_to {
                    let _ = SetWindowPos(hwnd, None, nx, ny, 0, 0, SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE);
                }
                if hover_changed {
                    let _ = InvalidateRect(Some(hwnd), None, false);
                }
                LRESULT(0)
            }
            WM_MOUSELEAVE => {
                with_mini(app, |m| {
                    m.tracking = false;
                    m.hover_button = false;
                });
                let _ = InvalidateRect(Some(hwnd), None, false);
                LRESULT(0)
            }
            WM_LBUTTONUP => {
                let (x, y) = dips(app, lparam);
                let drag = with_mini(app, |m| m.drag.take()).flatten();
                let _ = ReleaseCapture();
                if let Some(d) = drag {
                    if d.moved {
                        clamp_into_work_area(app, hwnd, true);
                    } else if d.on_button && contains(&button_rect(), x, y) {
                        app.toggle();
                    } else {
                        app.show_main(); // restoring the main window closes the pill
                    }
                }
                LRESULT(0)
            }
            WM_RBUTTONUP => {
                let mut p = POINT::default();
                let _ = GetCursorPos(&mut p);
                app.tray_menu(p.x, p.y);
                LRESULT(0)
            }
            WM_TIMER if wparam.0 == TIMER_WATCH => {
                // Step aside for full-screen apps; come back afterwards.
                let busy = fullscreen_app_running();
                let visible = IsWindowVisible(hwnd).as_bool();
                if busy && visible {
                    let _ = ShowWindow(hwnd, SW_HIDE);
                } else if !busy && !visible {
                    let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
                    clamp_into_work_area(app, hwnd, false); // the taskbar may have moved
                }
                if !busy {
                    let _ = InvalidateRect(Some(hwnd), None, false);
                }
                LRESULT(0)
            }
            WM_DPICHANGED => {
                let dpi = u32::from(hiword(wparam.0));
                with_mini(app, |m| {
                    m.dpi = dpi;
                    if let Some(s) = &m.surface {
                        s.set_dpi(dpi);
                    }
                });
                let r = &*(lparam.0 as *const RECT);
                let _ = SetWindowPos(hwnd, None, r.left, r.top, r.right - r.left, r.bottom - r.top, SWP_NOZORDER | SWP_NOACTIVATE);
                LRESULT(0)
            }
            WM_DESTROY => {
                let _ = KillTimer(Some(hwnd), TIMER_WATCH);
                LRESULT(0)
            }
            _ => DefWindowProcW(hwnd, msg, wparam, lparam),
        }
    }
}
