"""Draws the app icon: an accent shield with a crosshair, on the app's dark tile.

Run `py -3 crates/ro-app/assets/make_icon.py` to regenerate icon.ico and icon-256.png beside it.
Each size is drawn on its own, supersampled then downscaled, and sizes of 24 px and under drop the
crosshair ring for a single dot: a ring two pixels across reads as noise.
"""

from pathlib import Path

from PIL import Image, ImageDraw

SIZES = [16, 20, 24, 32, 40, 48, 64, 96, 128, 256]
SUPERSAMPLE = 8
TILE = (0x17, 0x1A, 0x20, 255)
BORDER = (0x2A, 0x2F, 0x3A, 255)
ACCENT = (0x60, 0xCD, 0xFF, 255)
INK = (0x0F, 0x11, 0x14, 255)
OUT = Path(__file__).resolve().parent


def bezier(p0, p1, p2, steps=24):
    return [
        ((1 - t) ** 2 * p0[0] + 2 * (1 - t) * t * p1[0] + t**2 * p2[0],
         (1 - t) ** 2 * p0[1] + 2 * (1 - t) * t * p1[1] + t**2 * p2[1])
        for t in (i / steps for i in range(steps + 1))
    ]


def shield(left, top, right, bottom):
    """Flat top with a shallow dip, straight sides, curving to a point."""
    w, h = right - left, bottom - top
    mid = left + w / 2
    shoulder = top + h * 0.42
    points = [(left, top + h * 0.08)]
    points += bezier((left, top + h * 0.08), (mid, top - h * 0.02), (right, top + h * 0.08))[1:]
    points.append((right, shoulder))
    points += bezier((right, shoulder), (right, top + h * 0.82), (mid, bottom))[1:]
    points += bezier((mid, bottom), (left, top + h * 0.82), (left, shoulder))[1:]
    return points


def draw(size):
    s = size * SUPERSAMPLE
    image = Image.new("RGBA", (s, s), (0, 0, 0, 0))
    d = ImageDraw.Draw(image)

    inset = s * (0.02 if size <= 24 else 0.04)
    d.rounded_rectangle((inset, inset, s - inset, s - inset), radius=s * 0.22, fill=TILE,
                        outline=BORDER, width=max(SUPERSAMPLE, round(s * 0.015)))

    pad_x, pad_top, pad_bottom = s * 0.2, s * 0.16, s * 0.14
    d.polygon(shield(pad_x, pad_top, s - pad_x, s - pad_bottom), fill=ACCENT)

    cx, cy = s / 2, pad_top + (s - pad_bottom - pad_top) * 0.45
    if size <= 24:
        r = s * 0.1
        d.ellipse((cx - r, cy - r, cx + r, cy + r), fill=INK)
    else:
        ring, stroke = s * 0.16, s * 0.055
        d.ellipse((cx - ring, cy - ring, cx + ring, cy + ring), outline=INK, width=round(stroke))
        reach, gap = s * 0.26, ring * 0.45
        for dx, dy in ((1, 0), (-1, 0), (0, 1), (0, -1)):
            x0, y0 = cx + dx * gap, cy + dy * gap
            x1, y1 = cx + dx * reach, cy + dy * reach
            d.line((x0, y0, x1, y1), fill=INK, width=round(stroke))
        dot = stroke * 0.7
        d.ellipse((cx - dot, cy - dot, cx + dot, cy + dot), fill=INK)

    return image.resize((size, size), Image.LANCZOS)


def main():
    frames = [draw(size) for size in SIZES]
    frames[-1].save(OUT / "icon-256.png")
    frames[-1].save(OUT / "icon.ico", format="ICO", sizes=[(f.width, f.height) for f in frames],
                    append_images=frames[:-1])
    print(f"wrote icon.ico ({', '.join(map(str, SIZES))}) and icon-256.png")


if __name__ == "__main__":
    main()
