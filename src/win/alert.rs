//! The end-of-session pop-up: a small always-on-top window that takes focus,
//! so you notice the session ended even with the sound off.
//!
//! Windows normally stops background apps from stealing focus. We use the
//! well-known escalation: plain SetForegroundWindow, then briefly attaching to
//! the foreground thread's input queue, then a synthetic Alt key press. The
//! window is topmost either way, so it's visible even if focus can't be taken.
//!
//! Because the pop-up can appear mid-typing, its buttons ignore input for the
//! first moment (`GUARD_MS`) so a stray Space or Enter can't click them.

use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Direct2D::Common::D2D_RECT_F;
use windows::Win32::Foundation::D2DERR_RECREATE_TARGET;
use windows::Win32::Graphics::Gdi::{
    BeginPaint, EndPaint, GetMonitorInfoW, InvalidateRect, MonitorFromWindow, HBRUSH, MONITORINFO,
    MONITOR_DEFAULTTONEAREST, PAINTSTRUCT,
};
use windows::Win32::System::Threading::{AttachThreadInput, GetCurrentThreadId};
use windows::Win32::UI::HiDpi::GetDpiForWindow;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    ReleaseCapture, SendInput, SetCapture, SetFocus, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT,
    KEYBD_EVENT_FLAGS, KEYEVENTF_KEYUP, VIRTUAL_KEY, VK_ESCAPE, VK_MENU, VK_RETURN, VK_SPACE,
};
use windows::Win32::UI::Shell::{
    SHQueryUserNotificationState, QUNS_PRESENTATION_MODE, QUNS_RUNNING_D3D_FULL_SCREEN,
};
use windows::Win32::UI::WindowsAndMessaging::{
    BringWindowToTop, CreateWindowExW, DefWindowProcW, DestroyWindow, GetForegroundWindow,
    GetWindowLongPtrW, GetWindowThreadProcessId, KillTimer, RegisterClassExW, SetForegroundWindow,
    SetTimer, SetWindowLongPtrW, SetWindowPos, ShowWindow, CREATESTRUCTW, GWLP_USERDATA, HCURSOR,
    HICON, HWND_TOPMOST, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_SHOWWINDOW, SW_SHOWNOACTIVATE,
    WM_CLOSE, WM_DESTROY, WM_DPICHANGED, WM_ERASEBKGND, WM_KEYDOWN, WM_LBUTTONDOWN, WM_LBUTTONUP,
    WM_MOUSEMOVE, WM_NCCREATE, WM_PAINT, WM_SIZE, WM_SYSKEYUP, WM_TIMER, WNDCLASSEXW, WS_CAPTION,
    WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP, WS_SYSMENU,
};

use super::app::App;
use super::gfx::Surface;
use super::util::{contains, hiword, loword, now_ms, point_from_lparam, rect, set_title_bar_color, white, window_size, Rgb};

const CLASS: PCWSTR = w!("WilliamsAutomation.Pomobar.Alert");
const WIDTH: f32 = 420.0;
const HEIGHT: f32 = 190.0;
const GUARD_MS: u64 = 900;
const TIMER_GUARD: usize = 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    StartNext,
    Pause,
    Dismiss,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Button {
    Primary,
    Secondary,
}

pub struct Content {
    pub heading: String,
    pub body: String,
    pub primary: (&'static str, Action),
    pub secondary: (&'static str, Action),
    pub bg: Rgb,
}

pub struct Popup {
    hwnd: HWND,
    content: Content,
    shown_at: u64,
    hover: Option<Button>,
    pressed: Option<Button>,
    surface: Option<Surface>,
    dpi: u32,
}

pub fn register(hinst: HINSTANCE, icon: HICON, cursor: HCURSOR) {
    let wc = WNDCLASSEXW {
        cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
        lpfnWndProc: Some(proc_),
        hInstance: hinst,
        hIcon: icon,
        hIconSm: icon,
        hCursor: cursor,
        hbrBackground: HBRUSH::default(),
        lpszClassName: CLASS,
        ..Default::default()
    };
    unsafe {
        RegisterClassExW(&wc);
    }
}

/// Show the pop-up (replacing any existing one).
pub fn show(app: &App, content: Content, take_focus: bool) {
    close(app);
    unsafe {
        // Appear on the monitor the user is working on, which may not be Pomobar's.
        let fg = GetForegroundWindow();
        let anchor = if fg.is_invalid() { app.hwnd() } else { fg };
        let mut mi = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        let _ = GetMonitorInfoW(MonitorFromWindow(anchor, MONITOR_DEFAULTTONEAREST), &mut mi);
        let wa = mi.rcWork;
        let style = WS_POPUP | WS_CAPTION | WS_SYSMENU;
        let ex = WS_EX_TOPMOST | WS_EX_TOOLWINDOW;
        let Ok(hwnd) = CreateWindowExW(
            ex,
            CLASS,
            w!("Pomobar"),
            style,
            wa.left + (wa.right - wa.left) / 2 - 200,
            wa.top + (wa.bottom - wa.top) / 2 - 100,
            400,
            200,
            None,
            None,
            Some(app.hinst),
            Some(app as *const App as *const _),
        ) else {
            return;
        };
        let dpi = GetDpiForWindow(hwnd).max(96);
        let (w, h) = window_size(WIDTH, HEIGHT, style, ex, dpi);
        let x = wa.left + (wa.right - wa.left - w) / 2;
        let y = wa.top + (wa.bottom - wa.top - h) / 2;
        let _ = SetWindowPos(hwnd, Some(HWND_TOPMOST), x, y, w, h, SWP_NOACTIVATE);
        set_title_bar_color(hwnd, content.bg);
        *app.popup.borrow_mut() = Some(Popup {
            hwnd,
            content,
            shown_at: now_ms(),
            hover: None,
            pressed: None,
            surface: None,
            dpi,
        });
        SetTimer(Some(hwnd), TIMER_GUARD, GUARD_MS as u32 + 20, None);

        // Don't grab focus from a full-screen game or a presentation.
        let busy = matches!(
            SHQueryUserNotificationState(),
            Ok(s) if s == QUNS_RUNNING_D3D_FULL_SCREEN || s == QUNS_PRESENTATION_MODE
        );
        if take_focus && !busy {
            force_foreground(hwnd);
        } else {
            let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
        }
    }
}

pub fn close(app: &App) {
    let hwnd = app.popup.borrow().as_ref().map(|p| p.hwnd);
    if let Some(hwnd) = hwnd {
        unsafe {
            let _ = DestroyWindow(hwnd);
        }
    }
    *app.popup.borrow_mut() = None;
}

/// Bring `hwnd` to the front and give it keyboard focus, escalating as needed.
unsafe fn force_foreground(hwnd: HWND) -> bool {
    let _ = SetWindowPos(hwnd, Some(HWND_TOPMOST), 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_SHOWWINDOW);
    if SetForegroundWindow(hwnd).as_bool() && GetForegroundWindow() == hwnd {
        return true;
    }

    let fg = GetForegroundWindow();
    let fg_thread = if fg.is_invalid() { 0 } else { GetWindowThreadProcessId(fg, None) };
    let me = GetCurrentThreadId();
    let attached = fg_thread != 0 && fg_thread != me && AttachThreadInput(me, fg_thread, true).as_bool();
    let _ = BringWindowToTop(hwnd);
    let _ = SetForegroundWindow(hwnd);
    let _ = SetFocus(Some(hwnd));
    if attached {
        let _ = AttachThreadInput(me, fg_thread, false);
    }
    if GetForegroundWindow() == hwnd {
        return true;
    }

    // Last resort: a synthetic Alt press lifts the foreground lock. The key-up
    // lands on our window (which swallows it), so no menu opens anywhere.
    let key = |flags: KEYBD_EVENT_FLAGS| INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: VK_MENU,
                wScan: 0,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    };
    let size = std::mem::size_of::<INPUT>() as i32;
    SendInput(&[key(KEYBD_EVENT_FLAGS(0))], size);
    let _ = SetForegroundWindow(hwnd);
    SendInput(&[key(KEYEVENTF_KEYUP)], size);
    GetForegroundWindow() == hwnd
}

struct Rects {
    heading: D2D_RECT_F,
    body: D2D_RECT_F,
    primary: D2D_RECT_F,
    secondary: D2D_RECT_F,
}

fn rects() -> Rects {
    let pad = 26.0;
    let bh = 42.0;
    let primary = rect(WIDTH - pad - 150.0, HEIGHT - pad - bh, WIDTH - pad, HEIGHT - pad);
    let secondary = rect(primary.left - 12.0 - 116.0, primary.top, primary.left - 12.0, primary.bottom);
    Rects {
        heading: rect(pad, 20.0, WIDTH - pad, 56.0),
        body: rect(pad, 56.0, WIDTH - pad, 108.0),
        primary,
        secondary,
    }
}

fn hit(x: f32, y: f32) -> Option<Button> {
    let r = rects();
    if contains(&r.primary, x, y) {
        Some(Button::Primary)
    } else if contains(&r.secondary, x, y) {
        Some(Button::Secondary)
    } else {
        None
    }
}

fn paint(app: &App, hwnd: HWND) {
    let Ok(mut guard) = app.popup.try_borrow_mut() else { return };
    let Some(p) = guard.as_mut() else { return };
    if p.surface.is_none() {
        p.surface = Surface::new(&app.gfx, hwnd, p.dpi).ok();
    }
    let Some(s) = p.surface.as_ref() else { return };
    let ready = now_ms().saturating_sub(p.shown_at) >= GUARD_MS;
    let t = &app.gfx.text;
    let r = rects();
    let bg = p.content.bg;
    unsafe { s.rt.BeginDraw() };
    s.clear(bg.d2d(1.0));
    s.text(&p.content.heading, &t.heading, r.heading, white(1.0));
    s.text(&p.content.body, &t.body, r.body, white(0.92));

    let fade = if ready { 1.0 } else { 0.5 };
    let pressed = |b| ready && p.pressed == Some(b) && p.hover == Some(b);
    let hovered = |b| ready && p.hover == Some(b);

    let mut pr = r.primary;
    if pressed(Button::Primary) {
        pr = super::util::offset(pr, 0.0, 2.0);
    }
    s.fill_round(pr, 10.0, white(fade * if hovered(Button::Primary) { 0.94 } else { 1.0 }));
    s.text(p.content.primary.0, &t.popup_button, pr, bg.d2d(fade));

    if hovered(Button::Secondary) {
        s.fill_round(r.secondary, 10.0, white(if pressed(Button::Secondary) { 0.22 } else { 0.12 }));
    }
    s.stroke_round(r.secondary, 10.0, 1.5, white(0.85 * fade));
    s.text(p.content.secondary.0, &t.popup_button, r.secondary, white(fade));

    if let Err(e) = unsafe { s.rt.EndDraw(None, None) } {
        if e.code() == D2DERR_RECREATE_TARGET {
            p.surface = None;
        }
    }
}

fn act(app: &App, button: Button) {
    let action = {
        let guard = app.popup.borrow();
        let Some(p) = guard.as_ref() else { return };
        if now_ms().saturating_sub(p.shown_at) < GUARD_MS {
            return;
        }
        match button {
            Button::Primary => p.content.primary.1,
            Button::Secondary => p.content.secondary.1,
        }
    };
    app.on_alert_action(action);
}

fn to_dips(app: &App, lparam: LPARAM) -> (f32, f32) {
    let dpi = app.popup.borrow().as_ref().map_or(96, |p| p.dpi);
    let (x, y) = point_from_lparam(lparam);
    let k = 96.0 / dpi as f32;
    (x as f32 * k, y as f32 * k)
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
        // Ignore stray messages once this pop-up has been replaced or closed.
        let mine = app
            .popup
            .try_borrow()
            .map(|g| g.as_ref().is_some_and(|p| p.hwnd == hwnd))
            .unwrap_or(false);
        match msg {
            WM_PAINT => {
                let mut ps = PAINTSTRUCT::default();
                BeginPaint(hwnd, &mut ps);
                if mine {
                    paint(app, hwnd);
                }
                let _ = EndPaint(hwnd, &ps);
                LRESULT(0)
            }
            WM_ERASEBKGND => LRESULT(1),
            WM_SIZE => {
                if let Some(s) = app.popup.borrow().as_ref().and_then(|p| p.surface.as_ref()) {
                    s.resize(u32::from(loword(lparam.0 as usize)), u32::from(hiword(lparam.0 as usize)));
                }
                LRESULT(0)
            }
            WM_DPICHANGED => {
                let dpi = u32::from(hiword(wparam.0));
                if let Some(p) = app.popup.borrow_mut().as_mut() {
                    p.dpi = dpi;
                    if let Some(s) = &p.surface {
                        s.set_dpi(dpi);
                    }
                }
                let r = &*(lparam.0 as *const RECT);
                let _ = SetWindowPos(hwnd, None, r.left, r.top, r.right - r.left, r.bottom - r.top, SWP_NOACTIVATE);
                LRESULT(0)
            }
            WM_TIMER if wparam.0 == TIMER_GUARD => {
                let _ = KillTimer(Some(hwnd), TIMER_GUARD);
                let _ = InvalidateRect(Some(hwnd), None, false);
                LRESULT(0)
            }
            WM_MOUSEMOVE if mine => {
                let (x, y) = to_dips(app, lparam);
                let h = hit(x, y);
                let changed = app.popup.borrow_mut().as_mut().is_some_and(|p| {
                    let c = p.hover != h;
                    p.hover = h;
                    c
                });
                if changed {
                    let _ = InvalidateRect(Some(hwnd), None, false);
                }
                LRESULT(0)
            }
            WM_LBUTTONDOWN if mine => {
                let (x, y) = to_dips(app, lparam);
                let h = hit(x, y);
                if let Some(p) = app.popup.borrow_mut().as_mut() {
                    p.pressed = h;
                }
                if h.is_some() {
                    SetCapture(hwnd);
                }
                let _ = InvalidateRect(Some(hwnd), None, false);
                LRESULT(0)
            }
            WM_LBUTTONUP if mine => {
                let (x, y) = to_dips(app, lparam);
                let h = hit(x, y);
                let pressed = app.popup.borrow_mut().as_mut().and_then(|p| p.pressed.take());
                let _ = ReleaseCapture();
                let _ = InvalidateRect(Some(hwnd), None, false);
                if let Some(b) = h.filter(|b| Some(*b) == pressed) {
                    act(app, b);
                }
                LRESULT(0)
            }
            WM_KEYDOWN if mine => {
                match VIRTUAL_KEY(wparam.0 as u16) {
                    VK_RETURN | VK_SPACE => act(app, Button::Primary),
                    VK_ESCAPE => act(app, Button::Secondary),
                    _ => {}
                }
                LRESULT(0)
            }
            // Swallow the Alt key-up from force_foreground so it doesn't open the window menu.
            WM_SYSKEYUP if VIRTUAL_KEY(wparam.0 as u16) == VK_MENU => LRESULT(0),
            WM_CLOSE => {
                if mine {
                    app.on_alert_action(Action::Dismiss);
                } else {
                    let _ = DestroyWindow(hwnd);
                }
                LRESULT(0)
            }
            WM_DESTROY => {
                let _ = KillTimer(Some(hwnd), TIMER_GUARD);
                LRESULT(0)
            }
            _ => DefWindowProcW(hwnd, msg, wparam, lparam),
        }
    }
}
