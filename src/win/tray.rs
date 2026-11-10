//! The optional notification-area (system tray) icon, which shows the minutes left.

use windows::Win32::Foundation::HWND;
use windows::Win32::UI::Shell::{
    Shell_NotifyIconW, NIF_ICON, NIF_MESSAGE, NIF_SHOWTIP, NIF_TIP, NIM_ADD, NIM_DELETE, NIM_MODIFY,
    NIM_SETVERSION, NOTIFYICONDATAW, NOTIFYICON_VERSION_4,
};
use windows::Win32::UI::WindowsAndMessaging::{HICON, WM_APP};

use super::util::fill_wstr;

/// Window message the tray icon sends to the main window.
pub const WM_TRAY: u32 = WM_APP + 1;
const ID: u32 = 1;

fn data(hwnd: HWND, icon: Option<HICON>, tip: &str) -> NOTIFYICONDATAW {
    let mut nid = NOTIFYICONDATAW {
        cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
        hWnd: hwnd,
        uID: ID,
        uFlags: NIF_MESSAGE | NIF_TIP | NIF_SHOWTIP,
        uCallbackMessage: WM_TRAY,
        ..Default::default()
    };
    if let Some(i) = icon {
        nid.hIcon = i;
        nid.uFlags |= NIF_ICON;
    }
    fill_wstr(&mut nid.szTip, tip);
    nid
}

pub fn add(hwnd: HWND, icon: Option<HICON>, tip: &str) -> bool {
    unsafe {
        let mut nid = data(hwnd, icon, tip);
        if !Shell_NotifyIconW(NIM_ADD, &nid).as_bool() {
            return false;
        }
        nid.Anonymous.uVersion = NOTIFYICON_VERSION_4;
        let _ = Shell_NotifyIconW(NIM_SETVERSION, &nid);
        true
    }
}

pub fn update(hwnd: HWND, icon: Option<HICON>, tip: &str) {
    unsafe {
        let _ = Shell_NotifyIconW(NIM_MODIFY, &data(hwnd, icon, tip));
    }
}

pub fn remove(hwnd: HWND) {
    unsafe {
        let _ = Shell_NotifyIconW(NIM_DELETE, &data(hwnd, None, ""));
    }
}
