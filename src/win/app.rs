//! The main window, and the glue between the timer, the UI, the taskbar, the
//! tray icon, sounds and the pop-up.
//!
//! State lives in an `App` that is created once and leaked, so window
//! procedures can reach it through a plain pointer. Fields use `Cell`/`RefCell`
//! and borrows are kept short, so re-entrant window messages are always safe.

use std::cell::{Cell, RefCell};
use std::path::PathBuf;

use windows::core::{w, HSTRING, PCWSTR};
use windows::Win32::Foundation::{
    GetLastError, ERROR_ALREADY_EXISTS, HINSTANCE, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM,
};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, EndPaint, InvalidateRect, MonitorFromPoint, ScreenToClient, HBRUSH, MONITOR_DEFAULTTONULL,
    PAINTSTRUCT,
};
use windows::Win32::System::Com::{CoInitializeEx, COINIT_APARTMENTTHREADED, COINIT_DISABLE_OLE1DDE};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Threading::CreateMutexW;
use windows::Win32::UI::Controls::{
    WM_MOUSELEAVE, InitCommonControlsEx, LoadIconMetric, ICC_STANDARD_CLASSES, ICC_UPDOWN_CLASS, INITCOMMONCONTROLSEX,
    LIM_LARGE, LIM_SMALL,
};
use windows::Win32::UI::HiDpi::{
    GetDpiForSystem, GetDpiForWindow, GetSystemMetricsForDpi, SetProcessDpiAwarenessContext,
    DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetKeyState, ReleaseCapture, SetCapture, TrackMouseEvent, TME_LEAVE, TRACKMOUSEEVENT, VIRTUAL_KEY,
    VK_CONTROL, VK_OEM_COMMA, VK_RETURN, VK_SPACE,
};
use windows::Win32::UI::Shell::{NINF_KEY, NIN_SELECT, THBN_CLICKED};

const NIN_KEYSELECT: u32 = NIN_SELECT | NINF_KEY;
use windows::Win32::UI::WindowsAndMessaging::{
    SIZE_MINIMIZED, SIZE_RESTORED, AllowSetForegroundWindow, AppendMenuW, CreatePopupMenu, CreateWindowExW, DefWindowProcW,
    DestroyMenu, DestroyWindow, DispatchMessageW, FindWindowW, FlashWindowEx, GetForegroundWindow,
    GetCursorPos, GetMessageW, GetWindowLongPtrW, GetWindowRect, GetWindowThreadProcessId, IsDialogMessageW, IsIconic,
    IsWindowVisible, KillTimer, LoadCursorW, LoadIconW, MessageBoxW, PostMessageW, PostQuitMessage,
    RegisterClassExW, RegisterWindowMessageW, SetCursor, SetForegroundWindow, SetTimer, SetWindowLongPtrW,
    SetWindowPos, SetWindowTextW, ShowWindow, TrackPopupMenu, TranslateMessage, CREATESTRUCTW,
    CW_USEDEFAULT, FLASHWINFO, FLASHW_STOP, FLASHW_TIMERNOFG, FLASHW_TRAY, GWLP_USERDATA, HICON,
    HTCLIENT, HWND_NOTOPMOST, HWND_TOPMOST, IDC_ARROW, IDC_HAND, IDI_APPLICATION, IDYES, MB_ICONERROR,
    MB_ICONQUESTION, MB_OK, MB_YESNO, MF_SEPARATOR, MF_STRING, MSG, SM_CXSMICON, SWP_NOACTIVATE,
    SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER, SW_MINIMIZE, SW_RESTORE, SW_SHOW, TPM_NONOTIFY, TPM_RETURNCMD,
    TPM_RIGHTBUTTON, WINDOW_EX_STYLE, WM_APP, WM_CLOSE, WM_COMMAND, WM_CONTEXTMENU, WM_DESTROY,
    WM_DPICHANGED, WM_ERASEBKGND, WM_KEYDOWN, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MOUSEMOVE,
    WM_MOVE, WM_NCCREATE, WM_NULL, WM_PAINT, WM_SETCURSOR, WM_SIZE, WM_TIMER, WNDCLASSEXW, WS_CAPTION,
    WS_EX_TOPMOST, WS_MINIMIZEBOX, WS_OVERLAPPED, WS_SYSMENU,
};

use super::alert::{self, Action, Content};
use super::gfx::{Gfx, Surface};
use super::icons::{self, Shape};
use super::mini;
use super::settings_dialog;
use super::sound;
use super::taskbar::{Taskbar, ThumbIcons, BTN_RESET, BTN_SKIP, BTN_TOGGLE};
use super::tray::{self, WM_TRAY};
use super::ui::{self, Hit, Layout, View};
use super::util::{
    hiword, loword, mode_color, now_ms, point_from_lparam, set_title_bar_color, today, window_size,
    work_area, IDLE_GREY,
};
use crate::settings::{self, AlertStyle, Settings, Stored};
use crate::timer::{format_mmss, Completion, Mode, Phase, Timer};

const MAIN_CLASS: PCWSTR = w!("WilliamsAutomation.WillPomo.Main");
/// Posted by a second copy of the app, asking this one to show itself.
const WM_APP_ACTIVATE: u32 = WM_APP + 2;
const TIMER_TICK: usize = 1;

const CMD_TOGGLE: u16 = 201;
const CMD_SKIP: u16 = 202;
const CMD_RESET: u16 = 203;
const CMD_SHOW: u16 = 204;
const CMD_SETTINGS: u16 = 205;
const CMD_EXIT: u16 = 206;

pub struct App {
    pub hinst: HINSTANCE,
    pub gfx: Gfx,
    layout: Layout,
    demo: bool,
    store_path: Option<PathBuf>,
    hwnd: Cell<HWND>,
    dpi: Cell<u32>,
    timer: Cell<Timer>,
    window_pos: Cell<Option<(i32, i32)>>,
    mini_pos: Cell<Option<(i32, i32)>>,
    stats_date: RefCell<String>,
    surface: RefCell<Option<Surface>>,
    hover: Cell<Option<Hit>>,
    pressed: Cell<Option<Hit>>,
    tracking_mouse: Cell<bool>,
    taskbar: RefCell<Option<Taskbar>>,
    thumb_icons: ThumbIcons,
    tray_added: Cell<bool>,
    overlay_icon: Cell<Option<HICON>>,
    tray_icon: Cell<Option<HICON>>,
    badge_key: Cell<Option<(Mode, Phase, u64)>>,
    shown_running: Cell<Option<bool>>,
    shown_mode: Cell<Option<Mode>>,
    last_title: RefCell<String>,
    pub popup: RefCell<Option<alert::Popup>>,
    pub dialog: RefCell<Option<settings_dialog::Dialog>>,
    pub mini: RefCell<Option<mini::Mini>>,
    msg_taskbar_created: u32,
    msg_button_created: u32,
}

pub fn run() {
    unsafe {
        // The embedded manifest normally does this; this covers builds without it.
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE);
        let icc = INITCOMMONCONTROLSEX {
            dwSize: std::mem::size_of::<INITCOMMONCONTROLSEX>() as u32,
            dwICC: ICC_STANDARD_CLASSES | ICC_UPDOWN_CLASS,
        };
        let _ = InitCommonControlsEx(&icc);
    }

    // --demo: "minutes" become seconds and the pop-up is on, for trying things out quickly.
    let demo = std::env::args().any(|a| a == "--demo");
    if !demo && activate_existing_instance() {
        return;
    }

    let hinst = unsafe { GetModuleHandleW(None) }.map(|m| HINSTANCE(m.0)).unwrap_or_default();
    let gfx = match Gfx::new() {
        Ok(g) => g,
        Err(e) => return fatal(&format!("WillPomo couldn't start its graphics.\n\n{e}")),
    };

    let store_path = settings::default_path();
    let stored = store_path.as_deref().map(settings::load).unwrap_or_default();
    let date = today();
    let completed = if stored.stats_date.as_deref() == Some(date.as_str()) {
        stored.stats_completed
    } else {
        0
    };
    let mut initial = stored.settings;
    if demo {
        initial.alert = AlertStyle::SoundAndPopup;
    }

    let small = small_icon_size();
    let app: &'static App = Box::leak(Box::new(App {
        hinst,
        layout: Layout::new(&gfx),
        gfx,
        demo,
        store_path,
        hwnd: Cell::new(HWND::default()),
        dpi: Cell::new(96),
        timer: Cell::new(Timer::new(initial, completed, if demo { 1_000 } else { 60_000 })),
        window_pos: Cell::new(stored.window_pos),
        mini_pos: Cell::new(stored.mini_pos),
        stats_date: RefCell::new(date),
        surface: RefCell::new(None),
        hover: Cell::new(None),
        pressed: Cell::new(None),
        tracking_mouse: Cell::new(false),
        taskbar: RefCell::new(None),
        thumb_icons: ThumbIcons {
            play: icons::glyph(small, "\u{E768}"),
            pause: icons::glyph(small, "\u{E769}"),
            skip: icons::glyph(small, ui::GLYPH_SKIP),
            reset: icons::glyph(small, ui::GLYPH_RESET),
        },
        tray_added: Cell::new(false),
        overlay_icon: Cell::new(None),
        tray_icon: Cell::new(None),
        badge_key: Cell::new(None),
        shown_running: Cell::new(None),
        shown_mode: Cell::new(None),
        last_title: RefCell::new(String::new()),
        popup: RefCell::new(None),
        dialog: RefCell::new(None),
        mini: RefCell::new(None),
        msg_taskbar_created: unsafe { RegisterWindowMessageW(w!("TaskbarCreated")) },
        msg_button_created: unsafe { RegisterWindowMessageW(w!("TaskbarButtonCreated")) },
    }));

    unsafe {
        register_classes(hinst);
        if !create_main_window(app, stored.window_pos) {
            return fatal("WillPomo couldn't create its window.");
        }
    }
    if demo {
        app.start();
    }

    unsafe {
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).0 > 0 {
            // Lets Tab, Enter and Esc work in the Settings window.
            let dialog = app.dialog.try_borrow().ok().and_then(|d| d.as_ref().map(|d| d.hwnd));
            if let Some(dlg) = dialog {
                if IsDialogMessageW(dlg, &msg).as_bool() {
                    continue;
                }
            }
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}

fn fatal(text: &str) {
    unsafe {
        MessageBoxW(None, &HSTRING::from(text), w!("WillPomo"), MB_OK | MB_ICONERROR);
    }
}

/// If WillPomo is already running, bring it forward and return true.
fn activate_existing_instance() -> bool {
    unsafe {
        // Deliberately never closed: it marks this process as the running copy.
        let _mutex = CreateMutexW(None, false, w!("Local\\WilliamsAutomation.WillPomo"));
        if GetLastError() != ERROR_ALREADY_EXISTS {
            return false;
        }
        if let Ok(existing) = FindWindowW(MAIN_CLASS, PCWSTR::null()) {
            let mut pid = 0u32;
            GetWindowThreadProcessId(existing, Some(&mut pid));
            let _ = AllowSetForegroundWindow(pid);
            let _ = PostMessageW(Some(existing), WM_APP_ACTIVATE, WPARAM(0), LPARAM(0));
        }
        true
    }
}

/// Small-icon size (16 px at 100% scaling) for the taskbar's DPI.
fn small_icon_size() -> i32 {
    unsafe { GetSystemMetricsForDpi(SM_CXSMICON, GetDpiForSystem()).max(16) }
}

unsafe fn register_classes(hinst: HINSTANCE) {
    // Icon resource 1 is embedded by build.rs (MAKEINTRESOURCE(1)).
    let res = PCWSTR(std::ptr::without_provenance(1));
    let big = LoadIconMetric(Some(hinst), res, LIM_LARGE)
        .or_else(|_| LoadIconW(None, IDI_APPLICATION))
        .unwrap_or_default();
    let small = LoadIconMetric(Some(hinst), res, LIM_SMALL).unwrap_or(big);
    let cursor = LoadCursorW(None, IDC_ARROW).unwrap_or_default();
    let wc = WNDCLASSEXW {
        cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
        lpfnWndProc: Some(main_proc),
        hInstance: hinst,
        hIcon: big,
        hIconSm: small,
        hCursor: cursor,
        hbrBackground: HBRUSH::default(),
        lpszClassName: MAIN_CLASS,
        ..Default::default()
    };
    RegisterClassExW(&wc);
    alert::register(hinst, small, cursor);
    settings_dialog::register(hinst, small, cursor);
    mini::register(hinst, cursor);
}

unsafe fn create_main_window(app: &'static App, saved_pos: Option<(i32, i32)>) -> bool {
    let style = WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_MINIMIZEBOX;
    let ex = if app.settings().always_on_top {
        WS_EX_TOPMOST
    } else {
        WINDOW_EX_STYLE(0)
    };
    let Ok(hwnd) = CreateWindowExW(
        ex,
        MAIN_CLASS,
        w!("WillPomo"),
        style,
        CW_USEDEFAULT,
        CW_USEDEFAULT,
        400,
        340,
        None,
        None,
        Some(app.hinst),
        Some(app as *const App as *const _),
    ) else {
        return false;
    };
    let dpi = GetDpiForWindow(hwnd).max(96);
    app.dpi.set(dpi);
    let (w, h) = window_size(ui::WIDTH, ui::HEIGHT, style, ex, dpi);
    // Reopen where it was last time, if that spot is still on a screen.
    let visible = |&(x, y): &(i32, i32)| {
        let top_middle = POINT { x: x + w / 2, y: y + 8 };
        !MonitorFromPoint(top_middle, MONITOR_DEFAULTTONULL).is_invalid()
    };
    let (x, y) = match saved_pos.filter(visible) {
        Some(p) => p,
        None => {
            let wa = work_area(hwnd);
            (wa.left + (wa.right - wa.left - w) / 2, wa.top + (wa.bottom - wa.top - h) / 2)
        }
    };
    let _ = SetWindowPos(hwnd, None, x, y, w, h, SWP_NOZORDER | SWP_NOACTIVATE);
    // If that put the window on a monitor with different scaling, size it for that monitor.
    let moved_dpi = GetDpiForWindow(hwnd).max(96);
    if moved_dpi != dpi {
        app.dpi.set(moved_dpi);
        let (w, h) = window_size(ui::WIDTH, ui::HEIGHT, style, ex, moved_dpi);
        let _ = SetWindowPos(hwnd, None, 0, 0, w, h, SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE);
    }
    if app.settings().tray_icon {
        app.add_tray();
    }
    app.refresh();
    let _ = ShowWindow(hwnd, SW_SHOW);
    true
}

extern "system" fn main_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    unsafe {
        if msg == WM_NCCREATE {
            let cs = &*(lparam.0 as *const CREATESTRUCTW);
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, cs.lpCreateParams as isize);
        }
        let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const App;
        match ptr.as_ref() {
            Some(app) => {
                if msg == WM_NCCREATE {
                    app.hwnd.set(hwnd);
                }
                app.handle(hwnd, msg, wparam, lparam)
            }
            None => DefWindowProcW(hwnd, msg, wparam, lparam),
        }
    }
}

impl App {
    pub fn hwnd(&self) -> HWND {
        self.hwnd.get()
    }

    pub fn settings(&self) -> Settings {
        self.timer.get().settings()
    }

    pub fn timer(&self) -> Timer {
        self.timer.get()
    }

    pub fn mini_pos(&self) -> Option<(i32, i32)> {
        self.mini_pos.get()
    }

    pub fn set_mini_pos(&self, x: i32, y: i32) {
        self.mini_pos.set(Some((x, y)));
        self.save();
    }

    fn update_timer<R>(&self, f: impl FnOnce(&mut Timer) -> R) -> R {
        let mut t = self.timer.get();
        let r = f(&mut t);
        self.timer.set(t);
        r
    }

    /// A clone of the taskbar interface, so no borrow is held while calling into the shell.
    fn taskbar(&self) -> Option<Taskbar> {
        self.taskbar.borrow().clone()
    }

    // ----- actions -------------------------------------------------------

    pub fn toggle(&self) {
        let now = now_ms();
        self.update_timer(|t| t.toggle(now));
        self.after_change();
    }

    pub fn start(&self) {
        let now = now_ms();
        self.update_timer(|t| t.start(now));
        self.after_change();
    }

    pub fn pause(&self) {
        let now = now_ms();
        self.update_timer(|t| t.pause(now));
        self.after_change();
    }

    fn reset(&self) {
        self.update_timer(|t| t.reset());
        self.after_change();
    }

    fn skip(&self) {
        let now = now_ms();
        let c = self.update_timer(|t| t.skip(now));
        self.on_completion(c);
        self.after_change();
    }

    fn switch_mode(&self, mode: Mode) {
        let t = self.timer.get();
        if t.mode() == mode && t.phase() == Phase::Idle {
            return;
        }
        if t.phase() != Phase::Idle {
            let text = if t.mode() == mode {
                format!("Restart this {} session?", t.mode().label().to_lowercase())
            } else {
                format!(
                    "Stop this {} session and switch to {}?",
                    t.mode().label().to_lowercase(),
                    mode.label().to_lowercase()
                )
            };
            let answer = unsafe {
                MessageBoxW(Some(self.hwnd()), &HSTRING::from(text), w!("WillPomo"), MB_YESNO | MB_ICONQUESTION)
            };
            if answer != IDYES {
                return;
            }
        }
        self.update_timer(|t| t.switch_to(mode));
        self.after_change();
    }

    fn after_change(&self) {
        self.refresh();
        self.schedule_tick();
    }

    /// Wake up just after the displayed second changes, rather than polling.
    fn schedule_tick(&self) {
        let t = self.timer.get();
        unsafe {
            if t.is_running() {
                let rem = t.remaining_ms(now_ms());
                let until_next_second = match rem % 1000 {
                    0 => 1000,
                    r => r,
                };
                SetTimer(Some(self.hwnd()), TIMER_TICK, (until_next_second + 15) as u32, None);
            } else {
                let _ = KillTimer(Some(self.hwnd()), TIMER_TICK);
            }
        }
    }

    fn on_tick(&self) {
        let now = now_ms();
        if let Some(c) = self.update_timer(|t| t.tick(now)) {
            self.on_completion(c);
        }
        self.after_change();
    }

    fn on_completion(&self, c: Completion) {
        // The "#n" count is per day.
        let date = today();
        let new_day = *self.stats_date.borrow() != date;
        if new_day {
            *self.stats_date.borrow_mut() = date;
            let n = u32::from(c.finished == Mode::Focus);
            self.update_timer(|t| t.set_completed(n));
        }
        self.save();
        if c.skipped {
            return;
        }
        let alert_style = self.settings().alert;
        if alert_style.plays_sound() {
            sound::play_chime();
        }
        self.flash(true);
        if alert_style.shows_popup() {
            alert::show(self, self.alert_content(c), true);
        }
    }

    fn alert_content(&self, c: Completion) -> Content {
        let s = self.settings();
        let unit = if self.demo { "second" } else { "minute" };
        let done = self.timer.get().completed();
        let sessions = if done == 1 { "1 session".to_string() } else { format!("{done} sessions") };
        let (heading, body) = match c.next {
            Mode::LongBreak => (
                "Focus session complete",
                format!("Nice work: {sessions} today. Time for a {}-{unit} long break.", s.long_break_minutes),
            ),
            Mode::ShortBreak => (
                "Focus session complete",
                format!("Time for a {}-{unit} break.", s.short_break_minutes),
            ),
            Mode::Focus => ("Break's over", format!("Ready for a {}-{unit} focus session?", s.focus_minutes)),
        };
        let start = if c.next.is_break() { "Start break" } else { "Start focus" };
        let (primary, secondary) = if c.auto_started {
            (("OK", Action::Dismiss), ("Pause", Action::Pause))
        } else {
            ((start, Action::StartNext), ("Not now", Action::Dismiss))
        };
        Content {
            heading: heading.to_string(),
            body,
            primary,
            secondary,
            bg: mode_color(c.next),
        }
    }

    pub fn on_alert_action(&self, action: Action) {
        alert::close(self);
        self.flash(false);
        match action {
            Action::StartNext => self.start(),
            Action::Pause => self.pause(),
            Action::Dismiss => {}
        }
    }

    /// Flash the taskbar button until the user switches to WillPomo.
    fn flash(&self, on: bool) {
        let info = FLASHWINFO {
            cbSize: std::mem::size_of::<FLASHWINFO>() as u32,
            hwnd: self.hwnd(),
            dwFlags: if on { FLASHW_TRAY | FLASHW_TIMERNOFG } else { FLASHW_STOP },
            uCount: 0,
            dwTimeout: 0,
        };
        unsafe {
            let _ = FlashWindowEx(&info);
        }
    }

    pub fn apply_settings(&self, new: Settings) {
        let old = self.settings();
        self.update_timer(|t| t.apply_settings(new));
        if new.tray_icon != old.tray_icon {
            if new.tray_icon {
                self.add_tray();
            } else {
                self.remove_tray();
            }
        }
        if new.always_on_top != old.always_on_top {
            let after = if new.always_on_top { HWND_TOPMOST } else { HWND_NOTOPMOST };
            unsafe {
                let _ = SetWindowPos(self.hwnd(), Some(after), 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
            }
        }
        if !new.mini_mode {
            mini::hide(self);
        }
        self.badge_key.set(None);
        self.save();
        self.after_change();
    }

    fn save(&self) {
        if self.demo {
            return; // demo mode never touches your real settings
        }
        let Some(path) = &self.store_path else { return };
        let t = self.timer.get();
        let st = Stored {
            settings: t.settings(),
            window_pos: self.window_pos.get(),
            mini_pos: self.mini_pos.get(),
            stats_date: Some(self.stats_date.borrow().clone()),
            stats_completed: t.completed(),
        };
        let _ = settings::save(path, &st);
    }

    pub fn show_main(&self) {
        let hwnd = self.hwnd();
        unsafe {
            if IsIconic(hwnd).as_bool() {
                let _ = ShowWindow(hwnd, SW_RESTORE);
            } else {
                let _ = ShowWindow(hwnd, SW_SHOW);
            }
            let _ = SetForegroundWindow(hwnd);
        }
    }

    fn open_settings(&self) {
        self.show_main();
        settings_dialog::open(self);
    }

    // ----- what's shown --------------------------------------------------

    /// Update everything that shows the time: window, title, taskbar and tray.
    fn refresh(&self) {
        let hwnd = self.hwnd();
        if hwnd.is_invalid() {
            return;
        }
        let t = self.timer.get();
        let now = now_ms();
        let time = format_mmss(t.remaining_ms(now));

        // The title is also the taskbar button's label (when labels are shown).
        let title = match t.phase() {
            Phase::Paused => format!("{time} – Paused"),
            _ => format!("{time} – {}", t.mode().label()),
        };
        let title_changed = *self.last_title.borrow() != title;
        if title_changed {
            unsafe {
                let _ = SetWindowTextW(hwnd, &HSTRING::from(title.as_str()));
            }
            *self.last_title.borrow_mut() = title;
        }

        if self.shown_mode.get() != Some(t.mode()) {
            self.shown_mode.set(Some(t.mode()));
            set_title_bar_color(hwnd, mode_color(t.mode()));
        }

        let taskbar = self.taskbar();
        if let Some(tb) = &taskbar {
            tb.set_progress(hwnd, t.phase(), t.elapsed_ms(now), t.total_ms());
            let running = t.is_running();
            if self.shown_running.get() != Some(running) {
                self.shown_running.set(Some(running));
                tb.update_buttons(hwnd, &self.thumb_icons, running);
            }
        }

        let minutes = t.minutes_left(now);
        let key = (t.mode(), t.phase(), minutes);
        if self.badge_key.get() != Some(key) {
            self.badge_key.set(Some(key));
            self.update_badges(&t, minutes, taskbar.as_ref());
        }

        unsafe {
            let _ = InvalidateRect(Some(hwnd), None, false);
        }
        mini::invalidate(self);
    }

    /// Redraw the minutes badge on the taskbar button and in the tray.
    fn update_badges(&self, t: &Timer, minutes: u64, taskbar: Option<&Taskbar>) {
        let hwnd = self.hwnd();
        let size = small_icon_size();
        let color = if t.is_running() { mode_color(t.mode()) } else { IDLE_GREY };
        let text = minutes.min(99).to_string();
        let unit = if self.demo { "second" } else { "minute" };
        let left = format!("{minutes} {unit}{} left", if minutes == 1 { "" } else { "s" });

        if let Some(tb) = taskbar {
            // Only while a session is under way; a fresh timer shows no badge.
            let overlay = if t.phase() == Phase::Idle {
                None
            } else {
                icons::badge(size, &text, color, Shape::Circle)
            };
            tb.set_overlay(hwnd, overlay, &left);
            icons::destroy(self.overlay_icon.replace(overlay));
        }

        if self.tray_added.get() {
            let icon = icons::badge(size, &text, color, Shape::RoundedSquare);
            tray::update(hwnd, icon, &self.tray_tip(t, &left));
            icons::destroy(self.tray_icon.replace(icon));
        }
    }

    fn tray_tip(&self, t: &Timer, left: &str) -> String {
        match t.phase() {
            Phase::Idle => format!("WillPomo: ready for {} ({left})", t.mode().label().to_lowercase()),
            Phase::Running => format!("WillPomo: {}, {left}", t.mode().label().to_lowercase()),
            Phase::Paused => format!("WillPomo: paused, {left}"),
        }
    }

    fn add_tray(&self) {
        let t = self.timer.get();
        let now = now_ms();
        let minutes = t.minutes_left(now);
        let color = if t.is_running() { mode_color(t.mode()) } else { IDLE_GREY };
        let icon = icons::badge(small_icon_size(), &minutes.min(99).to_string(), color, Shape::RoundedSquare);
        let unit = if self.demo { "second" } else { "minute" };
        let left = format!("{minutes} {unit}{} left", if minutes == 1 { "" } else { "s" });
        if tray::add(self.hwnd(), icon, &self.tray_tip(&t, &left)) {
            self.tray_added.set(true);
            icons::destroy(self.tray_icon.replace(icon));
        } else {
            icons::destroy(icon);
        }
    }

    fn remove_tray(&self) {
        if self.tray_added.replace(false) {
            tray::remove(self.hwnd());
        }
        icons::destroy(self.tray_icon.take());
    }

    fn paint(&self) {
        let hwnd = self.hwnd();
        let Ok(mut surface) = self.surface.try_borrow_mut() else { return };
        if surface.is_none() {
            *surface = Surface::new(&self.gfx, hwnd, self.dpi.get()).ok();
        }
        let Some(s) = surface.as_ref() else { return };
        let t = self.timer.get();
        let view = View {
            mode: t.mode(),
            phase: t.phase(),
            time: format_mmss(t.remaining_ms(now_ms())),
            session: t.session_number(),
            dots: t.cycle(),
            hover: self.hover.get(),
            pressed: self.pressed.get(),
        };
        if !ui::paint(s, &self.gfx, &self.layout, &view) {
            *surface = None; // device lost: rebuild on the next paint
            unsafe {
                let _ = InvalidateRect(Some(hwnd), None, false);
            }
        }
    }

    fn hit_at(&self, lparam: LPARAM) -> Option<Hit> {
        let (x, y) = point_from_lparam(lparam);
        let k = 96.0 / self.dpi.get() as f32;
        self.layout.hit(x as f32 * k, y as f32 * k)
    }

    fn activate(&self, hit: Hit) {
        match hit {
            Hit::Tab(mode) => self.switch_mode(mode),
            Hit::Start => self.toggle(),
            Hit::Reset => self.reset(),
            Hit::Skip => self.skip(),
            Hit::Settings => self.open_settings(),
        }
    }

    fn command(&self, id: u16) {
        match id {
            CMD_TOGGLE => self.toggle(),
            CMD_SKIP => self.skip(),
            CMD_RESET => self.reset(),
            CMD_SHOW => self.show_main(),
            CMD_SETTINGS => self.open_settings(),
            CMD_EXIT => unsafe {
                let _ = DestroyWindow(self.hwnd());
            },
            _ => {}
        }
    }

    pub fn tray_menu(&self, x: i32, y: i32) {
        let hwnd = self.hwnd();
        let toggle = match self.timer.get().phase() {
            Phase::Idle => w!("Start"),
            Phase::Running => w!("Pause"),
            Phase::Paused => w!("Resume"),
        };
        unsafe {
            let Ok(menu) = CreatePopupMenu() else { return };
            let _ = AppendMenuW(menu, MF_STRING, CMD_TOGGLE as usize, toggle);
            let _ = AppendMenuW(menu, MF_STRING, CMD_SKIP as usize, w!("Skip to next session"));
            let _ = AppendMenuW(menu, MF_STRING, CMD_RESET as usize, w!("Reset this session"));
            let _ = AppendMenuW(menu, MF_SEPARATOR, 0, PCWSTR::null());
            let _ = AppendMenuW(menu, MF_STRING, CMD_SHOW as usize, w!("Show WillPomo"));
            let _ = AppendMenuW(menu, MF_STRING, CMD_SETTINGS as usize, w!("Settings…"));
            let _ = AppendMenuW(menu, MF_SEPARATOR, 0, PCWSTR::null());
            let _ = AppendMenuW(menu, MF_STRING, CMD_EXIT as usize, w!("Exit"));
            // Needed so the menu closes when you click elsewhere.
            let _ = SetForegroundWindow(hwnd);
            let cmd = TrackPopupMenu(menu, TPM_RETURNCMD | TPM_RIGHTBUTTON | TPM_NONOTIFY, x, y, None, hwnd, None);
            let _ = PostMessageW(Some(hwnd), WM_NULL, WPARAM(0), LPARAM(0));
            let _ = DestroyMenu(menu);
            if cmd.0 != 0 {
                self.command(cmd.0 as u16);
            }
        }
    }

    fn on_tray(&self, wparam: WPARAM, lparam: LPARAM) {
        let event = u32::from(loword(lparam.0 as usize));
        match event {
            NIN_SELECT | NIN_KEYSELECT => unsafe {
                let hwnd = self.hwnd();
                let up_front = IsWindowVisible(hwnd).as_bool()
                    && !IsIconic(hwnd).as_bool()
                    && GetForegroundWindow() == hwnd;
                if up_front {
                    let _ = ShowWindow(hwnd, SW_MINIMIZE);
                } else {
                    self.show_main();
                }
            },
            WM_CONTEXTMENU => {
                // NOTIFYICON_VERSION_4 passes the anchor point in wParam.
                let (x, y) = point_from_lparam(LPARAM(wparam.0 as isize));
                self.tray_menu(x, y);
            }
            _ => {}
        }
    }

    fn on_destroy(&self) {
        unsafe {
            let _ = KillTimer(Some(self.hwnd()), TIMER_TICK);
        }
        alert::close(self);
        settings_dialog::close(self);
        mini::hide(self);
        self.save();
        self.remove_tray();
        if let Some(tb) = self.taskbar() {
            tb.set_overlay(self.hwnd(), None, "");
        }
        icons::destroy(self.overlay_icon.take());
        unsafe { PostQuitMessage(0) };
    }

    fn handle(&self, hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
        unsafe {
            if msg == self.msg_button_created && msg != 0 {
                // Explorer created (or re-created) our taskbar button.
                let missing = self.taskbar.borrow().is_none();
                if missing {
                    *self.taskbar.borrow_mut() = Taskbar::new();
                }
                if let Some(tb) = self.taskbar() {
                    tb.add_buttons(hwnd, &self.thumb_icons, self.timer.get().is_running());
                }
                self.badge_key.set(None);
                self.shown_running.set(None);
                self.refresh();
                return LRESULT(0);
            }
            if msg == self.msg_taskbar_created && msg != 0 {
                // Explorer restarted: the tray icon has to be added again.
                if self.settings().tray_icon {
                    self.tray_added.set(false);
                    self.add_tray();
                }
                return LRESULT(0);
            }
            match msg {
                WM_PAINT => {
                    let mut ps = PAINTSTRUCT::default();
                    BeginPaint(hwnd, &mut ps);
                    self.paint();
                    let _ = EndPaint(hwnd, &ps);
                    LRESULT(0)
                }
                WM_ERASEBKGND => LRESULT(1),
                WM_SIZE => {
                    // Minimised: show the mini timer. Restored: put it away.
                    match wparam.0 as u32 {
                        SIZE_MINIMIZED if self.settings().mini_mode => mini::show(self),
                        SIZE_RESTORED => mini::hide(self),
                        _ => {}
                    }
                    if let Ok(s) = self.surface.try_borrow() {
                        if let Some(s) = s.as_ref() {
                            s.resize(u32::from(loword(lparam.0 as usize)), u32::from(hiword(lparam.0 as usize)));
                        }
                    }
                    LRESULT(0)
                }
                WM_DPICHANGED => {
                    let dpi = u32::from(hiword(wparam.0));
                    self.dpi.set(dpi);
                    if let Ok(s) = self.surface.try_borrow() {
                        if let Some(s) = s.as_ref() {
                            s.set_dpi(dpi);
                        }
                    }
                    let r = &*(lparam.0 as *const RECT);
                    let _ = SetWindowPos(hwnd, None, r.left, r.top, r.right - r.left, r.bottom - r.top, SWP_NOZORDER | SWP_NOACTIVATE);
                    LRESULT(0)
                }
                WM_TIMER if wparam.0 == TIMER_TICK => {
                    self.on_tick();
                    LRESULT(0)
                }
                WM_MOUSEMOVE => {
                    let hit = self.hit_at(lparam);
                    if self.hover.replace(hit) != hit {
                        let _ = InvalidateRect(Some(hwnd), None, false);
                    }
                    if !self.tracking_mouse.get() {
                        let mut tme = TRACKMOUSEEVENT {
                            cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32,
                            dwFlags: TME_LEAVE,
                            hwndTrack: hwnd,
                            dwHoverTime: 0,
                        };
                        let _ = TrackMouseEvent(&mut tme);
                        self.tracking_mouse.set(true);
                    }
                    LRESULT(0)
                }
                WM_MOUSELEAVE => {
                    self.tracking_mouse.set(false);
                    if self.hover.replace(None).is_some() {
                        let _ = InvalidateRect(Some(hwnd), None, false);
                    }
                    LRESULT(0)
                }
                WM_LBUTTONDOWN => {
                    let hit = self.hit_at(lparam);
                    self.pressed.set(hit);
                    if hit.is_some() {
                        SetCapture(hwnd);
                    }
                    let _ = InvalidateRect(Some(hwnd), None, false);
                    LRESULT(0)
                }
                WM_LBUTTONUP => {
                    let hit = self.hit_at(lparam);
                    let pressed = self.pressed.take();
                    let _ = ReleaseCapture();
                    let _ = InvalidateRect(Some(hwnd), None, false);
                    if let Some(h) = hit.filter(|h| Some(*h) == pressed) {
                        self.activate(h);
                    }
                    LRESULT(0)
                }
                WM_SETCURSOR if u32::from(loword(lparam.0 as usize)) == HTCLIENT => {
                    // Hit-test where the pointer is now (WM_MOUSEMOVE arrives after this).
                    let mut p = POINT::default();
                    let over_button = GetCursorPos(&mut p).is_ok()
                        && ScreenToClient(hwnd, &mut p).as_bool()
                        && {
                            let k = 96.0 / self.dpi.get() as f32;
                            self.layout.hit(p.x as f32 * k, p.y as f32 * k).is_some()
                        };
                    let id = if over_button { IDC_HAND } else { IDC_ARROW };
                    if let Ok(c) = LoadCursorW(None, id) {
                        SetCursor(Some(c));
                    }
                    LRESULT(1)
                }
                WM_KEYDOWN => {
                    let ctrl = GetKeyState(i32::from(VK_CONTROL.0)) < 0;
                    match VIRTUAL_KEY(wparam.0 as u16) {
                        VK_SPACE | VK_RETURN => self.toggle(),
                        VK_OEM_COMMA if ctrl => self.open_settings(),
                        k => match k.0 {
                            0x4E => self.skip(),  // N: next
                            0x52 => self.reset(), // R: reset
                            0x31 => self.switch_mode(Mode::Focus),
                            0x32 => self.switch_mode(Mode::ShortBreak),
                            0x33 => self.switch_mode(Mode::LongBreak),
                            _ => {}
                        },
                    }
                    LRESULT(0)
                }
                WM_COMMAND => {
                    let id = loword(wparam.0);
                    if u32::from(hiword(wparam.0)) == THBN_CLICKED {
                        match u32::from(id) {
                            BTN_TOGGLE => self.toggle(),
                            BTN_SKIP => self.skip(),
                            BTN_RESET => self.reset(),
                            _ => {}
                        }
                    } else {
                        self.command(id);
                    }
                    LRESULT(0)
                }
                WM_TRAY => {
                    self.on_tray(wparam, lparam);
                    LRESULT(0)
                }
                WM_APP_ACTIVATE => {
                    self.show_main();
                    LRESULT(0)
                }
                WM_MOVE => {
                    if !IsIconic(hwnd).as_bool() {
                        let mut r = RECT::default();
                        if GetWindowRect(hwnd, &mut r).is_ok() {
                            self.window_pos.set(Some((r.left, r.top)));
                        }
                    }
                    LRESULT(0)
                }
                WM_CLOSE => {
                    let _ = DestroyWindow(hwnd);
                    LRESULT(0)
                }
                WM_DESTROY => {
                    self.on_destroy();
                    LRESULT(0)
                }
                _ => DefWindowProcW(hwnd, msg, wparam, lparam),
            }
        }
    }
}
