//! Direct2D + DirectWrite: shared factories, text styles, and a per-window drawing surface.

use windows::core::{Result, HSTRING};
use windows::Win32::Foundation::{HWND, RECT};
use windows::Win32::Graphics::Direct2D::Common::{
    D2D1_ALPHA_MODE_UNKNOWN, D2D1_COLOR_F, D2D1_PIXEL_FORMAT, D2D_RECT_F, D2D_SIZE_U,
};
use windows::Win32::Graphics::Direct2D::{
    D2D1CreateFactory, ID2D1Factory, ID2D1HwndRenderTarget, ID2D1SolidColorBrush, ID2D1StrokeStyle,
    D2D1_DRAW_TEXT_OPTIONS_NONE, D2D1_FACTORY_TYPE_SINGLE_THREADED, D2D1_FEATURE_LEVEL_DEFAULT,
    D2D1_HWND_RENDER_TARGET_PROPERTIES, D2D1_PRESENT_OPTIONS_NONE, D2D1_RENDER_TARGET_PROPERTIES,
    D2D1_RENDER_TARGET_TYPE_DEFAULT, D2D1_RENDER_TARGET_USAGE_NONE, D2D1_ROUNDED_RECT,
};
use windows::Win32::Graphics::DirectWrite::{
    DWriteCreateFactory, IDWriteFactory, IDWriteFontCollection, IDWriteTextFormat,
    DWRITE_FACTORY_TYPE_SHARED, DWRITE_FONT_STRETCH_NORMAL, DWRITE_FONT_STYLE_NORMAL,
    DWRITE_FONT_WEIGHT, DWRITE_FONT_WEIGHT_NORMAL, DWRITE_FONT_WEIGHT_SEMI_BOLD,
    DWRITE_MEASURING_MODE_NATURAL, DWRITE_PARAGRAPH_ALIGNMENT_CENTER, DWRITE_TEXT_ALIGNMENT,
    DWRITE_TEXT_ALIGNMENT_CENTER, DWRITE_TEXT_ALIGNMENT_LEADING, DWRITE_TEXT_METRICS,
    DWRITE_WORD_WRAPPING_NO_WRAP, DWRITE_WORD_WRAPPING_WRAP,
};
use windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_UNKNOWN;
use windows::Win32::UI::WindowsAndMessaging::GetClientRect;

/// Text styles used across the app. Sizes are in DIPs (1/96 inch).
pub struct TextStyles {
    pub timer: IDWriteTextFormat,
    pub tab: IDWriteTextFormat,
    pub tab_selected: IDWriteTextFormat,
    pub button: IDWriteTextFormat,
    pub footer: IDWriteTextFormat,
    pub glyph: IDWriteTextFormat,
    pub heading: IDWriteTextFormat,
    pub body: IDWriteTextFormat,
    pub popup_button: IDWriteTextFormat,
}

pub struct Gfx {
    pub factory: ID2D1Factory,
    pub dwrite: IDWriteFactory,
    pub text: TextStyles,
}

impl Gfx {
    pub fn new() -> Result<Gfx> {
        unsafe {
            let factory: ID2D1Factory = D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED, None)?;
            let dwrite: IDWriteFactory = DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED)?;

            // Windows 11 fonts first, falling back to their Windows 10 equivalents.
            let display = pick_family(&dwrite, &["Segoe UI Variable Display", "Segoe UI"]);
            let ui = pick_family(&dwrite, &["Segoe UI Variable Text", "Segoe UI"]);
            let icons = pick_family(&dwrite, &["Segoe Fluent Icons", "Segoe MDL2 Assets"]);

            let semi = DWRITE_FONT_WEIGHT_SEMI_BOLD;
            let normal = DWRITE_FONT_WEIGHT_NORMAL;
            let center = DWRITE_TEXT_ALIGNMENT_CENTER;
            let leading = DWRITE_TEXT_ALIGNMENT_LEADING;
            let text = TextStyles {
                timer: format(&dwrite, &display, semi, 92.0, center, false)?,
                tab: format(&dwrite, &ui, normal, 14.0, center, false)?,
                tab_selected: format(&dwrite, &ui, semi, 14.0, center, false)?,
                button: format(&dwrite, &ui, semi, 19.0, center, false)?,
                footer: format(&dwrite, &ui, semi, 14.0, leading, false)?,
                glyph: format(&dwrite, &icons, normal, 18.0, center, false)?,
                heading: format(&dwrite, &display, semi, 22.0, leading, false)?,
                body: format(&dwrite, &ui, normal, 15.0, leading, true)?,
                popup_button: format(&dwrite, &ui, semi, 15.0, center, false)?,
            };
            Ok(Gfx { factory, dwrite, text })
        }
    }

    /// Width in DIPs of `s` laid out on one line.
    pub fn text_width(&self, fmt: &IDWriteTextFormat, s: &str) -> f32 {
        let w: Vec<u16> = s.encode_utf16().collect();
        unsafe {
            let Ok(layout) = self.dwrite.CreateTextLayout(&w, fmt, 10_000.0, 1_000.0) else {
                return 0.0;
            };
            let mut m = DWRITE_TEXT_METRICS::default();
            if layout.GetMetrics(&mut m).is_ok() {
                m.widthIncludingTrailingWhitespace
            } else {
                0.0
            }
        }
    }
}

unsafe fn pick_family(dwrite: &IDWriteFactory, candidates: &[&str]) -> HSTRING {
    let mut collection: Option<IDWriteFontCollection> = None;
    if dwrite.GetSystemFontCollection(&mut collection, false).is_ok() {
        if let Some(collection) = collection {
            for name in candidates {
                let mut index = 0u32;
                let mut exists = windows::core::BOOL::default();
                let h = HSTRING::from(*name);
                if collection.FindFamilyName(&h, &mut index, &mut exists).is_ok() && exists.as_bool() {
                    return h;
                }
            }
        }
    }
    HSTRING::from(*candidates.last().unwrap_or(&"Segoe UI"))
}

unsafe fn format(
    dwrite: &IDWriteFactory,
    family: &HSTRING,
    weight: DWRITE_FONT_WEIGHT,
    size: f32,
    align: DWRITE_TEXT_ALIGNMENT,
    wrap: bool,
) -> Result<IDWriteTextFormat> {
    let f = dwrite.CreateTextFormat(
        family,
        None,
        weight,
        DWRITE_FONT_STYLE_NORMAL,
        DWRITE_FONT_STRETCH_NORMAL,
        size,
        &HSTRING::from("en-us"),
    )?;
    f.SetTextAlignment(align)?;
    f.SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_CENTER)?;
    f.SetWordWrapping(if wrap { DWRITE_WORD_WRAPPING_WRAP } else { DWRITE_WORD_WRAPPING_NO_WRAP })?;
    Ok(f)
}

/// A window's Direct2D render target plus one reusable brush.
/// Coordinates are DIPs; the render target's DPI does the scaling.
pub struct Surface {
    pub rt: ID2D1HwndRenderTarget,
    brush: ID2D1SolidColorBrush,
}

impl Surface {
    pub fn new(gfx: &Gfx, hwnd: HWND, dpi: u32) -> Result<Surface> {
        unsafe {
            let mut rc = RECT::default();
            GetClientRect(hwnd, &mut rc)?;
            let props = D2D1_RENDER_TARGET_PROPERTIES {
                r#type: D2D1_RENDER_TARGET_TYPE_DEFAULT,
                pixelFormat: D2D1_PIXEL_FORMAT {
                    format: DXGI_FORMAT_UNKNOWN,
                    alphaMode: D2D1_ALPHA_MODE_UNKNOWN,
                },
                dpiX: dpi as f32,
                dpiY: dpi as f32,
                usage: D2D1_RENDER_TARGET_USAGE_NONE,
                minLevel: D2D1_FEATURE_LEVEL_DEFAULT,
            };
            let hwnd_props = D2D1_HWND_RENDER_TARGET_PROPERTIES {
                hwnd,
                pixelSize: D2D_SIZE_U {
                    width: (rc.right - rc.left).max(1) as u32,
                    height: (rc.bottom - rc.top).max(1) as u32,
                },
                presentOptions: D2D1_PRESENT_OPTIONS_NONE,
            };
            let rt = gfx.factory.CreateHwndRenderTarget(&props, &hwnd_props)?;
            let brush = rt.CreateSolidColorBrush(&super::util::white(1.0), None)?;
            Ok(Surface { rt, brush })
        }
    }

    pub fn resize(&self, width: u32, height: u32) {
        unsafe {
            let _ = self.rt.Resize(&D2D_SIZE_U {
                width: width.max(1),
                height: height.max(1),
            });
        }
    }

    pub fn set_dpi(&self, dpi: u32) {
        unsafe { self.rt.SetDpi(dpi as f32, dpi as f32) }
    }

    pub fn clear(&self, c: D2D1_COLOR_F) {
        unsafe { self.rt.Clear(Some(&c)) }
    }

    pub fn fill_round(&self, r: D2D_RECT_F, radius: f32, c: D2D1_COLOR_F) {
        unsafe {
            self.brush.SetColor(&c);
            let rr = D2D1_ROUNDED_RECT {
                rect: r,
                radiusX: radius,
                radiusY: radius,
            };
            self.rt.FillRoundedRectangle(&rr, &self.brush);
        }
    }

    pub fn stroke_round(&self, r: D2D_RECT_F, radius: f32, width: f32, c: D2D1_COLOR_F) {
        unsafe {
            self.brush.SetColor(&c);
            let rr = D2D1_ROUNDED_RECT {
                rect: r,
                radiusX: radius,
                radiusY: radius,
            };
            self.rt.DrawRoundedRectangle(&rr, &self.brush, width, None::<&ID2D1StrokeStyle>);
        }
    }

    pub fn text(&self, s: &str, fmt: &IDWriteTextFormat, r: D2D_RECT_F, c: D2D1_COLOR_F) {
        let w: Vec<u16> = s.encode_utf16().collect();
        unsafe {
            self.brush.SetColor(&c);
            self.rt.DrawText(
                &w,
                fmt,
                &r,
                &self.brush,
                D2D1_DRAW_TEXT_OPTIONS_NONE,
                DWRITE_MEASURING_MODE_NATURAL,
            );
        }
    }
}
