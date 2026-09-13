#!/usr/bin/env python3
"""The app icon, drawn: a rounded square in the app's primary colour (#3A6EA5, the
seed of the Material palette in assets/css/tokens.css) with a lighter rounded square
inset in it. Writes icon-1024.png beside this file with nothing but the standard
library; the smaller sizes are resampled from it by `sips` (see the header comment
of Dioxus.toml), and icon.ico wraps the 256 px PNG for the Windows installer.

    uv run crates/satz-studio/assets/icons/render.py
"""

import struct
import zlib
from pathlib import Path

SIZE = 1024
OUTER = (0x3A, 0x6E, 0xA5)  # primary
INNER = (0xC6, 0xDC, 0xF3)  # a light tone of the same hue
OUTER_RADIUS = 0.22 * SIZE  # corner radius of the outer square
INNER_INSET = 0.27 * SIZE  # the inner square starts this far in on every side
INNER_RADIUS = 0.11 * SIZE


def rounded_rect_coverage(x, y, x0, y0, x1, y1, r):
    """Coverage in [0, 1] of the pixel centred at (x, y) by the rounded rectangle
    (x0, y0)-(x1, y1) with corner radius r: the signed distance to its edge, mapped
    over one pixel so the edge is anti-aliased."""
    cx = min(max(x, x0 + r), x1 - r)
    cy = min(max(y, y0 + r), y1 - r)
    d = ((x - cx) ** 2 + (y - cy) ** 2) ** 0.5 - r
    return min(1.0, max(0.0, 0.5 - d))


def png(width, height, rows):
    def chunk(kind, data):
        body = kind + data
        return struct.pack(">I", len(data)) + body + struct.pack(">I", zlib.crc32(body) & 0xFFFFFFFF)

    raw = b"".join(b"\x00" + bytes(row) for row in rows)
    return (
        b"\x89PNG\r\n\x1a\n"
        + chunk(b"IHDR", struct.pack(">IIBBBBB", width, height, 8, 6, 0, 0, 0))
        + chunk(b"IDAT", zlib.compress(raw, 9))
        + chunk(b"IEND", b"")
    )


def main():
    rows = []
    for j in range(SIZE):
        row = bytearray()
        y = j + 0.5
        for i in range(SIZE):
            x = i + 0.5
            a = rounded_rect_coverage(x, y, 0, 0, SIZE, SIZE, OUTER_RADIUS)
            b = rounded_rect_coverage(
                x, y, INNER_INSET, INNER_INSET, SIZE - INNER_INSET, SIZE - INNER_INSET, INNER_RADIUS
            )
            # the inner square is painted over the outer one; alpha is the outer coverage
            rgb = tuple(round(o * (1 - b) + n * b) for o, n in zip(OUTER, INNER))
            row += bytes(rgb) + bytes([round(a * 255)])
        rows.append(row)
    out = Path(__file__).with_name("icon-1024.png")
    out.write_bytes(png(SIZE, SIZE, rows))
    print(out)


if __name__ == "__main__":
    main()
