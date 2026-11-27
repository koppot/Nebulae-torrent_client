#!/usr/bin/env python3
"""Generate the macOS dmg window backdrop to the approved brief.

Deterministic: fixed canvas, fixed palette, no timestamps or metadata.
Re-run to regenerate the committed files byte for byte:

    python3 scripts/make-dmg-background.py

Outputs (2x first so the arrow geometry is identical after halving):
    assets/dmg/dmg-background.png      660x400
    assets/dmg/dmg-background@2x.png  1320x800

Brief: dark charcoal #141414 background, white bold title "Install Nebulae",
two EMPTY 128x128 icon drop zones centered at (180,190) and (480,190), a red
#E5102B arrow from the left zone to the right zone, captions under the zones
and a bottom line pointing to the Open Anyway step.
"""
from PIL import Image, ImageDraw, ImageFont

W, H = 660, 400
BG = (0x14, 0x14, 0x14, 255)
FG = (255, 255, 255, 255)
RED = (0xE5, 0x10, 0x2B, 255)

REPO = __file__.rsplit("/", 2)[0]
OUT_1X = REPO + "/assets/dmg/dmg-background.png"
OUT_2X = REPO + "/assets/dmg/dmg-background@2x.png"


def draw(scale):
    w, h = W * scale, H * scale
    img = Image.new("RGB", (w, h), BG)
    d = ImageDraw.Draw(img)

    def font(size):
        try:
            return ImageFont.load_default(size=size * scale)
        except TypeError:  # older Pillow: fixed bitmap font
            return ImageFont.load_default()

    def center(text, y, f, fill):
        tw = d.textlength(text, font=f)
        d.text(((w - tw) / 2, y), text, font=f, fill=fill)

    # Title, top center.
    center("Install Nebulae", 40 * scale, font(28), FG)

    # Empty 128x128 drop zones centered at (180,190) and (480,190).
    # Left EMPTY on purpose: the app icon lands there at runtime; the
    # Applications folder icon lands on the right zone. Draw nothing inside.
    left = (180 - 64, 190 - 64, 180 + 64, 190 + 64)
    right = (480 - 64, 190 - 64, 480 + 64, 190 + 64)
    s = scale
    d.rectangle([left[0] * s, left[1] * s, left[2] * s, left[3] * s],
                outline=FG, width=3 * s)
    d.rectangle([right[0] * s, right[1] * s, right[2] * s, right[3] * s],
                outline=FG, width=3 * s)

    # Red arrow from the left zone to the right zone, between the zones
    # above the captions.
    y = 190 * s
    d.line((260 * s, y, 386 * s, y), fill=RED, width=6 * s)
    d.polygon([(386 * s, y - 14 * s), (386 * s, y + 14 * s), (412 * s, y)],
              fill=RED)

    # Captions under the zones.
    def label(cx, text, y, f, fill):
        tw = d.textlength(text, font=f)
        d.text((cx * s - tw / 2, y), text, font=f, fill=fill)

    label(180, "1. Drag Nebulae to Applications", 268 * scale, font(16), FG)
    label(480, "2. Open Applications", 268 * scale, font(16), FG)

    # Bottom line.
    center("3. Open Nebulae - first launch: System Settings > Privacy & "
           "Security > Open Anyway", 344 * scale, font(13), FG)
    return img


img2x = draw(2)
img2x.save(OUT_2X, format="PNG", optimize=False)
print(f"wrote {OUT_2X} ({img2x.width}x{img2x.height})")

img1x = draw(1)
img1x.save(OUT_1X, format="PNG", optimize=False)
print(f"wrote {OUT_1X} ({img1x.width}x{img1x.height})")
