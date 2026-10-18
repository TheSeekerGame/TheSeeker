#!/usr/bin/env python3
"""Draw the tiny keycaps that hint each toolbar slot's skill key.

The sheet holds one 8x8 keycap per slot, in slot order, for the keys the controls
reference lists first: J, K, L, semicolon and apostrophe. Glyphs are 5 pixels tall,
small enough for the cap to rest above a 12x12 icon. Colors follow the pixel panels'
keycaps (`game/src/ui/pixel_panel.rs`).

Run from anywhere: python3 tools/generate_key_hints.py [--preview DIR]
Requires Pillow. Every pixel is placed explicitly.
"""

from pathlib import Path
import argparse

from PIL import Image


ROOT = Path(__file__).resolve().parents[1]
SHEET = ROOT / "assets/ui/game/SkillKeyHints.png"
CAP = 8

RIM = (88, 96, 112, 255)
FILL = (18, 21, 29, 255)
LIP = (41, 48, 62, 255)
GLYPH = (240, 242, 232, 255)

# Each glyph is drawn into the cap's six interior columns, rows 1 to 5.
GLYPHS = [
    [
        "..###.",
        "....#.",
        "....#.",
        ".#..#.",
        "..##..",
    ],
    [
        ".#..#.",
        ".#.#..",
        ".##...",
        ".#.#..",
        ".#..#.",
    ],
    [
        ".#....",
        ".#....",
        ".#....",
        ".#....",
        ".####.",
    ],
    [
        "......",
        "...#..",
        "......",
        "...#..",
        "..#...",
    ],
    [
        "...#..",
        "...#..",
        "..#...",
        "......",
        "......",
    ],
]


def keycap(glyph):
    image = Image.new("RGBA", (CAP, CAP), (0, 0, 0, 0))
    for x in range(1, CAP - 1):
        image.putpixel((x, 0), RIM)
        image.putpixel((x, CAP - 1), LIP)
    for y in range(1, CAP - 1):
        image.putpixel((0, y), RIM)
        image.putpixel((CAP - 1, y), RIM)
        for x in range(1, CAP - 1):
            image.putpixel((x, y), FILL)
    for y, row in enumerate(glyph):
        for x, code in enumerate(row):
            if code == "#":
                image.putpixel((1 + x, 1 + y), GLYPH)
    return image


def sheet():
    image = Image.new("RGBA", (CAP * len(GLYPHS), CAP), (0, 0, 0, 0))
    for index, glyph in enumerate(GLYPHS):
        image.alpha_composite(keycap(glyph), (index * CAP, 0))
    return image


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--preview", type=Path, help="write an enlarged preview here instead of the asset")
    args = parser.parse_args()
    image = sheet()
    if args.preview:
        args.preview.mkdir(parents=True, exist_ok=True)
        backdrop = Image.new("RGBA", (image.width + 4, image.height + 4), (0, 0, 0, 255))
        backdrop.alpha_composite(image, (2, 2))
        backdrop.resize((backdrop.width * 12, backdrop.height * 12), Image.NEAREST).save(args.preview / "key_hints.png")
        return
    image.save(SHEET)


if __name__ == "__main__":
    main()
