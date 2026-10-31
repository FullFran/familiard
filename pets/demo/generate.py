"""Draws the demo pet, a small round ghost, using only the standard library."""
import json
import math
import os
import struct
import zlib

CELL_W, CELL_H, COLUMNS, ROWS = 96, 104, 8, 9
HERE = os.path.dirname(os.path.abspath(__file__))
BODY, SHADE, EYE = (150, 120, 235), (112, 86, 200), (30, 22, 60)


def ghost(lift=0.0, sway=0.0, eyes=1.0, arms=0.0, mouth=0.0, tint=None):
    """Returns CELL_H rows of RGBA pixels for one pose."""
    body = tint or BODY
    cx, cy = CELL_W / 2 + sway, 46 - lift
    pixels = []
    for y in range(CELL_H):
        row = []
        for x in range(CELL_W):
            dx, dy = x - cx, y - cy
            hem = 86 - lift + 4 * math.sin(x / 5 + sway)
            inside = (dx * dx / 26**2 + dy * dy / 30**2 <= 1) or (abs(dx) <= 26 and cy <= y <= hem)
            arm = arms and abs(abs(dx) - 30) <= 4 and abs(dy + arms) <= 5
            color, alpha = (0, 0, 0), 0
            if inside or arm:
                color, alpha = (SHADE if y > hem - 8 else body), 255
            if abs(abs(dx) - 10) <= 3 and abs(dy + 4) <= max(0.6, 5 * eyes):
                color, alpha = EYE, 255
            if mouth and abs(dx) <= 4 and abs(dy - 8) <= mouth:
                color, alpha = EYE, 255
            row.append((*color, alpha))
        pixels.append(row)
    return pixels


def wave(i, n, amplitude):
    return amplitude * math.sin(2 * math.pi * i / n)


def rows():
    breathe = [ghost(lift=wave(i, 8, 2)) for i in range(8)] + [ghost(eyes=0)]
    side = [ghost(sway=wave(i, 8, 6), lift=abs(wave(i, 8, 5))) for i in range(8)]
    wave_hi = [ghost(arms=wave(i, 4, 8) + 6, mouth=2) for i in range(4)]
    jump = [ghost(lift=[0, 8, 14, 8, 0][i], arms=8, mouth=3) for i in range(5)]
    failed = [ghost(lift=-wave(i, 8, 1), eyes=0.2, tint=(120, 110, 150)) for i in range(8)]
    waiting = [ghost(sway=wave(i, 6, 3)) for i in range(6)]
    working = [ghost(lift=wave(i, 6, 1), eyes=0.5, mouth=1) for i in range(6)]
    review = [ghost(sway=wave(i, 6, 1), eyes=0.7) for i in range(6)]
    return [breathe, side, side, wave_hi, jump, failed, waiting, working, review]


def png(path, width, height, rgba_rows):
    def chunk(kind, data):
        body = kind + data
        return struct.pack(">I", len(data)) + body + struct.pack(">I", zlib.crc32(body))

    raw = b"".join(b"\0" + bytes(v for px in row for v in px) for row in rgba_rows)
    with open(path, "wb") as out:
        out.write(b"\x89PNG\r\n\x1a\n")
        out.write(chunk(b"IHDR", struct.pack(">IIBBBBB", width, height, 8, 6, 0, 0, 0)))
        out.write(chunk(b"IDAT", zlib.compress(raw, 9)))
        out.write(chunk(b"IEND", b""))


def main():
    width = CELL_W * 9
    sheet = []
    for frames in rows():
        for y in range(CELL_H):
            row = []
            for frame in frames:
                row += frame[y]
            sheet.append(row + [(0, 0, 0, 0)] * (width - len(row)))
    png(os.path.join(HERE, "spritesheet.png"), width, CELL_H * ROWS, sheet)
    manifest = {"id": "demo", "displayName": "Demo", "spritesheetPath": "spritesheet.png"}
    with open(os.path.join(HERE, "pet.json"), "w") as out:
        json.dump(manifest, out, indent=2)
        out.write("\n")


if __name__ == "__main__":
    main()
