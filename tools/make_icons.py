#!/usr/bin/env python3
"""Draws the WillPomo icon and writes every size Windows and the Store need.

    pip install pillow
    python tools/make_icons.py

Outputs (all committed, so builds don't need Python):
    res/pomobar.ico                     the .exe icon
    packaging/Assets/*.png              MSIX logos at each scale / target size
    store/box-art-2160.png              optional 1:1 Store listing artwork

The design: a tomato with a white progress bar across it, a nod to the
progress bar WillPomo shows on its taskbar button.
"""

import io
import struct
from pathlib import Path

from PIL import Image, ImageDraw, ImageFilter

ROOT = Path(__file__).resolve().parent.parent
S = 1024  # master canvas

RED_TOP = (232, 90, 69)
RED_BOTTOM = (190, 56, 45)
GREEN = (63, 163, 91)
GREEN_DARK = (44, 128, 68)


def lerp(a, b, t):
    return tuple(round(x + (y - x) * t) for x, y in zip(a, b))


def draw_master(small: bool) -> Image.Image:
    """Full artwork at 1024 px. `small` makes details chunkier for 16-32 px icons."""
    img = Image.new("RGBA", (S, S), (0, 0, 0, 0))

    # Tomato body: a slightly squat circle with a top-to-bottom gradient.
    body = (92, 210, 932, 958)
    grad = Image.new("RGBA", (S, S))
    gd = ImageDraw.Draw(grad)
    for y in range(S):
        t = min(max((y - body[1]) / (body[3] - body[1]), 0.0), 1.0)
        gd.line([(0, y), (S, y)], fill=lerp(RED_TOP, RED_BOTTOM, t) + (255,))
    mask = Image.new("L", (S, S), 0)
    ImageDraw.Draw(mask).ellipse(body, fill=255)
    img.paste(grad, (0, 0), mask)

    # Soft highlight, upper left.
    if not small:
        hl = Image.new("L", (S, S), 0)
        ImageDraw.Draw(hl).ellipse((230, 330, 470, 520), fill=70)
        hl = hl.filter(ImageFilter.GaussianBlur(40))
        white = Image.new("RGBA", (S, S), (255, 255, 255, 255))
        img.paste(white, (0, 0), Image.composite(hl, Image.new("L", (S, S), 0), mask))

    d = ImageDraw.Draw(img)

    # Progress bar: a translucent track with a solid white fill.
    bar_h = 150 if small else 120
    cy = 640
    track = (232, cy - bar_h // 2, 792, cy + bar_h // 2)
    fill = (232, cy - bar_h // 2, 232 + int((792 - 232) * 0.62), cy + bar_h // 2)
    overlay = Image.new("RGBA", (S, S), (0, 0, 0, 0))
    od = ImageDraw.Draw(overlay)
    od.rounded_rectangle(track, radius=bar_h // 2, fill=(255, 255, 255, 110))
    od.rounded_rectangle(fill, radius=bar_h // 2, fill=(255, 255, 255, 255))
    img.alpha_composite(overlay)

    # Calyx: five pointed leaves around the top, plus a short stem.
    cx, top = 512, 236
    leaves = [
        [(cx, top + 40), (cx - 250, top - 10), (cx - 70, top + 95)],
        [(cx, top + 40), (cx + 250, top - 10), (cx + 70, top + 95)],
        [(cx, top + 40), (cx - 150, top + 150), (cx - 10, top + 105)],
        [(cx, top + 40), (cx + 150, top + 150), (cx + 10, top + 105)],
        [(cx - 60, top + 60), (cx, top - 60), (cx + 60, top + 60)],
    ]
    if small:
        leaves = leaves[:2] + [leaves[4]]
    for poly in leaves:
        d.polygon(poly, fill=GREEN)
    d.ellipse((cx - 70, top - 10, cx + 70, top + 110), fill=GREEN)
    stem_w = 70 if small else 56
    d.rounded_rectangle((cx - stem_w // 2, 130, cx + stem_w // 2, top + 40), radius=stem_w // 2, fill=GREEN_DARK)
    return img


MASTER = draw_master(small=False)
MASTER_SMALL = draw_master(small=True)


def icon(px: int) -> Image.Image:
    src = MASTER_SMALL if px <= 32 else MASTER
    return src.resize((px, px), Image.LANCZOS)


def on_canvas(w: int, h: int, scale: float) -> Image.Image:
    """The icon centred on a transparent canvas, `scale` of the shorter side."""
    side = round(min(w, h) * scale)
    canvas = Image.new("RGBA", (w, h), (0, 0, 0, 0))
    canvas.alpha_composite(icon(side), ((w - side) // 2, (h - side) // 2))
    return canvas


def write_ico(path: Path, sizes):
    images = []
    for px in sizes:
        buf = io.BytesIO()
        icon(px).save(buf, "PNG")
        images.append((px, buf.getvalue()))
    header = struct.pack("<HHH", 0, 1, len(images))
    offset = 6 + 16 * len(images)
    entries, blobs = b"", b""
    for px, data in images:
        dim = 0 if px >= 256 else px
        entries += struct.pack("<BBBBHHII", dim, dim, 0, 0, 1, 32, len(data), offset)
        blobs += data
        offset += len(data)
    path.write_bytes(header + entries + blobs)


def main():
    (ROOT / "res").mkdir(exist_ok=True)
    write_ico(ROOT / "res" / "pomobar.ico", [16, 20, 24, 32, 40, 48, 64, 256])

    assets = ROOT / "packaging" / "Assets"
    assets.mkdir(parents=True, exist_ok=True)
    for old in assets.glob("*.png"):
        old.unlink()

    def save(img, name):
        img.save(assets / name, optimize=True)

    for scale in (100, 125, 150, 200, 400):
        k = scale / 100
        save(icon(round(44 * k)), f"Square44x44Logo.scale-{scale}.png")
        save(on_canvas(round(150 * k), round(150 * k), 0.6), f"Square150x150Logo.scale-{scale}.png")
        save(icon(round(50 * k)), f"StoreLogo.scale-{scale}.png")
    for scale in (100, 200):
        k = scale / 100
        save(on_canvas(round(310 * k), round(150 * k), 0.66), f"Wide310x150Logo.scale-{scale}.png")
        save(on_canvas(round(620 * k), round(300 * k), 0.5), f"SplashScreen.scale-{scale}.png")
    # Taskbar / Start icons. "unplated" = drawn without a coloured tile behind it.
    for px in (16, 20, 24, 32, 40, 48, 64, 256):
        for suffix in ("", "_altform-unplated", "_altform-lightunplated"):
            save(icon(px), f"Square44x44Logo.targetsize-{px}{suffix}.png")

    store = ROOT / "store"
    store.mkdir(exist_ok=True)
    art = Image.new("RGBA", (2160, 2160), (250, 240, 236, 255))
    art.alpha_composite(icon(1300), (430, 430))
    art.convert("RGB").save(store / "box-art-2160.png", optimize=True)
    print("Icons written.")


if __name__ == "__main__":
    main()
