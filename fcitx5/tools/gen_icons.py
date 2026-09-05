#!/usr/bin/env python3
"""Regenerate the PSKK fcitx5 input-mode indicator icons.

Produces hicolor theme icons "pskk-hiragana" ("あ") and
"pskk-alphanumeric" ("A") used by the fcitx5 addon's subModeIconImpl().

Usage:  python3 fcitx5/tools/gen_icons.py
Output: fcitx5/data/icons/hicolor/{22,24,32,48,64,96}x{22,24,32,48,64,96}/apps/*.png
"""
import os
from pathlib import Path

from PIL import Image, ImageDraw, ImageFont

OUT_ROOT = Path(__file__).resolve().parent.parent / "data" / "icons" / "hicolor"
SIZES = [22, 24, 32, 48, 64, 96]
GLYPHS = {
    "pskk-hiragana": "あ",
    "pskk-alphanumeric": "A",
}
# Tile colors: a medium blue reads on both light and dark panels.
TILE_COLOR = (72, 130, 255, 255)
GLYPH_COLOR = (255, 255, 255, 255)


def font_path() -> str:
    import subprocess

    out = subprocess.run(
        ["fc-match", "-f", "%{file}", ":lang=ja"], capture_output=True, text=True
    ).stdout.strip()
    if out and os.path.exists(out):
        return out
    # Fallback: any installed font covering Japanese
    return "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc"


def rounded_rect(draw: ImageDraw.ImageDraw, xy, radius, fill):
    draw.rounded_rectangle(xy, radius=radius, fill=fill)


def main() -> None:
    font_file = font_path()
    print(f"font: {font_file}")
    for size in SIZES:
        for name, glyph in GLYPHS.items():
            img = Image.new("RGBA", (size, size), (0, 0, 0, 0))
            draw = ImageDraw.Draw(img)

            pad = max(1, int(size * 0.03))
            rounded_rect(
                draw,
                (pad, pad, size - pad - 1, size - pad - 1),
                radius=int(size * 0.22),
                fill=TILE_COLOR,
            )

            px = int(size * 0.62)
            try:
                font = ImageFont.truetype(font_file, px)
            except OSError:
                font = ImageFont.load_default()
            bbox = draw.textbbox((0, 0), glyph, font=font)
            w, h = bbox[2] - bbox[0], bbox[3] - bbox[1]
            x = (size - w) / 2 - bbox[0]
            y = (size - h) / 2 - bbox[1] + int(size * 0.02)
            draw.text((x, y), glyph, font=font, fill=GLYPH_COLOR)

            out = OUT_ROOT / f"{size}x{size}" / "apps" / f"{name}.png"
            out.parent.mkdir(parents=True, exist_ok=True)
            img.save(out)
            print(f"  {out} ({size}x{size})")


if __name__ == "__main__":
    main()
