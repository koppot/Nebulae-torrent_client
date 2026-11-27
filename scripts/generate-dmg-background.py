#!/usr/bin/env python3
"""Generate assets/dmg-background.png — the macOS dmg window backdrop.

Deterministic: fixed canvas, fixed palette, PIL's default bitmap font, no
timestamps or metadata. Re-run to regenerate the committed file byte for
byte:

    python3 scripts/generate-dmg-background.py

Layout matches the tauri.conf.json dmg window (660x400) with the app icon at
(180, 180) and the Applications folder at (480, 180): title and the three
steps sit in the upper half, the arrow between the two icon drop zones in
the lower half.
"""
from PIL import Image, ImageDraw, ImageFont

W, H = 660, 400
BG = (245, 245, 247, 255)
FG = (40, 40, 46, 255)
MUTED = (110, 110, 120, 255)
ARROW = (70, 130, 200, 255)

OUT = __file__.rsplit("/", 2)[0] + "/assets/dmg-background.png"

img = Image.new("RGB", (W, H), BG)
d = ImageDraw.Draw(img)


def font(size):
    try:
        return ImageFont.load_default(size=size)
    except TypeError:  # older Pillow: fixed bitmap font
        return ImageFont.load_default()


def center(text, y, f, fill):
    w = d.textlength(text, font=f)
    d.text(((W - w) / 2, y), text, font=f, fill=fill)


center("Install Nebulae", 24, font(28), FG)
center("Step 1: Drag Nebulae to Applications", 92, font(18), FG)
center("Step 2: Open it, click Done on the Apple warning", 124, font(18), FG)
center("Step 3: System Settings > Privacy & Security > Open Anyway", 156, font(18), FG)

# Arrow between the app drop zone (x=180) and Applications (x=480).
y = 268
d.line((240, y, 404, y), fill=ARROW, width=6)
d.polygon([(404, y - 16), (404, y + 16), (432, y)], fill=ARROW)
d.rectangle((120, 216, 240, 320), outline=MUTED, width=3)
d.rectangle((420, 216, 540, 320), outline=MUTED, width=3)


def label(text, cx, y, f, fill):
    w = d.textlength(text, font=f)
    d.text((cx - w / 2, y), text, font=f, fill=fill)


label("Nebulae", 180, 330, font(14), MUTED)
label("Applications", 480, 330, font(14), MUTED)

center('Details in "READ ME FIRST.txt"', 368, font(14), MUTED)

img.save(OUT, format="PNG", optimize=False)
print(f"wrote {OUT} ({W}x{H})")
