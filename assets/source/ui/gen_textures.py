#!/usr/bin/env python3
"""Generate the main-window textures (doc 09, DEC-066). Original art: made from random noise and
simple shapes by this script, nothing is traced or copied from any game.

Needs Python 3 and Pillow. Run from anywhere:  python3 assets/source/ui/gen_textures.py
Output goes to assets/ui/. The random seed is fixed, so the output is repeatable.
Colours are the doc 09 tokens; the numbers below only add small noise around them.
"""
import math
import random
from pathlib import Path

from PIL import Image, ImageDraw, ImageFilter

OUT = Path(__file__).resolve().parents[2] / "ui"
OUT.mkdir(parents=True, exist_ok=True)
rng = random.Random(20261106)


def hexrgb(h):
    h = h.lstrip("#")
    return tuple(int(h[i : i + 2], 16) for i in (0, 2, 4))


PAPER = hexrgb("45433d")
PARCHMENT = hexrgb("d8c49c")
MINT = hexrgb("5fd69a")


def save(img, name):
    # assets-check wants 8-bit RGBA, not interlaced and square (doc 13).
    img.convert("RGBA").save(OUT / name, optimize=True)
    print(f"{name}: {(OUT / name).stat().st_size} bytes")


def clamp(v):
    return max(0, min(255, int(v)))


def tiled_fibres(img, count, length, colour_delta, alpha):
    """Short faint strokes that wrap around the edges, so the tile stays seamless."""
    w, h = img.size
    layer = Image.new("RGBA", (w, h), (0, 0, 0, 0))
    d = ImageDraw.Draw(layer)
    for _ in range(count):
        x, y = rng.uniform(0, w), rng.uniform(0, h)
        a = rng.uniform(0, math.tau)
        l = rng.uniform(length / 2, length)
        dx, dy = math.cos(a) * l, math.sin(a) * l
        shade = 255 if colour_delta > 0 else 0
        for ox in (-w, 0, w):
            for oy in (-h, 0, h):
                d.line([(x + ox, y + oy), (x + ox + dx, y + oy + dy)], fill=(shade, shade, shade, alpha))
    img.paste(Image.alpha_composite(img.convert("RGBA"), layer).convert("RGB"))


def noise_tile(size, base, amp, fibres):
    img = Image.new("RGB", (size, size))
    px = img.load()
    for y in range(size):
        for x in range(size):
            n = rng.randint(-amp, amp)
            px[x, y] = tuple(clamp(c + n) for c in base)
    tiled_fibres(img, fibres, 18, +1, 9)
    tiled_fibres(img, fibres, 18, -1, 9)
    return img


def grain():
    """Film grain: black and white specks with random strength. Drawn at low opacity."""
    size = 128
    img = Image.new("RGBA", (size, size))
    px = img.load()
    for y in range(size):
        for x in range(size):
            c = 255 if rng.random() < 0.5 else 0
            px[x, y] = (c, c, c, rng.randint(0, 22))
    return img


def halftone():
    """A 45 degree dot grid in white at low alpha, for player rows."""
    size = 12
    scale = 4
    big = Image.new("RGBA", (size * scale, size * scale), (0, 0, 0, 0))
    d = ImageDraw.Draw(big)
    step = size * scale / 2
    for gy in range(-1, 4):
        for gx in range(-1, 4):
            cx = gx * step + (step / 2 if gy % 2 else 0)
            cy = gy * step / 1
            r = scale * 0.9
            d.ellipse([cx - r, cy - r, cx + r, cy + r], fill=(255, 255, 255, 22))
    return big.resize((size, size), Image.LANCZOS)


def brush():
    """A hand-painted mint stroke with ragged ends and dry-brush streaks at the edges.
    The middle band is solid mint so text on top keeps its contrast."""
    w, h = 640, 72
    img = Image.new("RGBA", (w, h), (0, 0, 0, 0))
    px = img.load()
    # Smooth wobble for the top and bottom edges.
    def wobble(seed_phase):
        a, b, c = (rng.uniform(0.5, 1.5) for _ in range(3))
        return lambda x: (
            math.sin(x / 61.0 * a + seed_phase) * 2.2
            + math.sin(x / 23.0 * b + seed_phase * 2) * 1.4
            + math.sin(x / 9.0 * c) * 0.7
        )

    top_w, bot_w = wobble(0.3), wobble(1.9)
    # Each row (one bristle) ends a little earlier or later at both ends.
    row_start = [int(rng.uniform(4, 34) * (1 + 0.8 * abs(y - h / 2) / (h / 2))) for y in range(h)]
    row_end = [int(w - rng.uniform(4, 40) * (1 + 0.8 * abs(y - h / 2) / (h / 2))) for y in range(h)]
    for y in range(h):
        streak = rng.random()
        for x in range(w):
            top = 7 + top_w(x)
            bot = h - 7 + bot_w(x)
            if y < top - 4 or y > bot + 4 or x < row_start[y] or x > row_end[y]:
                continue
            # Edge fade: soft over 4 px, plus dry-brush gaps within 9 px of an edge.
            edge = min(y - top, bot - y)
            a = 1.0 if edge > 9 else max(0.0, (edge + 4) / 13)
            if edge <= 9 and rng.random() < 0.18 + (9 - edge) * 0.03:
                a *= 0.35
            # Bristle tone: faint lighter/darker lines along the stroke.
            tone = 1.0 + (streak - 0.5) * 0.07 + rng.uniform(-0.012, 0.012)
            col = tuple(clamp(c * tone) for c in MINT)
            px[x, y] = col + (clamp(255 * a),)
    return img.filter(ImageFilter.GaussianBlur(0.4))


def backdrop_lines():
    """Very faint concentric arcs and radial lines fanning out from the top-left corner (DEC-074).
    Cream at under 4% opacity (alpha 10 of 255), drawn 2x and scaled down for smooth lines. The
    UI draws it as a square from the corner, so the arcs stay circular."""
    size = 1024
    scale = 2
    big = Image.new("L", (size * scale, size * scale), 0)
    d = ImageDraw.Draw(big)
    # Concentric arcs, spaced a little wider each time, with a break in some of them.
    r = 140.0
    gap = 70.0
    ring = 0
    while r < size * 1.45:
        w = 2 if ring % 3 else 3
        box = [-r * scale, -r * scale, r * scale, r * scale]
        start = rng.uniform(0, 12)
        d.arc(box, start, 90 - rng.uniform(0, 14), fill=255, width=w * scale)
        r += gap
        gap *= 1.07
        ring += 1
    # Radial lines from the corner, uneven spacing, each stopping at its own length.
    ang = 4.0
    while ang < 88:
        a = math.radians(ang)
        length = rng.uniform(0.55, 1.5) * size * scale
        inner = rng.uniform(60, 220) * scale
        d.line(
            [(math.cos(a) * inner, math.sin(a) * inner), (math.cos(a) * length, math.sin(a) * length)],
            fill=150,
            width=scale,
        )
        ang += rng.uniform(5, 11)
    mask = big.resize((size, size), Image.LANCZOS)
    cream = hexrgb("efdebf")
    img = Image.new("RGBA", (size, size), cream + (0,))
    # Alpha is the mask scaled to at most 10 of 255 (3.9%).
    img.putalpha(mask.point(lambda v: v * 10 // 255))
    return img


def dashes(horizontal):
    """An 8x8 dash pattern for the dashed inner rule on parchment cards: 4 px of ink, 4 px gap,
    along x (horizontal edges) or along y (vertical edges). Square, like every texture here."""
    img = Image.new("RGBA", (8, 8), (0, 0, 0, 0))
    px = img.load()
    ink = hexrgb("2b2118")
    for y in range(8):
        for x in range(8):
            along = x if horizontal else y
            if along < 4:
                px[x, y] = ink + (170,)
    return img


def main():
    save(grain(), "grain.png")
    save(backdrop_lines(), "lines.png")
    save(dashes(True), "dash-h.png")
    save(dashes(False), "dash-v.png")
    save(noise_tile(160, PAPER, 3, 140), "paper.png")
    save(noise_tile(160, PARCHMENT, 3, 120), "parchment.png")
    save(halftone(), "halftone.png")
    # The stroke is drawn 640x72 and stored square; the UI stretches it to fit (image-fit: fill).
    save(brush().resize((512, 512), Image.LANCZOS), "brush.png")


if __name__ == "__main__":
    main()
