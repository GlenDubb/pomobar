//! Everything that talks to Windows.

mod alert;
mod app;
mod gfx;
mod icons;
mod mini;
mod settings_dialog;
mod sound;
mod taskbar;
mod tray;
mod ui;
mod util;

pub fn run() {
    app::run();
}
