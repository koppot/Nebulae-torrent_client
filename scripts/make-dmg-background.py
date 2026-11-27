#!/usr/bin/env python3
"""
make-dmg-background.py
Produces ~/Downloads/dmg-background.png  (660x400)
       ~/Downloads/dmg-background@2x.png (1320x800)
for the Nebulae macOS installer window.
"""

import hashlib
import os
import sys
from pathlib import Path

# ---------------------------------------------------------------------------
# Config
# ---------------------------------------------------------------------------
ART_SRC   = "/Users/dev/.gemini/antigravity/brain/0c27f4b1-e2f2-4b96-b87b-1f7954bb2320/nebula_art_1795808378943.jpg"
OUT_2X    = Path.home() / "Downloads" / "dmg-background@2x.png"
OUT_1X    = Path.home() / "Downloads" / "dmg-background.png"
FONT_DIR  = Path("/Users/dev/Library/Fonts")
SYS_FONTS = Path("/System/Library/Fonts")

W, H = 1320, 800          # @2x canvas
ZONE_HALF = 128            # half of 256
ZONE_CENTERS = [(360, 380), (960, 380)]

# ---------------------------------------------------------------------------
# Imports
# ---------------------------------------------------------------------------
try:
    from PIL import Image, ImageDraw, ImageFont, ImageFilter
except ImportError:
    sys.exit("Pillow not found — run this inside the nebula venv.")

# ---------------------------------------------------------------------------
# Step 1 – Load and resize artwork to 1320x800
# ---------------------------------------------------------------------------
print("[1] Loading artwork …")
art = Image.open(ART_SRC).convert("RGB")
art = art.resize((W, H), Image.LANCZOS)
print(f"    Art size after resize: {art.size}")

# ---------------------------------------------------------------------------
# Step 2 – Darken to keep background around #141414–#1c1c1c
#           Blend 60 % toward solid #141414
# ---------------------------------------------------------------------------
print("[2] Darkening art …")
dark_base = Image.new("RGB", (W, H), (20, 20, 20))   # #141414
art_dark  = Image.blend(art, dark_base, alpha=0.72)   # 72 % toward dark

# Optional: apply a very gentle Gaussian blur to soften any JPEG artefacts
art_dark = art_dark.filter(ImageFilter.GaussianBlur(radius=1.5))

# Work on a copy from here so we never alter the snapshot reference
canvas = art_dark.copy()

# Keep pixel-exact copies of the two zone areas from the CANVAS (pre-draw)
# so we can verify nothing was drawn inside them later.
zone_snapshots = {}
for cx, cy in ZONE_CENTERS:
    x0, y0 = cx - ZONE_HALF, cy - ZONE_HALF
    x1, y1 = cx + ZONE_HALF, cy + ZONE_HALF
    zone_snapshots[(cx, cy)] = canvas.crop((x0, y0, x1, y1)).tobytes()
draw   = ImageDraw.Draw(canvas)

# ---------------------------------------------------------------------------
# Helper: load a TrueType font at the given size (bold preferred)
# ---------------------------------------------------------------------------
def load_font(size: int, bold: bool = False) -> ImageFont.FreeTypeFont:
    candidates = []
    if bold:
        candidates = [
            FONT_DIR / "HelveticaNeueLT-Bold.ttf",
            FONT_DIR / "HelveticaNeueUI-CondensedBold.ttf",
            SYS_FONTS / "HelveticaNeue.ttc",
        ]
    else:
        candidates = [
            FONT_DIR / "HelveticaNeueLT-Roman.ttf",
            FONT_DIR / "HelveticaNeueDeskInterface-Regular.ttf",
            SYS_FONTS / "HelveticaNeue.ttc",
            SYS_FONTS / "Helvetica.ttc",
        ]
    for path in candidates:
        if path.exists():
            try:
                return ImageFont.truetype(str(path), size)
            except Exception:
                continue
    # Last resort: PIL default
    return ImageFont.load_default()

# ---------------------------------------------------------------------------
# Helper: draw text centered at (cx, cy) or left-aligned at x
# ---------------------------------------------------------------------------
def draw_centered(text, cx, cy, font, color=(255, 255, 255)):
    bbox = draw.textbbox((0, 0), text, font=font)
    tw, th = bbox[2] - bbox[0], bbox[3] - bbox[1]
    draw.text((cx - tw // 2, cy - th // 2), text, font=font, fill=color)

# ---------------------------------------------------------------------------
# Step 3 – Title  "Install Nebulae"  top center ~y=70
# ---------------------------------------------------------------------------
print("[3] Drawing title …")
title_font = load_font(44, bold=True)
draw_centered("Install Nebulae", W // 2, 70, title_font)

# ---------------------------------------------------------------------------
# Step 4 – Arrow label above arrow  ~y=330, arrow at y=380
#           Arrow spans x=500 → x=820  (safely outside both 256-px zones)
#           Zone edges: left zone right edge  = 360+128=488
#                       right zone left edge  = 960-128=832
#           Arrow x-range 500–820 is safely within the gap.
# ---------------------------------------------------------------------------
print("[4] Drawing drag arrow …")
label_font  = load_font(26, bold=False)
arrow_color = (229, 16, 43)   # #E5102B red

label_text = "Step 1: Drag Nebulae to Applications"
# Keep label ABOVE the zone band (zones span y=252..508); use y=215
draw_centered(label_text, W // 2, 215, label_font)

# Draw arrow  500 → 820 at y=380
AX0, AX1, AY = 500, 820, 380
arrow_w = 3                # shaft thickness half-width
head_len = 28
head_w   = 14

# Shaft
draw.rectangle([AX0, AY - 2, AX1 - head_len, AY + 2], fill=arrow_color)
# Arrowhead (triangle)
head_pts = [
    (AX1,         AY),
    (AX1 - head_len, AY - head_w),
    (AX1 - head_len, AY + head_w),
]
draw.polygon(head_pts, fill=arrow_color)

# ---------------------------------------------------------------------------
# Step 5 – Bottom instruction block  starting ~y=600
# ---------------------------------------------------------------------------
print("[5] Drawing instructions …")
body_font  = load_font(24, bold=False)
LINE_H     = 38
MARGIN     = 60
lines = [
    "Step 2: Open Nebulae. If macOS says it cannot verify the app, click Done.",
    "Step 3: System Settings > Privacy & Security > Open Anyway.",
    "Details: READ ME FIRST.txt in this window",
]
y_start = 600
for i, line in enumerate(lines):
    bbox = draw.textbbox((0, 0), line, font=body_font)
    tw   = bbox[2] - bbox[0]
    # Center the text
    x    = (W - tw) // 2
    # Safety: clamp to margin
    x    = max(MARGIN, x)
    draw.text((x, y_start + i * LINE_H), line, font=body_font, fill=(255, 255, 255))

# ---------------------------------------------------------------------------
# Step 6 – Save @2x  (no EXIF, RGB PNG)
# ---------------------------------------------------------------------------
print("[6] Saving @2x …")
canvas_rgb = canvas.convert("RGB")
canvas_rgb.save(str(OUT_2X), format="PNG", optimize=False)
print(f"    Saved {OUT_2X}  size={canvas_rgb.size}")

# ---------------------------------------------------------------------------
# Step 7 – Downscale to 660x400 for 1x
# ---------------------------------------------------------------------------
print("[7] Saving 1x …")
img_1x = canvas_rgb.resize((660, 400), Image.LANCZOS)
img_1x.save(str(OUT_1X), format="PNG", optimize=False)
print(f"    Saved {OUT_1X}  size={img_1x.size}")

# ---------------------------------------------------------------------------
# Step 8 – Verification
# ---------------------------------------------------------------------------
print("\n[VERIFY] ── Dimensions ──────────────────────────────")
for path, expected in [(OUT_2X, (1320, 800)), (OUT_1X, (660, 400))]:
    img  = Image.open(path)
    ok   = "✓" if img.size == expected else "✗ FAIL"
    print(f"  {ok}  {path.name}: {img.size}")

print("\n[VERIFY] ── Zone integrity (nothing drawn inside) ───")
# Compare zone pixels on the in-memory canvas (before PNG round-trip)
# against the pre-draw snapshots taken from the same canvas object.
for cx, cy in ZONE_CENTERS:
    x0, y0 = cx - ZONE_HALF, cy - ZONE_HALF
    x1, y1 = cx + ZONE_HALF, cy + ZONE_HALF
    crop_bytes = canvas_rgb.crop((x0, y0, x1, y1)).tobytes()
    ref_bytes  = zone_snapshots[(cx, cy)]
    if crop_bytes == ref_bytes:
        print(f"  ✓  Zone centred at ({cx},{cy}): unchanged")
    else:
        # Detailed pixel diff to locate the modified region
        from PIL import Image as _Img
        ref_img  = _Img.frombytes("RGB", (256, 256), ref_bytes)
        crop_img = _Img.frombytes("RGB", (256, 256), crop_bytes)
        diff_pixels = [(x, y) for x in range(256) for y in range(256)
                       if ref_img.getpixel((x,y)) != crop_img.getpixel((x,y))]
        print(f"  ✗  Zone centred at ({cx},{cy}): {len(diff_pixels)} pixel(s) changed!")

print("\n[VERIFY] ── SHA-256 & file sizes ───────────────────")
for path in [OUT_2X, OUT_1X]:
    data = path.read_bytes()
    sha  = hashlib.sha256(data).hexdigest()
    print(f"  {path.name}")
    print(f"    size   : {len(data):,} bytes")
    print(f"    sha256 : {sha}")

print("\nDone.")
