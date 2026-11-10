//! User settings, plus the little bit of state WillPomo remembers between runs.
//!
//! Stored as a plain `key=value` text file in `%LOCALAPPDATA%\WillPomo\settings.ini`,
//! which keeps the app free of extra dependencies. Unknown keys are ignored and
//! out-of-range values are clamped, so a hand-edited file can't break anything.

use std::path::{Path, PathBuf};

use crate::timer::Mode;

/// What happens when a session ends.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AlertStyle {
    /// Play a chime.
    Sound,
    /// No sound: pop up a window that takes focus.
    SilentPopup,
    /// Both.
    SoundAndPopup,
}

impl AlertStyle {
    pub const ALL: [AlertStyle; 3] = [
        AlertStyle::Sound,
        AlertStyle::SilentPopup,
        AlertStyle::SoundAndPopup,
    ];

    pub fn plays_sound(self) -> bool {
        matches!(self, AlertStyle::Sound | AlertStyle::SoundAndPopup)
    }

    pub fn shows_popup(self) -> bool {
        matches!(self, AlertStyle::SilentPopup | AlertStyle::SoundAndPopup)
    }

    fn key(self) -> &'static str {
        match self {
            AlertStyle::Sound => "sound",
            AlertStyle::SilentPopup => "silent_popup",
            AlertStyle::SoundAndPopup => "sound_and_popup",
        }
    }

    fn from_key(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|a| a.key() == s)
    }
}

pub const MINUTES_MIN: u32 = 1;
pub const MINUTES_MAX: u32 = 99;
pub const EVERY_MIN: u32 = 1;
pub const EVERY_MAX: u32 = 12;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Settings {
    pub focus_minutes: u32,
    pub short_break_minutes: u32,
    pub long_break_minutes: u32,
    /// Take a long break after this many focus sessions.
    pub long_break_every: u32,
    pub auto_start_breaks: bool,
    pub auto_start_focus: bool,
    pub alert: AlertStyle,
    /// Also show the minutes left in the notification area (system tray).
    pub tray_icon: bool,
    pub always_on_top: bool,
    /// Minimising shows a small always-on-top timer instead of just the taskbar button.
    pub mini_mode: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            focus_minutes: 25,
            short_break_minutes: 5,
            long_break_minutes: 15,
            long_break_every: 4,
            auto_start_breaks: false,
            auto_start_focus: false,
            alert: AlertStyle::Sound,
            tray_icon: true,
            always_on_top: false,
            mini_mode: true,
        }
    }
}

impl Settings {
    pub fn minutes(&self, mode: Mode) -> u32 {
        match mode {
            Mode::Focus => self.focus_minutes,
            Mode::ShortBreak => self.short_break_minutes,
            Mode::LongBreak => self.long_break_minutes,
        }
    }

    pub fn clamped(mut self) -> Self {
        self.focus_minutes = clamp_minutes(self.focus_minutes);
        self.short_break_minutes = clamp_minutes(self.short_break_minutes);
        self.long_break_minutes = clamp_minutes(self.long_break_minutes);
        self.long_break_every = self.long_break_every.clamp(EVERY_MIN, EVERY_MAX);
        self
    }
}

pub fn clamp_minutes(v: u32) -> u32 {
    v.clamp(MINUTES_MIN, MINUTES_MAX)
}

/// Everything written to settings.ini.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Stored {
    pub settings: Settings,
    /// Top-left corner of the main window, in screen pixels.
    pub window_pos: Option<(i32, i32)>,
    /// Top-left corner of the mini timer, in screen pixels.
    pub mini_pos: Option<(i32, i32)>,
    /// The day `stats_completed` refers to, as YYYY-MM-DD (local time).
    pub stats_date: Option<String>,
    pub stats_completed: u32,
}

impl Stored {
    pub fn parse(text: &str) -> Stored {
        let mut st = Stored::default();
        let (mut wx, mut wy) = (None, None);
        let (mut mx, mut my) = (None, None);
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with(['#', ';', '[']) {
                continue;
            }
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            let (key, value) = (key.trim(), value.trim());
            let s = &mut st.settings;
            match key {
                "focus_minutes" => set_num(&mut s.focus_minutes, value),
                "short_break_minutes" => set_num(&mut s.short_break_minutes, value),
                "long_break_minutes" => set_num(&mut s.long_break_minutes, value),
                "long_break_every" => set_num(&mut s.long_break_every, value),
                "auto_start_breaks" => set_bool(&mut s.auto_start_breaks, value),
                "auto_start_focus" => set_bool(&mut s.auto_start_focus, value),
                "tray_icon" => set_bool(&mut s.tray_icon, value),
                "always_on_top" => set_bool(&mut s.always_on_top, value),
                "mini_mode" => set_bool(&mut s.mini_mode, value),
                "mini_x" => mx = value.parse::<i32>().ok(),
                "mini_y" => my = value.parse::<i32>().ok(),
                "alert" => {
                    if let Some(a) = AlertStyle::from_key(value) {
                        s.alert = a;
                    }
                }
                "window_x" => wx = value.parse::<i32>().ok(),
                "window_y" => wy = value.parse::<i32>().ok(),
                "stats_date" => {
                    if is_iso_date(value) {
                        st.stats_date = Some(value.to_string());
                    }
                }
                "stats_completed" => set_num(&mut st.stats_completed, value),
                _ => {}
            }
        }
        st.settings = st.settings.clamped();
        if let (Some(x), Some(y)) = (wx, wy) {
            st.window_pos = Some((x, y));
        }
        if let (Some(x), Some(y)) = (mx, my) {
            st.mini_pos = Some((x, y));
        }
        st
    }

    pub fn to_ini(&self) -> String {
        let s = &self.settings;
        let mut out = String::from("# WillPomo settings. Edit while WillPomo is closed.\n");
        let mut put = |k: &str, v: String| {
            out.push_str(k);
            out.push('=');
            out.push_str(&v);
            out.push('\n');
        };
        put("focus_minutes", s.focus_minutes.to_string());
        put("short_break_minutes", s.short_break_minutes.to_string());
        put("long_break_minutes", s.long_break_minutes.to_string());
        put("long_break_every", s.long_break_every.to_string());
        put("auto_start_breaks", bool_str(s.auto_start_breaks));
        put("auto_start_focus", bool_str(s.auto_start_focus));
        put("alert", s.alert.key().to_string());
        put("tray_icon", bool_str(s.tray_icon));
        put("always_on_top", bool_str(s.always_on_top));
        put("mini_mode", bool_str(s.mini_mode));
        if let Some((x, y)) = self.window_pos {
            put("window_x", x.to_string());
            put("window_y", y.to_string());
        }
        if let Some((x, y)) = self.mini_pos {
            put("mini_x", x.to_string());
            put("mini_y", y.to_string());
        }
        if let Some(d) = &self.stats_date {
            put("stats_date", d.clone());
            put("stats_completed", self.stats_completed.to_string());
        }
        out
    }
}

fn set_num(field: &mut u32, value: &str) {
    if let Ok(n) = value.parse() {
        *field = n;
    }
}

fn set_bool(field: &mut bool, value: &str) {
    match value.to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => *field = true,
        "0" | "false" | "no" | "off" => *field = false,
        _ => {}
    }
}

fn bool_str(b: bool) -> String {
    if b { "1" } else { "0" }.to_string()
}

fn is_iso_date(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() == 10
        && b[4] == b'-'
        && b[7] == b'-'
        && b.iter()
            .enumerate()
            .all(|(i, c)| i == 4 || i == 7 || c.is_ascii_digit())
}

/// `%LOCALAPPDATA%\WillPomo\settings.ini`. When installed from the Store,
/// Windows transparently keeps this inside the app's own package data.
pub fn default_path() -> Option<PathBuf> {
    std::env::var_os("LOCALAPPDATA").map(|d| PathBuf::from(d).join("WillPomo").join("settings.ini"))
}

pub fn load(path: &Path) -> Stored {
    std::fs::read_to_string(path)
        .map(|t| Stored::parse(&t))
        .unwrap_or_default()
}

/// Write via a temporary file and rename, so a crash can't leave a half-written file.
pub fn save(path: &Path, st: &Stored) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("ini.tmp");
    std::fs::write(&tmp, st.to_ini())?;
    std::fs::rename(&tmp, path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips() {
        let st = Stored {
            settings: Settings {
                focus_minutes: 50,
                short_break_minutes: 10,
                long_break_minutes: 30,
                long_break_every: 3,
                auto_start_breaks: true,
                auto_start_focus: true,
                alert: AlertStyle::SilentPopup,
                tray_icon: false,
                always_on_top: true,
                mini_mode: false,
            },
            window_pos: Some((-1200, 40)),
            mini_pos: Some((10, 700)),
            stats_date: Some("2026-11-10".into()),
            stats_completed: 7,
        };
        assert_eq!(Stored::parse(&st.to_ini()), st);
    }

    #[test]
    fn empty_or_garbage_gives_defaults() {
        assert_eq!(Stored::parse(""), Stored::default());
        assert_eq!(Stored::parse("nonsense\n=\n[section]\nfoo=bar"), Stored::default());
    }

    #[test]
    fn clamps_and_ignores_bad_values() {
        let st = Stored::parse(
            "focus_minutes=0\nshort_break_minutes=500\nlong_break_every=99\nalert=loud\n\
             auto_start_breaks=maybe\nstats_date=10/11/2026\nwindow_x=5\n",
        );
        assert_eq!(st.settings.focus_minutes, MINUTES_MIN);
        assert_eq!(st.settings.short_break_minutes, MINUTES_MAX);
        assert_eq!(st.settings.long_break_every, EVERY_MAX);
        assert_eq!(st.settings.alert, AlertStyle::Sound);
        assert!(!st.settings.auto_start_breaks);
        assert_eq!(st.stats_date, None);
        assert_eq!(st.window_pos, None, "needs both x and y");
    }

    #[test]
    fn accepts_friendly_booleans_and_whitespace() {
        let st = Stored::parse("  tray_icon = off \n always_on_top=YES\n");
        assert!(!st.settings.tray_icon);
        assert!(st.settings.always_on_top);
    }

    #[test]
    fn alert_style_flags() {
        assert!(AlertStyle::Sound.plays_sound() && !AlertStyle::Sound.shows_popup());
        assert!(!AlertStyle::SilentPopup.plays_sound() && AlertStyle::SilentPopup.shows_popup());
        assert!(AlertStyle::SoundAndPopup.plays_sound() && AlertStyle::SoundAndPopup.shows_popup());
    }

    #[test]
    fn save_and_load_files() {
        let dir = std::env::temp_dir().join(format!("pomobar-test-{}", std::process::id()));
        let path = dir.join("settings.ini");
        let st = Stored {
            stats_date: Some("2026-01-02".into()),
            stats_completed: 3,
            ..Stored::default()
        };
        save(&path, &st).unwrap();
        save(&path, &st).unwrap(); // overwriting works too
        assert_eq!(load(&path), st);
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(load(&path), Stored::default(), "missing file gives defaults");
    }
}
