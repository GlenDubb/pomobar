//! Small icons drawn at runtime: the minutes badge (taskbar overlay and tray)
//! and the white glyphs on the taskbar thumbnail buttons.
//!
//! Text is rasterised with GDI (grey-scale anti-aliasing) into a coverage mask,
//! centred on its actual ink, then composited in software into a 32-bit
//! premultiplied-alpha bitmap. That gives crisp, transparent icons at any size.

use windows::core::HSTRING;
use windows::Win32::Foundation::RECT;
use windows::Win32::Graphics::Gdi::{
    CreateBitmap, CreateCompatibleDC, CreateDIBSection, CreateFontW, DeleteDC, DeleteObject, DrawTextW,
    GdiFlush, GetDC, ReleaseDC, SelectObject, SetBkMode, SetTextColor,
    ANTIALIASED_QUALITY, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, CLIP_DEFAULT_PRECIS, DEFAULT_CHARSET,
    DIB_RGB_COLORS, DT_CENTER, DT_NOPREFIX, DT_SINGLELINE, DT_VCENTER, FW_BOLD, FW_NORMAL, HBITMAP,
    HDC, OUT_TT_PRECIS, TRANSPARENT,
};
use windows::Win32::UI::WindowsAndMessaging::{CreateIconIndirect, DestroyIcon, HICON, ICONINFO};

use super::util::Rgb;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Shape {
    Circle,
    RoundedSquare,
}

/// White text (e.g. "24") on a coloured shape.
pub fn badge(size: i32, text: &str, bg: Rgb, shape: Shape) -> Option<HICON> {
    let size = size.clamp(8, 256);
    // Digits fill most of the badge; the circle needs a little more margin.
    let (max_w, max_h) = match shape {
        Shape::Circle => (size as f32 * 0.78, size as f32 * 0.56),
        Shape::RoundedSquare => (size as f32 * 0.94, size as f32 * 0.66),
    };
    let mask = text_mask(size, text, "Segoe UI", true, max_w, max_h)?;
    let mut px = vec![0u8; (size * size * 4) as usize];
    let half = size as f32 / 2.0;
    let radius = size as f32 * 0.22;
    for y in 0..size {
        for x in 0..size {
            let (fx, fy) = (x as f32 + 0.5, y as f32 + 0.5);
            let shape_cov = match shape {
                Shape::Circle => {
                    let d = ((fx - half).powi(2) + (fy - half).powi(2)).sqrt();
                    (half - d + 0.5).clamp(0.0, 1.0)
                }
                Shape::RoundedSquare => {
                    let qx = (fx - half).abs() - (half - radius);
                    let qy = (fy - half).abs() - (half - radius);
                    let outside = (qx.max(0.0).powi(2) + qy.max(0.0).powi(2)).sqrt();
                    let dist = outside + qx.max(qy).min(0.0) - radius;
                    (0.5 - dist).clamp(0.0, 1.0)
                }
            };
            let i = (y * size + x) as usize;
            let t = f32::from(mask[i]) / 255.0 * shape_cov;
            let a = shape_cov * (1.0 - t) + t;
            let mix = |c: u8| (f32::from(c) / 255.0 * shape_cov * (1.0 - t) + t) * 255.0;
            px[i * 4] = mix(bg.2).round() as u8;
            px[i * 4 + 1] = mix(bg.1).round() as u8;
            px[i * 4 + 2] = mix(bg.0).round() as u8;
            px[i * 4 + 3] = (a * 255.0).round() as u8;
        }
    }
    make_icon(size, &px)
}

/// A single white icon-font glyph on a transparent background.
pub fn glyph(size: i32, glyph: &str) -> Option<HICON> {
    let size = size.clamp(8, 256);
    let font = if font_exists("Segoe Fluent Icons") { "Segoe Fluent Icons" } else { "Segoe MDL2 Assets" };
    let mask = text_mask(size, glyph, font, false, size as f32 * 0.8, size as f32 * 0.8)?;
    let mut px = vec![0u8; (size * size * 4) as usize];
    for (i, &m) in mask.iter().enumerate() {
        px[i * 4..i * 4 + 4].copy_from_slice(&[m, m, m, m]);
    }
    make_icon(size, &px)
}

pub fn destroy(icon: Option<HICON>) {
    if let Some(i) = icon {
        unsafe {
            let _ = DestroyIcon(i);
        }
    }
}

fn font_exists(face: &str) -> bool {
    // GDI substitutes a fallback face for missing fonts; compare what we got.
    unsafe {
        let screen = GetDC(None);
        let dc = CreateCompatibleDC(Some(screen));
        ReleaseDC(None, screen);
        let font = create_font(face, 16, false);
        let old = SelectObject(dc, font.into());
        let mut buf = [0u16; 64];
        windows::Win32::Graphics::Gdi::GetTextFaceW(dc, Some(&mut buf));
        SelectObject(dc, old);
        let _ = DeleteObject(font.into());
        let _ = DeleteDC(dc);
        let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
        String::from_utf16_lossy(&buf[..len]).eq_ignore_ascii_case(face)
    }
}

unsafe fn create_font(face: &str, px: i32, bold: bool) -> windows::Win32::Graphics::Gdi::HFONT {
    CreateFontW(
        -px,
        0,
        0,
        0,
        if bold { FW_BOLD.0 as i32 } else { FW_NORMAL.0 as i32 },
        0,
        0,
        0,
        DEFAULT_CHARSET,
        OUT_TT_PRECIS,
        CLIP_DEFAULT_PRECIS,
        ANTIALIASED_QUALITY,
        0,
        &HSTRING::from(face),
    )
}

struct Dib {
    dc: HDC,
    bmp: HBITMAP,
    bits: *mut u8,
    w: i32,
    h: i32,
}

impl Dib {
    unsafe fn new(w: i32, h: i32) -> Option<Dib> {
        let screen = GetDC(None);
        let dc = CreateCompatibleDC(Some(screen));
        ReleaseDC(None, screen);
        let bmi = bitmap_info(w, h);
        let mut bits = std::ptr::null_mut();
        let bmp = CreateDIBSection(Some(dc), &bmi, DIB_RGB_COLORS, &mut bits, None, 0).ok()?;
        SelectObject(dc, bmp.into());
        std::ptr::write_bytes(bits as *mut u8, 0, (w * h * 4) as usize);
        Some(Dib { dc, bmp, bits: bits as *mut u8, w, h })
    }

    fn pixels(&self) -> &[u8] {
        unsafe { std::slice::from_raw_parts(self.bits, (self.w * self.h * 4) as usize) }
    }
}

impl Drop for Dib {
    fn drop(&mut self) {
        unsafe {
            let _ = DeleteDC(self.dc);
            let _ = DeleteObject(self.bmp.into());
        }
    }
}

fn bitmap_info(w: i32, h: i32) -> BITMAPINFO {
    BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: w,
            biHeight: -h, // top-down
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        },
        ..Default::default()
    }
}

/// Render `text` white-on-black, pick the biggest font whose ink fits in
/// max_w x max_h, and return a size x size coverage mask with the ink centred.
fn text_mask(size: i32, text: &str, face: &str, bold: bool, max_w: f32, max_h: f32) -> Option<Vec<u8>> {
    let canvas = size * 3;
    let mut chars: Vec<u16> = text.encode_utf16().collect();
    unsafe {
        let dib = Dib::new(canvas, canvas)?;
        let mut px = size * 2;
        loop {
            std::ptr::write_bytes(dib.bits, 0, (canvas * canvas * 4) as usize);
            let font = create_font(face, px, bold);
            let old = SelectObject(dib.dc, font.into());
            SetBkMode(dib.dc, TRANSPARENT);
            SetTextColor(dib.dc, windows::Win32::Foundation::COLORREF(0x00FF_FFFF));
            let mut rc = RECT { left: 0, top: 0, right: canvas, bottom: canvas };
            DrawTextW(dib.dc, &mut chars, &mut rc, DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX);
            let _ = GdiFlush();
            SelectObject(dib.dc, old);
            let _ = DeleteObject(font.into());

            let ink = ink_bounds(dib.pixels(), canvas);
            let fits = match ink {
                Some((l, t, r, b)) => ((r - l) as f32) <= max_w && ((b - t) as f32) <= max_h,
                None => true,
            };
            if fits || px <= 5 {
                let mut mask = vec![0u8; (size * size) as usize];
                if let Some((l, t, r, b)) = ink {
                    // Shift so the ink's centre lands on the icon's centre (whole pixels keep it crisp).
                    let dx = (size - (r - l)) / 2 - l;
                    let dy = (size - (b - t)) / 2 - t;
                    let src = dib.pixels();
                    for y in 0..size {
                        for x in 0..size {
                            let (sx, sy) = (x - dx, y - dy);
                            if sx >= 0 && sy >= 0 && sx < canvas && sy < canvas {
                                mask[(y * size + x) as usize] = src[((sy * canvas + sx) * 4 + 1) as usize];
                            }
                        }
                    }
                }
                return Some(mask);
            }
            px -= 1;
        }
    }
}

/// (left, top, right, bottom) of non-zero pixels, right/bottom exclusive.
fn ink_bounds(px: &[u8], w: i32) -> Option<(i32, i32, i32, i32)> {
    let (mut l, mut t, mut r, mut b) = (w, w, -1, -1);
    for y in 0..w {
        for x in 0..w {
            if px[((y * w + x) * 4 + 1) as usize] > 24 {
                l = l.min(x);
                t = t.min(y);
                r = r.max(x);
                b = b.max(y);
            }
        }
    }
    (r >= 0).then_some((l, t, r + 1, b + 1))
}

fn make_icon(size: i32, premultiplied_bgra: &[u8]) -> Option<HICON> {
    unsafe {
        let bmi = bitmap_info(size, size);
        let mut bits = std::ptr::null_mut();
        let color = CreateDIBSection(None, &bmi, DIB_RGB_COLORS, &mut bits, None, 0).ok()?;
        std::ptr::copy_nonoverlapping(premultiplied_bgra.as_ptr(), bits as *mut u8, premultiplied_bgra.len());
        let mask_bytes = vec![0u8; (((size + 15) / 16 * 2) * size) as usize];
        let mask = CreateBitmap(size, size, 1, 1, Some(mask_bytes.as_ptr() as *const _));
        let info = ICONINFO {
            fIcon: true.into(),
            xHotspot: 0,
            yHotspot: 0,
            hbmMask: mask,
            hbmColor: color,
        };
        let icon = CreateIconIndirect(&info).ok();
        let _ = DeleteObject(color.into());
        let _ = DeleteObject(mask.into());
        icon
    }
}
