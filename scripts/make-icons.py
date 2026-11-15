#!/usr/bin/env python3
"""Draw the kubernet-wizard app icon and export every platform format.

Usage: python3 scripts/make-icons.py   (needs Pillow; macOS for .icns)

Outputs (committed, so CI needs no Pillow):
  assets/icon/icon.png            1024 master
  assets/icon/kubernet-wizard.icns  macOS bundle icon
  assets/icon/kubernet-wizard.ico   Windows exe icon
  assets/icon/hicolor/<n>x<n>.png   Linux launcher sizes
  crates/kw-app/assets/logo.png     in-app logo (256)
"""
import math
import shutil
import subprocess
import tempfile
from pathlib import Path

from PIL import Image, ImageDraw, ImageFilter

ROOT = Path(__file__).resolve().parent.parent
OUT = ROOT / "assets" / "icon"
SS = 4  # supersampling factor
N = 1024 * SS

TOP_LEFT = (59, 91, 219)  # indigo
BOTTOM_RIGHT = (18, 184, 166)  # teal
GOLD = (255, 212, 59)
WHITE = (255, 255, 255, 255)


def s(v: float) -> int:
    return int(round(v * SS))


def gradient(size: int) -> Image.Image:
    small = Image.new("RGB", (256, 256))
    px = small.load()
    for y in range(256):
        for x in range(256):
            t = (x + y) / 510
            px[x, y] = tuple(int(a + (b - a) * t) for a, b in zip(TOP_LEFT, BOTTOM_RIGHT))
    return small.resize((size, size), Image.BICUBIC)


def hexagon(cx: float, cy: float, r: float) -> list[tuple[float, float]]:
    return [(cx + r * math.cos(math.radians(60 * i - 90)), cy + r * math.sin(math.radians(60 * i - 90))) for i in range(6)]


def sparkle(cx: float, cy: float, r: float) -> list[tuple[float, float]]:
    pts = []
    for i in range(8):
        a = math.radians(45 * i - 90)
        rr = r if i % 2 == 0 else r * 0.28
        pts.append((cx + rr * math.cos(a), cy + rr * math.sin(a)))
    return pts


def draw() -> Image.Image:
    img = Image.new("RGBA", (N, N), (0, 0, 0, 0))

    # macOS-style body: 824px rounded square centred in 1024, soft drop shadow.
    body = (s(100), s(100), s(924), s(924))
    radius = s(185)
    shadow = Image.new("RGBA", (N, N), (0, 0, 0, 0))
    ImageDraw.Draw(shadow).rounded_rectangle((body[0], body[1] + s(12), body[2], body[3] + s(12)), radius, fill=(0, 0, 0, 90))
    img.alpha_composite(shadow.filter(ImageFilter.GaussianBlur(s(18))))

    mask = Image.new("L", (N, N), 0)
    ImageDraw.Draw(mask).rounded_rectangle(body, radius, fill=255)
    img.paste(gradient(N), (0, 0), mask)

    d = ImageDraw.Draw(img)
    cx, cy = 512, 530

    # Cluster outline.
    hexa = hexagon(cx, cy, 270)
    # Repeat the first two points so the start corner is joined like the others.
    d.line([(s(x), s(y)) for x, y in hexa + hexa[:2]], fill=WHITE, width=s(34), joint="curve")

    # Three connected nodes.
    nodes = [(cx, cy - 120), (cx - 112, cy + 78), (cx + 112, cy + 78)]
    for i, a in enumerate(nodes):
        b = nodes[(i + 1) % 3]
        d.line([(s(a[0]), s(a[1])), (s(b[0]), s(b[1]))], fill=(255, 255, 255, 220), width=s(22))
    for x, y in nodes:
        d.ellipse((s(x - 50), s(y - 50), s(x + 50), s(y + 50)), fill=WHITE)
        d.ellipse((s(x - 22), s(y - 22), s(x + 22), s(y + 22)), fill=TOP_LEFT + (255,))

    # Wizard sparkles.
    d.polygon([(s(x), s(y)) for x, y in sparkle(760, 250, 92)], fill=GOLD)
    d.polygon([(s(x), s(y)) for x, y in sparkle(842, 382, 40)], fill=GOLD)
    return img.resize((1024, 1024), Image.LANCZOS)


def main() -> None:
    OUT.mkdir(parents=True, exist_ok=True)
    master = draw()
    master.save(OUT / "icon.png")

    hicolor = OUT / "hicolor"
    hicolor.mkdir(exist_ok=True)
    for n in (16, 32, 48, 64, 128, 256, 512):
        master.resize((n, n), Image.LANCZOS).save(hicolor / f"{n}x{n}.png")

    master.save(OUT / "kubernet-wizard.ico", sizes=[(n, n) for n in (16, 24, 32, 48, 64, 128, 256)])

    logo = ROOT / "crates" / "kw-app" / "assets" / "logo.png"
    logo.parent.mkdir(parents=True, exist_ok=True)
    master.resize((256, 256), Image.LANCZOS).save(logo, optimize=True)

    if shutil.which("iconutil"):
        with tempfile.TemporaryDirectory() as tmp:
            iconset = Path(tmp) / "kubernet-wizard.iconset"
            iconset.mkdir()
            for n in (16, 32, 128, 256, 512):
                master.resize((n, n), Image.LANCZOS).save(iconset / f"icon_{n}x{n}.png")
                master.resize((n * 2, n * 2), Image.LANCZOS).save(iconset / f"icon_{n}x{n}@2x.png")
            subprocess.run(["iconutil", "-c", "icns", str(iconset), "-o", str(OUT / "kubernet-wizard.icns")], check=True)
    else:
        print("iconutil not found: skipped .icns (run on macOS)")
    print("icons written to", OUT)


if __name__ == "__main__":
    main()
