//! The Settings window, built from standard Windows controls so it works with
//! the keyboard, screen readers and high-contrast themes out of the box.

use windows::core::{w, HSTRING, PCWSTR};
use windows::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    CreateFontIndirectW, DeleteObject, GetSysColorBrush, SetBkMode, COLOR_WINDOW, HDC, HFONT, TRANSPARENT,
};
use windows::Win32::UI::Controls::{
    CheckDlgButton, IsDlgButtonChecked, BST_CHECKED, BST_UNCHECKED, UDM_SETPOS32, UDM_SETRANGE32,
    UDS_ARROWKEYS, UDS_HOTTRACK, UDS_NOTHOUSANDS, UDS_SETBUDDYINT, UPDOWN_CLASSW,
};
use windows::Win32::UI::HiDpi::{GetDpiForWindow, SystemParametersInfoForDpi};
use windows::Win32::UI::Input::KeyboardAndMouse::{EnableWindow, SetFocus};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, GetDlgItem, GetDlgItemInt, GetWindowLongPtrW,
    GetWindowRect, RegisterClassExW, SendMessageW, SetDlgItemInt, SetForegroundWindow, SetWindowLongPtrW,
    SetWindowPos, ShowWindow, BS_AUTOCHECKBOX, BS_AUTORADIOBUTTON, BS_DEFPUSHBUTTON, BS_PUSHBUTTON,
    CREATESTRUCTW, ES_NUMBER, GWLP_USERDATA, HCURSOR, HICON, HMENU, IDCANCEL, IDOK, NONCLIENTMETRICSW,
    SPI_GETNONCLIENTMETRICS, SWP_NOACTIVATE, SWP_NOZORDER, SW_SHOW, SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS,
    WINDOW_EX_STYLE, WINDOW_STYLE, WM_CLOSE, WM_COMMAND, WM_CTLCOLORBTN, WM_CTLCOLORSTATIC, WM_DESTROY,
    WM_DPICHANGED, WM_NCCREATE, WM_SETFONT, WNDCLASSEXW, WS_CAPTION, WS_CHILD, WS_EX_CLIENTEDGE,
    WS_EX_DLGMODALFRAME, WS_GROUP, WS_POPUP, WS_SYSMENU, WS_TABSTOP, WS_VISIBLE,
};

use super::app::App;
use super::util::{scale, window_size};
use crate::settings::{AlertStyle, Settings, EVERY_MAX, EVERY_MIN, MINUTES_MAX, MINUTES_MIN};

const CLASS: PCWSTR = w!("WilliamsAutomation.WillPomo.Settings");
const WIDTH: f32 = 360.0;
const HEIGHT: f32 = 506.0;

const ID_FOCUS: i32 = 1001;
const ID_SHORT: i32 = 1002;
const ID_LONG: i32 = 1003;
const ID_EVERY: i32 = 1004;
const ID_AUTO_BREAKS: i32 = 1010;
const ID_AUTO_FOCUS: i32 = 1011;
const ID_ALERT_SOUND: i32 = 1020;
const ID_ALERT_SILENT: i32 = 1021;
const ID_ALERT_BOTH: i32 = 1022;
const ID_TRAY: i32 = 1030;
const ID_TOP: i32 = 1031;
const ID_MINI: i32 = 1032;

/// The dialog's window and its controls' positions (in DIPs) for re-layout on DPI changes.
pub struct Dialog {
    pub hwnd: HWND,
    controls: Vec<(HWND, [f32; 4])>,
    font: HFONT,
}

pub fn register(hinst: HINSTANCE, icon: HICON, cursor: HCURSOR) {
    let wc = WNDCLASSEXW {
        cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
        lpfnWndProc: Some(proc_),
        hInstance: hinst,
        hIcon: icon,
        hIconSm: icon,
        hCursor: cursor,
        hbrBackground: unsafe { GetSysColorBrush(COLOR_WINDOW) },
        lpszClassName: CLASS,
        ..Default::default()
    };
    unsafe {
        RegisterClassExW(&wc);
    }
}

pub fn open(app: &App) {
    if let Some(h) = app.dialog.borrow().as_ref().map(|d| d.hwnd) {
        unsafe {
            let _ = SetForegroundWindow(h);
        }
        return;
    }
    unsafe {
        let owner = app.hwnd();
        let style = WS_POPUP | WS_CAPTION | WS_SYSMENU;
        let ex = WS_EX_DLGMODALFRAME;
        let mut owner_rc = RECT::default();
        let _ = GetWindowRect(owner, &mut owner_rc);
        let Ok(hwnd) = CreateWindowExW(
            ex,
            CLASS,
            w!("WillPomo settings"),
            style,
            owner_rc.left + 20,
            owner_rc.top + 20,
            400,
            500,
            Some(owner),
            None,
            Some(app.hinst),
            Some(app as *const App as *const _),
        ) else {
            return;
        };
        let dpi = GetDpiForWindow(hwnd).max(96);
        let mut d = Dialog {
            hwnd,
            controls: Vec::new(),
            font: HFONT::default(),
        };
        build(app, &mut d);
        *app.dialog.borrow_mut() = Some(d);
        relayout(app, dpi);

        // Centre over the main window.
        let (w, h) = window_size(WIDTH, HEIGHT, style, ex, dpi);
        let x = owner_rc.left + ((owner_rc.right - owner_rc.left) - w) / 2;
        let y = owner_rc.top + ((owner_rc.bottom - owner_rc.top) - h) / 2;
        let _ = SetWindowPos(hwnd, None, x.max(0), y.max(0), w, h, SWP_NOZORDER);

        load_values(hwnd, app.settings());
        let _ = EnableWindow(owner, false); // behave like a modal dialog
        let _ = ShowWindow(hwnd, SW_SHOW);
        if let Ok(first) = GetDlgItem(Some(hwnd), ID_FOCUS) {
            let _ = SetFocus(Some(first));
        }
    }
}

pub fn close(app: &App) {
    let Some(d) = app.dialog.borrow_mut().take() else { return };
    unsafe {
        // Re-enable the owner first so Windows hands activation back to it.
        let _ = EnableWindow(app.hwnd(), true);
        let _ = DestroyWindow(d.hwnd);
        let _ = DeleteObject(d.font.into());
    }
}

#[allow(clippy::too_many_arguments)]
unsafe fn child(
    d: &mut Dialog,
    app: &App,
    class: PCWSTR,
    text: &str,
    style: u32,
    ex: WINDOW_EX_STYLE,
    id: i32,
    r: [f32; 4],
) -> HWND {
    let hwnd = CreateWindowExW(
        ex,
        class,
        &HSTRING::from(text),
        WINDOW_STYLE(style) | WS_CHILD | WS_VISIBLE,
        0,
        0,
        10,
        10,
        Some(d.hwnd),
        Some(HMENU(id as isize as *mut _)),
        Some(app.hinst),
        None,
    )
    .unwrap_or_default();
    d.controls.push((hwnd, r));
    hwnd
}

unsafe fn build(app: &App, d: &mut Dialog) {
    let x0 = 20.0;
    let x1 = WIDTH - 20.0;
    let none = WINDOW_EX_STYLE(0);
    let tab = WS_TABSTOP.0;
    let mut y = 20.0;

    let number_rows = [
        ("Focus (minutes)", ID_FOCUS, MINUTES_MIN, MINUTES_MAX),
        ("Short break (minutes)", ID_SHORT, MINUTES_MIN, MINUTES_MAX),
        ("Long break (minutes)", ID_LONG, MINUTES_MIN, MINUTES_MAX),
        ("Focus sessions before a long break", ID_EVERY, EVERY_MIN, EVERY_MAX),
    ];
    for (label, id, min, max) in number_rows {
        child(d, app, w!("STATIC"), label, 0, none, -1, [x0, y + 3.0, x1 - 90.0, y + 23.0]);
        let edit = child(
            d,
            app,
            w!("EDIT"),
            "",
            tab | ES_NUMBER as u32,
            WS_EX_CLIENTEDGE,
            id,
            [x1 - 80.0, y, x1 - 22.0, y + 26.0],
        );
        let spin = child(
            d,
            app,
            UPDOWN_CLASSW,
            "",
            UDS_SETBUDDYINT | UDS_ARROWKEYS | UDS_NOTHOUSANDS | UDS_HOTTRACK,
            none,
            id + 100,
            [x1 - 22.0, y, x1, y + 26.0],
        );
        SendMessageW(spin, windows::Win32::UI::Controls::UDM_SETBUDDY, Some(WPARAM(edit.0 as usize)), None);
        SendMessageW(spin, UDM_SETRANGE32, Some(WPARAM(min as usize)), Some(LPARAM(max as isize)));
        y += 36.0;
    }

    y += 6.0;
    child(d, app, w!("BUTTON"), "Start breaks automatically", tab | BS_AUTOCHECKBOX as u32, none, ID_AUTO_BREAKS, [x0, y, x1, y + 24.0]);
    y += 28.0;
    child(d, app, w!("BUTTON"), "Start focus sessions automatically", tab | BS_AUTOCHECKBOX as u32, none, ID_AUTO_FOCUS, [x0, y, x1, y + 24.0]);
    y += 40.0;

    child(d, app, w!("STATIC"), "When a session ends:", 0, none, -1, [x0, y, x1, y + 20.0]);
    y += 26.0;
    let radios = [
        ("Play a sound", ID_ALERT_SOUND),
        ("Silent: pop up and take focus", ID_ALERT_SILENT),
        ("Sound and pop-up", ID_ALERT_BOTH),
    ];
    for (i, (label, id)) in radios.into_iter().enumerate() {
        let group = if i == 0 { WS_GROUP.0 | tab } else { 0 };
        child(d, app, w!("BUTTON"), label, group | BS_AUTORADIOBUTTON as u32, none, id, [x0 + 12.0, y, x1, y + 24.0]);
        y += 26.0;
    }
    y += 14.0;

    child(d, app, w!("BUTTON"), "Show minutes left in the notification area", WS_GROUP.0 | tab | BS_AUTOCHECKBOX as u32, none, ID_TRAY, [x0, y, x1, y + 24.0]);
    y += 28.0;
    child(d, app, w!("BUTTON"), "Keep WillPomo on top of other windows", tab | BS_AUTOCHECKBOX as u32, none, ID_TOP, [x0, y, x1, y + 24.0]);
    y += 28.0;
    child(d, app, w!("BUTTON"), "When minimised, show a mini timer above the taskbar", tab | BS_AUTOCHECKBOX as u32, none, ID_MINI, [x0, y, x1, y + 24.0]);

    let by = HEIGHT - 20.0 - 30.0;
    child(d, app, w!("BUTTON"), "Save", WS_GROUP.0 | tab | BS_DEFPUSHBUTTON as u32, none, IDOK.0, [x1 - 186.0, by, x1 - 98.0, by + 30.0]);
    child(d, app, w!("BUTTON"), "Cancel", tab | BS_PUSHBUTTON as u32, none, IDCANCEL.0, [x1 - 88.0, by, x1, by + 30.0]);
}

/// Position every control and set the font for the given DPI.
fn relayout(app: &App, dpi: u32) {
    let mut guard = app.dialog.borrow_mut();
    let Some(d) = guard.as_mut() else { return };
    unsafe {
        let mut ncm = NONCLIENTMETRICSW {
            cbSize: std::mem::size_of::<NONCLIENTMETRICSW>() as u32,
            ..Default::default()
        };
        let font = if SystemParametersInfoForDpi(
            SPI_GETNONCLIENTMETRICS.0,
            ncm.cbSize,
            Some(&mut ncm as *mut _ as *mut _),
            SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0).0,
            dpi,
        )
        .is_ok()
        {
            CreateFontIndirectW(&ncm.lfMessageFont)
        } else {
            HFONT::default()
        };
        for (hwnd, [l, t, r, b]) in &d.controls {
            let _ = SetWindowPos(
                *hwnd,
                None,
                scale(*l, dpi),
                scale(*t, dpi),
                scale(r - l, dpi),
                scale(b - t, dpi),
                SWP_NOZORDER | SWP_NOACTIVATE,
            );
            SendMessageW(*hwnd, WM_SETFONT, Some(WPARAM(font.0 as usize)), Some(LPARAM(1)));
        }
        let old = std::mem::replace(&mut d.font, font);
        if !old.is_invalid() {
            let _ = DeleteObject(old.into());
        }
    }
}

unsafe fn load_values(hwnd: HWND, s: Settings) {
    for (id, v) in [
        (ID_FOCUS, s.focus_minutes),
        (ID_SHORT, s.short_break_minutes),
        (ID_LONG, s.long_break_minutes),
        (ID_EVERY, s.long_break_every),
    ] {
        let _ = SetDlgItemInt(hwnd, id, v, false);
        if let Ok(spin) = GetDlgItem(Some(hwnd), id + 100) {
            SendMessageW(spin, UDM_SETPOS32, None, Some(LPARAM(v as isize)));
        }
    }
    let check = |id: i32, on: bool| {
        let _ = CheckDlgButton(hwnd, id, if on { BST_CHECKED } else { BST_UNCHECKED });
    };
    check(ID_AUTO_BREAKS, s.auto_start_breaks);
    check(ID_AUTO_FOCUS, s.auto_start_focus);
    check(ID_ALERT_SOUND, s.alert == AlertStyle::Sound);
    check(ID_ALERT_SILENT, s.alert == AlertStyle::SilentPopup);
    check(ID_ALERT_BOTH, s.alert == AlertStyle::SoundAndPopup);
    check(ID_TRAY, s.tray_icon);
    check(ID_TOP, s.always_on_top);
    check(ID_MINI, s.mini_mode);
}

unsafe fn read_values(hwnd: HWND, current: Settings) -> Settings {
    let num = |id: i32, fallback: u32| {
        let mut ok = windows::core::BOOL::default();
        let v = GetDlgItemInt(hwnd, id, Some(&mut ok), false);
        if ok.as_bool() {
            v
        } else {
            fallback
        }
    };
    let checked = |id: i32| IsDlgButtonChecked(hwnd, id) == BST_CHECKED.0;
    let alert = if checked(ID_ALERT_SILENT) {
        AlertStyle::SilentPopup
    } else if checked(ID_ALERT_BOTH) {
        AlertStyle::SoundAndPopup
    } else {
        AlertStyle::Sound
    };
    Settings {
        focus_minutes: num(ID_FOCUS, current.focus_minutes),
        short_break_minutes: num(ID_SHORT, current.short_break_minutes),
        long_break_minutes: num(ID_LONG, current.long_break_minutes),
        long_break_every: num(ID_EVERY, current.long_break_every),
        auto_start_breaks: checked(ID_AUTO_BREAKS),
        auto_start_focus: checked(ID_AUTO_FOCUS),
        alert,
        tray_icon: checked(ID_TRAY),
        always_on_top: checked(ID_TOP),
        mini_mode: checked(ID_MINI),
    }
    .clamped()
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
            WM_COMMAND => {
                let id = (wparam.0 & 0xFFFF) as i32;
                if id == IDOK.0 {
                    let new = read_values(hwnd, app.settings());
                    close(app);
                    app.apply_settings(new);
                } else if id == IDCANCEL.0 {
                    close(app);
                }
                LRESULT(0)
            }
            WM_CLOSE => {
                close(app);
                LRESULT(0)
            }
            // White background behind labels, checkboxes and radio buttons.
            WM_CTLCOLORSTATIC | WM_CTLCOLORBTN => {
                SetBkMode(HDC(wparam.0 as *mut _), TRANSPARENT);
                LRESULT(GetSysColorBrush(COLOR_WINDOW).0 as isize)
            }
            WM_DPICHANGED => {
                relayout(app, u32::from((wparam.0 >> 16) as u16));
                let r = &*(lparam.0 as *const RECT);
                let _ = SetWindowPos(hwnd, None, r.left, r.top, r.right - r.left, r.bottom - r.top, SWP_NOZORDER | SWP_NOACTIVATE);
                LRESULT(0)
            }
            WM_DESTROY => LRESULT(0),
            _ => DefWindowProcW(hwnd, msg, wparam, lparam),
        }
    }
}
