#!/usr/bin/env python3
"""Generate the Alpha placeholder assets (doc 13, Placeholders).

Standard library only. Run from anywhere: python3 assets/gen_placeholders.py
Output goes to assets/placeholders/. Each asset is a #808080 square with a
1 px darker border and the asset ID as text. Text in PNGs is drawn with a
built-in 3x5 bitmap font and only on sizes of 64 px and up; smaller PNGs
carry the border only.
"""
import struct
import zlib
from pathlib import Path

OUT = Path(__file__).resolve().parent / "placeholders"
FILL = (0x80, 0x80, 0x80)
BORDER = (0x40, 0x40, 0x40)
TEXT = (0x20, 0x20, 0x20)

SVG_ASSETS = [
    ("AST-ICON-001", "ast-icon-001-app.placeholder.svg", 512),
    ("AST-UI-005", "ast-ui-005-soul-lane-troopers.placeholder.svg", 16),
    ("AST-UI-005", "ast-ui-005-soul-neutral-camps.placeholder.svg", 16),
    ("AST-UI-005", "ast-ui-005-soul-breakables.placeholder.svg", 16),
    ("AST-UI-005", "ast-ui-005-soul-urn.placeholder.svg", 16),
    ("AST-UI-005", "ast-ui-005-soul-sinners-sacrifice.placeholder.svg", 16),
    ("AST-UI-008", "ast-ui-008-setup-pass.placeholder.svg", 16),
    ("AST-UI-008", "ast-ui-008-setup-fail.placeholder.svg", 16),
    ("AST-UI-008", "ast-ui-008-setup-pending.placeholder.svg", 16),
]
PNG_SIZES = [16, 24, 32, 48, 64, 128, 256, 512]

GLYPHS = {
    "A": ["010", "101", "111", "101", "101"],
    "S": ["011", "100", "010", "001", "110"],
    "T": ["111", "010", "010", "010", "010"],
    "I": ["111", "010", "010", "010", "111"],
    "C": ["011", "100", "100", "100", "011"],
    "O": ["010", "101", "101", "101", "010"],
    "N": ["110", "101", "101", "101", "101"],
    "-": ["000", "000", "111", "000", "000"],
    "0": ["111", "101", "101", "101", "111"],
    "1": ["010", "110", "010", "010", "111"],
    "2": ["110", "001", "010", "100", "111"],
}


def svg(asset_id: str, size: int) -> str:
    # Text size: the ID has ~12 chars; fit it in the width.
    font = round(size * 0.12, 2) if size > 16 else 2
    return (
        f'<svg xmlns="http://www.w3.org/2000/svg" width="{size}" height="{size}" '
        f'viewBox="0 0 {size} {size}">\n'
        f'  <rect x="0.5" y="0.5" width="{size - 1}" height="{size - 1}" '
        f'fill="#808080" stroke="#404040" stroke-width="1"/>\n'
        f'  <text x="{size / 2}" y="{size / 2}" font-family="sans-serif" '
        f'font-size="{font}" text-anchor="middle" dominant-baseline="central" '
        f'fill="#202020">{asset_id}</text>\n'
        f"</svg>\n"
    )


def png(size: int, label: str) -> bytes:
    px = [[FILL] * size for _ in range(size)]
    for i in range(size):
        for p in ((i, 0), (i, size - 1), (0, i), (size - 1, i)):
            px[p[1]][p[0]] = BORDER
    if size >= 64:
        scale = max(1, (size - 8) // (len(label) * 4))
        w = len(label) * 4 * scale - scale
        x0 = (size - w) // 2
        y0 = (size - 5 * scale) // 2
        for ci, ch in enumerate(label):
            for gy, row in enumerate(GLYPHS[ch]):
                for gx, bit in enumerate(row):
                    if bit == "1":
                        for dy in range(scale):
                            for dx in range(scale):
                                px[y0 + gy * scale + dy][x0 + (ci * 4 + gx) * scale + dx] = TEXT
    raw = b"".join(b"\x00" + b"".join(bytes(c) + b"\xff" for c in row) for row in px)

    def chunk(tag: bytes, data: bytes) -> bytes:
        body = tag + data
        return struct.pack(">I", len(data)) + body + struct.pack(">I", zlib.crc32(body))

    return (
        b"\x89PNG\r\n\x1a\n"
        + chunk(b"IHDR", struct.pack(">IIBBBBB", size, size, 8, 6, 0, 0, 0))
        + chunk(b"IDAT", zlib.compress(raw, 9))
        + chunk(b"IEND", b"")
    )


def main() -> None:
    OUT.mkdir(parents=True, exist_ok=True)
    for asset_id, name, size in SVG_ASSETS:
        (OUT / name).write_text(svg(asset_id, size), encoding="utf-8")
    for size in PNG_SIZES:
        (OUT / f"ast-icon-002-app-{size}.placeholder.png").write_bytes(png(size, "AST-ICON-002"))
    print(f"wrote {len(SVG_ASSETS)} SVGs and {len(PNG_SIZES)} PNGs to {OUT}")


if __name__ == "__main__":
    main()
