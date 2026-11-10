//! The taskbar button: progress bar, minutes badge (overlay icon) and the
//! Start/Pause, Skip and Reset buttons under the window's thumbnail preview.

use windows::core::HSTRING;
use windows::Win32::Foundation::HWND;
use windows::Win32::System::Com::{CoCreateInstance, CLSCTX_INPROC_SERVER};
use windows::Win32::UI::Shell::{
    ITaskbarList3, TaskbarList, THBF_ENABLED, THB_FLAGS, THB_ICON, THB_TOOLTIP, THUMBBUTTON,
    TBPF_NOPROGRESS, TBPF_NORMAL, TBPF_PAUSED,
};
use windows::Win32::UI::WindowsAndMessaging::HICON;

use super::util::fill_wstr;
use crate::timer::Phase;

pub const BTN_TOGGLE: u32 = 101;
pub const BTN_SKIP: u32 = 102;
pub const BTN_RESET: u32 = 103;

/// Glyph icons for the thumbnail buttons, created once.
#[derive(Clone, Copy)]
pub struct ThumbIcons {
    pub play: Option<HICON>,
    pub pause: Option<HICON>,
    pub skip: Option<HICON>,
    pub reset: Option<HICON>,
}

#[derive(Clone)]
pub struct Taskbar {
    list: ITaskbarList3,
}

impl Taskbar {
    /// Only valid after the window receives the "TaskbarButtonCreated" message.
    pub fn new() -> Option<Taskbar> {
        unsafe {
            let list: ITaskbarList3 = CoCreateInstance(&TaskbarList, None, CLSCTX_INPROC_SERVER).ok()?;
            list.HrInit().ok()?;
            Some(Taskbar { list })
        }
    }

    pub fn set_progress(&self, hwnd: HWND, phase: Phase, done: u64, total: u64) {
        unsafe {
            let state = match phase {
                Phase::Idle => TBPF_NOPROGRESS,
                Phase::Running => TBPF_NORMAL,
                Phase::Paused => TBPF_PAUSED,
            };
            let _ = self.list.SetProgressState(hwnd, state);
            if phase != Phase::Idle {
                let _ = self.list.SetProgressValue(hwnd, done.min(total), total.max(1));
            }
        }
    }

    pub fn set_overlay(&self, hwnd: HWND, icon: Option<HICON>, description: &str) {
        unsafe {
            let _ = self.list.SetOverlayIcon(hwnd, icon.unwrap_or_default(), &HSTRING::from(description));
        }
    }

    fn buttons(icons: &ThumbIcons, running: bool) -> [THUMBBUTTON; 3] {
        let make = |id: u32, icon: Option<HICON>, tip: &str| {
            let mut b = THUMBBUTTON {
                dwMask: THB_ICON | THB_TOOLTIP | THB_FLAGS,
                iId: id,
                hIcon: icon.unwrap_or_default(),
                dwFlags: THBF_ENABLED,
                ..Default::default()
            };
            fill_wstr(&mut b.szTip, tip);
            b
        };
        [
            if running {
                make(BTN_TOGGLE, icons.pause, "Pause")
            } else {
                make(BTN_TOGGLE, icons.play, "Start")
            },
            make(BTN_SKIP, icons.skip, "Skip to next session"),
            make(BTN_RESET, icons.reset, "Reset this session"),
        ]
    }

    /// Can only be done once per taskbar button (again after Explorer restarts).
    pub fn add_buttons(&self, hwnd: HWND, icons: &ThumbIcons, running: bool) -> bool {
        unsafe { self.list.ThumbBarAddButtons(hwnd, &Self::buttons(icons, running)).is_ok() }
    }

    pub fn update_buttons(&self, hwnd: HWND, icons: &ThumbIcons, running: bool) {
        unsafe {
            let _ = self.list.ThumbBarUpdateButtons(hwnd, &Self::buttons(icons, running));
        }
    }
}
