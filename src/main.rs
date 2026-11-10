//! WillPomo: a lightweight focus timer for Windows that shows the time left on your taskbar.
//!
//! Copyright (c) 2026 Williams Automation. Released under the MIT License.
//!
//! The timer, settings and chime modules are plain Rust with no Windows APIs, so
//! they can be unit-tested on any platform. Everything that touches Win32 lives
//! under `win/`.

// No console window for the real app; keep one for `cargo test` so output shows.
#![cfg_attr(not(test), windows_subsystem = "windows")]
#![cfg_attr(not(windows), allow(dead_code))]

mod chime;
mod settings;
mod timer;

#[cfg(windows)]
mod win;

#[cfg(windows)]
fn main() {
    win::run();
}

#[cfg(not(windows))]
fn main() {
    eprintln!("WillPomo is a Windows application. Build it with a Windows target.");
}
