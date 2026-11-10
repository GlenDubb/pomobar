//! Layout, hit-testing and painting for the main window. Everything is in DIPs.

use windows::Win32::Graphics::Direct2D::Common::D2D_RECT_F;
use windows::Win32::Foundation::D2DERR_RECREATE_TARGET;

use super::gfx::{Gfx, Surface};
use super::util::{black, contains, mode_color, offset, rect, white, Rgb};
use crate::timer::{Mode, Phase};

pub const WIDTH: f32 = 380.0;
pub const HEIGHT: f32 = 300.0;

/// Something clickable in the main window.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hit {
    Tab(Mode),
    Start,
    Reset,
    Skip,
    Settings,
}

// Segoe Fluent Icons / Segoe MDL2 Assets code points.
pub const GLYPH_RESET: &str = "\u{E72C}";
pub const GLYPH_SKIP: &str = "\u{E893}";
pub const GLYPH_SETTINGS: &str = "\u{E713}";

pub struct Layout {
    tabs: [D2D_RECT_F; 3],
    timer: D2D_RECT_F,
    start: D2D_RECT_F,
    reset: D2D_RECT_F,
    skip: D2D_RECT_F,
    settings: D2D_RECT_F,
    session: D2D_RECT_F,
    dots_x: f32,
    dots_y: f32,
    /// Widest digit and the colon, so the timer can use fixed-width cells.
    digit_w: f32,
    colon_w: f32,
}

impl Layout {
    pub fn new(gfx: &Gfx) -> Layout {
        let widths = Mode::ALL.map(|m| gfx.text_width(&gfx.text.tab_selected, m.label()) + 28.0);
        let gap = 4.0;
        let total: f32 = widths.iter().sum::<f32>() + gap * 2.0;
        let mut x = (WIDTH - total) / 2.0;
        let mut tabs = [rect(0.0, 0.0, 0.0, 0.0); 3];
        for (i, w) in widths.iter().enumerate() {
            tabs[i] = rect(x, 18.0, x + w, 50.0);
            x += w + gap;
        }
        let cx = WIDTH / 2.0;
        let start = rect(cx - 86.0, 190.0, cx + 86.0, 242.0);
        let side = 44.0;
        let side_top = start.top + (52.0 - side) / 2.0;
        Layout {
            tabs,
            timer: rect(0.0, 58.0, WIDTH, 180.0),
            start,
            reset: rect(start.left - 26.0 - side, side_top, start.left - 26.0, side_top + side),
            skip: rect(start.right + 26.0, side_top, start.right + 26.0 + side, side_top + side),
            settings: rect(WIDTH - 14.0 - 38.0, HEIGHT - 14.0 - 38.0, WIDTH - 14.0, HEIGHT - 14.0),
            session: rect(22.0, HEIGHT - 14.0 - 38.0, 90.0, HEIGHT - 14.0),
            dots_x: 74.0,
            dots_y: HEIGHT - 14.0 - 19.0,
            digit_w: (0..10)
                .map(|d| gfx.text_width(&gfx.text.timer, &d.to_string()))
                .fold(0.0, f32::max),
            colon_w: gfx.text_width(&gfx.text.timer, ":"),
        }
    }

    pub fn hit(&self, x: f32, y: f32) -> Option<Hit> {
        for (i, r) in self.tabs.iter().enumerate() {
            if contains(r, x, y) {
                return Some(Hit::Tab(Mode::ALL[i]));
            }
        }
        [
            (self.start, Hit::Start),
            (self.reset, Hit::Reset),
            (self.skip, Hit::Skip),
            (self.settings, Hit::Settings),
        ]
        .into_iter()
        .find(|(r, _)| contains(r, x, y))
        .map(|(_, h)| h)
    }
}

/// A snapshot of everything the main window shows.
pub struct View {
    pub mode: Mode,
    pub phase: Phase,
    pub time: String,
    pub session: u32,
    pub dots: (u32, u32),
    pub hover: Option<Hit>,
    pub pressed: Option<Hit>,
}

impl View {
    fn start_label(&self) -> &'static str {
        match self.phase {
            Phase::Idle => "Start",
            Phase::Running => "Pause",
            Phase::Paused => "Resume",
        }
    }
}

/// Draw the main window. Returns false if the render target was lost and must be recreated.
pub fn paint(s: &Surface, gfx: &Gfx, l: &Layout, v: &View) -> bool {
    let bg: Rgb = mode_color(v.mode);
    let t = &gfx.text;
    unsafe { s.rt.BeginDraw() };
    s.clear(bg.d2d(1.0));

    for (i, mode) in Mode::ALL.iter().enumerate() {
        let r = l.tabs[i];
        let selected = *mode == v.mode;
        if selected {
            s.fill_round(r, 16.0, white(0.20));
        } else if v.hover == Some(Hit::Tab(*mode)) {
            s.fill_round(r, 16.0, white(0.10));
        }
        let fmt = if selected { &t.tab_selected } else { &t.tab };
        s.text(mode.label(), fmt, r, white(if selected { 1.0 } else { 0.82 }));
    }

    // Each character gets a fixed-width cell, so the time never shifts sideways as it counts.
    let cell = |c: char| if c == ':' { l.colon_w } else { l.digit_w };
    let total: f32 = v.time.chars().map(cell).sum();
    let mut x = (WIDTH - total) / 2.0;
    for c in v.time.chars() {
        let w = cell(c);
        s.text(&c.to_string(), &t.timer, rect(x, l.timer.top, x + w, l.timer.bottom), white(1.0));
        x += w;
    }

    // Start / Pause: a white button with a soft shadow that sinks when pressed.
    let pressed = v.pressed == Some(Hit::Start) && v.hover == Some(Hit::Start);
    let hovered = v.hover == Some(Hit::Start);
    let r = if pressed { offset(l.start, 0.0, 3.0) } else { l.start };
    if !pressed {
        s.fill_round(offset(l.start, 0.0, 4.0), 12.0, black(0.16));
    }
    s.fill_round(r, 12.0, white(if hovered { 0.94 } else { 1.0 }));
    s.text(v.start_label(), &t.button, r, bg.d2d(1.0));

    for (hit, r, glyph) in [
        (Hit::Reset, l.reset, GLYPH_RESET),
        (Hit::Skip, l.skip, GLYPH_SKIP),
        (Hit::Settings, l.settings, GLYPH_SETTINGS),
    ] {
        if v.hover == Some(hit) {
            let radius = (r.right - r.left) / 2.0;
            let a = if v.pressed == Some(hit) { 0.24 } else { 0.14 };
            s.fill_round(r, radius, white(a));
        }
        s.text(glyph, &t.glyph, r, white(0.92));
    }

    // Footer: session number and dots showing progress towards the long break.
    s.text(&format!("#{}", v.session), &t.footer, l.session, white(0.92));
    let (filled, total) = v.dots;
    let d = 8.0;
    let gap = 6.0;
    for i in 0..total.min(12) {
        let x = l.dots_x + i as f32 * (d + gap);
        let dot = rect(x, l.dots_y - d / 2.0, x + d, l.dots_y + d / 2.0);
        s.fill_round(dot, d / 2.0, white(if i < filled { 0.92 } else { 0.32 }));
    }

    !matches!(unsafe { s.rt.EndDraw(None, None) }, Err(e) if e.code() == D2DERR_RECREATE_TARGET)
}
