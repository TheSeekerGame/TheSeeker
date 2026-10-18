#!/usr/bin/env python3
"""Draw the Tether Node: its planting animation, idle loop, item, and skill icon.

The node is a Seeker stake infected on purpose: a slate metal spine wrapped by a
black parasitic vine whose roots grip the ground, topped by a claw bud that opens
around a pale core. The veins that link nodes plug into the collar at the cell's
center, where the game anchors every network connection.

Run from anywhere: python3 tools/generate_tether_art.py [--preview DIR]
Requires Pillow. Every pixel is placed explicitly; there is no randomness.
"""

from pathlib import Path
import argparse
import math

from PIL import Image


ROOT = Path(__file__).resolve().parents[1]
SHEET = ROOT / "assets/animations/objects/Tether__r1c24__48x48.png"
ITEM = ROOT / "assets/items/drops/Tether.png"
ICON = ROOT / "assets/ui/game/TetherSkillIcon.png"
CELL = 48
# The cell's bottom row rests on the ground; network veins meet at the center.
GROUND = CELL - 1
CENTER = CELL // 2

PALETTE = {
    "k": "#07070b",  # black, the color of the network veins
    "v": "#100f16",  # vine
    "u": "#262131",  # vine, lit
    "d": "#0b0c11",  # spine shadow
    "m": "#191c25",  # spine
    "l": "#2d3241",  # spine, lit edge
    "h": "#4f5870",  # spine glint
    "f": "#0e0c12",  # cage shadow
    "p": "#211a28",  # cage
    "q": "#46384f",  # cage, lit rim
    "c": "#2e5869",  # core, dim
    "C": "#5ea4bb",  # core
    "L": "#a6d6e4",  # core, bright
    "W": "#e9f8fb",  # core, white hot
    # Icon frame, shared with the other device icons.
    "F": "#3f506e",
    "B": "#000000",
    "Q": "#86708e",  # icon cage
    "S": "#68738e",  # icon spine and roots
}

# Core brightness ramps, dimmest first, for the four core rings (rim to center).
CORE_LEVELS = [
    ("c", "c", "c", "C"),
    ("c", "c", "C", "C"),
    ("c", "C", "C", "L"),
    ("C", "C", "L", "L"),
    ("C", "L", "L", "W"),
    ("L", "L", "W", "W"),
]


def rgba(code):
    return (*bytes.fromhex(PALETTE[code][1:]), 255)


class Frame:
    def __init__(self, size=(CELL, CELL)):
        self.image = Image.new("RGBA", size, (0, 0, 0, 0))

    def put(self, x, y, code):
        if code == "." or not (0 <= x < self.image.width and 0 <= y < self.image.height):
            return
        self.image.putpixel((x, y), rgba(code))

    def stamp(self, origin, rows):
        """Literal pixel rows; dots are transparent."""
        ox, oy = origin
        for y, row in enumerate(rows):
            for x, code in enumerate(row):
                self.put(ox + x, oy + y, code)


# Heads are 12 pixels wide; columns 5 and 6 continue the spine. The open cage
# leaves room for the 4x4 core in rows 3 to 6.
HEAD_LEFT = CENTER - 6

BUD_CLOSED = [
    ".....qf.....",
    ".....qf.....",
    "....qpff....",
    "....qpff....",
    "....qpff....",
    "....qpff....",
    ".....pf.....",
    ".....pf.....",
    ".....mm.....",
    "....lmmd....",
]

BUD_SLIT = [
    ".....qf.....",
    ".....qf.....",
    "....qLff....",
    "....qWff....",
    "....qLff....",
    "....qCff....",
    ".....pf.....",
    ".....pf.....",
    ".....mm.....",
    "....lmmd....",
]

BUD_PARTING = [
    ".....qf.....",
    "....q..f....",
    "....qLLf....",
    "...qpWWff...",
    "...qpWWff...",
    "....qLLf....",
    ".....pf.....",
    ".....pf.....",
    ".....mm.....",
    "....lmmd....",
]

BUD_OPEN = [
    ".....qf.....",
    "....q..f....",
    "...q....f...",
    "..qp....ff..",
    "..q......f..",
    "..q......f..",
    "..qp....ff..",
    "...qp..ff...",
    "....pmmf....",
    "....lmmd....",
]

# The open cage breathes: its ribs bow out one pixel at the core's brightest.
BUD_WIDE = [
    ".....qf.....",
    "....q..f....",
    "..qp....ff..",
    ".q........f.",
    ".q........f.",
    ".q........f.",
    "..qp....ff..",
    "...qp..ff...",
    "....pmmf....",
    "....lmmd....",
]

# The collar where the network veins plug in; rows straddle the cell's center.
COLLAR = [
    ".lmd.",
    "lhmmd",
    "lmmmd",
    ".lmd.",
]


def core(frame, top, level):
    """A round 4x4 core whose rows top..top+3 brighten toward the middle."""
    rim, outer, inner, hot = CORE_LEVELS[level]
    frame.stamp((CENTER - 2, top), [
        "." + outer + outer + ".",
        outer + inner + hot + outer,
        rim + inner + inner + rim,
        "." + rim + rim + ".",
    ])


def spine(frame, top, bottom):
    """The two-pixel metal spine from `top` to `bottom` inclusive, ringed every five rows."""
    for y in range(top, bottom + 1):
        ring = (bottom - y) % 5 == 4
        frame.put(CENTER - 1, y, "h" if ring else "m")
        frame.put(CENTER, y, "l" if ring else "d")


# The vine climbs from the roots to the collar, two turns around the spine
# (columns 2 and 3 of this stamp); it hides behind the spine where it is absent.
VINE = [
    "...v..",
    "..u...",
    ".vv...",
    ".v....",
    ".vu...",
    "..uv..",
    "...vv.",
    "....v.",
    "....v.",
    "...vv.",
    "..uv..",
    ".vv...",
    ".v....",
    ".vu...",
    "..vv..",
]


def vine(frame, bottom, rows=len(VINE)):
    """The lowest `rows` rows of the vine, ending at `bottom`."""
    shown = VINE[len(VINE) - rows:]
    frame.stamp((CENTER - 3, bottom - len(shown) + 1), shown)


# Roots creep along the ground and dip into it, the way the network's veins branch.
ROOTS = [
    "..........v..v..........",
    ".........vk..kv.........",
    "...kkkkkkk.kk.kkkkkk....",
    "kkk......k.kk.k.....kkkk",
]


def roots(frame, reach):
    """The roots, cut to `reach` pixels either side of the spine."""
    width = len(ROOTS[0])
    left = CENTER - width // 2
    for y, row in enumerate(ROOTS):
        for x, code in enumerate(row):
            if abs(left + x - CENTER + 0.5) <= reach:
                frame.put(left + x, GROUND - len(ROOTS) + 1 + y, code)


# Black thorns on the spine and barbs on the collar, as (dx, dy) from the
# spine's left column at the ground and at the cell's center.
THORNS = [(-1, -11), (-2, -12), (3, -17), (4, -18)]
BARBS = [(-2, -1), (-3, -2), (3, -1), (4, -2)]


def thorns(frame, top):
    for dx, dy in THORNS:
        if GROUND + dy >= top + 2:
            frame.put(CENTER - 1 + dx, GROUND + dy, "k")


def barbs(frame):
    for dx, dy in BARBS:
        frame.put(CENTER - 1 + dx, CENTER + dy, "k")


def head(frame, rows, top):
    """A head stamp, its tip curling into a black hook."""
    frame.stamp((HEAD_LEFT, top), rows)
    frame.put(CENTER, top - 1, "f")
    frame.put(CENTER + 1, top - 2, "k")


# The full-grown node: head rows, spine, collar and roots.
HEAD_TOP = CENTER - 12
CORE_TOP = HEAD_TOP + 3
SPINE_TOP = HEAD_TOP + 10


def grown(frame, bud, level, flare=False):
    spine(frame, SPINE_TOP, GROUND - 2)
    roots(frame, 12)
    vine(frame, GROUND - 4)
    thorns(frame, SPINE_TOP)
    frame.stamp((CENTER - 2, CENTER - 2), COLLAR)
    barbs(frame)
    head(frame, bud, HEAD_TOP)
    if bud in (BUD_OPEN, BUD_WIDE):
        core(frame, CORE_TOP, level)
    if flare:
        for dx, dy, code in [(-1, -5, "C"), (0, -5, "C"), (-1, -4, "L"), (0, -4, "L"),
                             (-8, 1, "C"), (-7, 1, "L"), (-8, 2, "C"), (-7, 2, "L"),
                             (6, 1, "L"), (7, 1, "C"), (6, 2, "L"), (7, 2, "C")]:
            frame.put(CENTER + dx, CORE_TOP + dy, code)


def growing(frame, height, reach, bud=None):
    """A spine `height` pixels tall above the root bulb, with an optional bud on top."""
    top = GROUND - 3 - height
    spine(frame, top, GROUND - 2)
    roots(frame, reach)
    vine(frame, GROUND - 4, min(len(VINE), max(0, height - 4)))
    thorns(frame, top)
    if top <= CENTER - 2:
        frame.stamp((CENTER - 2, CENTER - 2), COLLAR)
        barbs(frame)
    if bud is not None:
        head(frame, bud, top - len(bud) + 2)


def planting_frames():
    frames = []

    seed = Frame()
    seed.stamp((CENTER - 1, GROUND - 4), [
        "W.",
        "lh",
        "ld",
        "ld",
        "kk",
    ])
    frames.append(seed)

    sprout = Frame()
    sprout.stamp((CENTER - 1, GROUND - 6), ["L.", "lh", "ld", "ld"])
    roots(sprout, 3)
    frames.append(sprout)

    for height, reach, bud in [(6, 5, BUD_CLOSED), (13, 8, BUD_CLOSED), (20, 11, BUD_CLOSED)]:
        frame = Frame()
        growing(frame, height, reach, bud)
        frames.append(frame)

    for bud, level, flare in [(BUD_SLIT, 0, False), (BUD_PARTING, 5, False), (BUD_WIDE, 5, True)]:
        frame = Frame()
        grown(frame, bud, level, flare)
        frames.append(frame)
    return frames


IDLE_FRAMES = 15


def idle_level(index):
    """One slow breath per loop: dim, swelling to white, and easing back."""
    swell = 0.5 - 0.5 * math.cos(2 * math.pi * index / IDLE_FRAMES)
    return round(1 + swell * 4)


def idle_frames():
    frames = []
    for index in range(IDLE_FRAMES):
        level = idle_level(index)
        frame = Frame()
        grown(frame, BUD_WIDE if level >= 5 else BUD_OPEN, level)
        frames.append(frame)
    return frames


HOLOGRAM_EDGE = (214, 224, 230, 255)
HOLOGRAM_FILL = (112, 122, 132, 255)


def hologram(source):
    """The placement preview: a pale silhouette of `source` that the preview tint can color.

    The node itself is nearly black, and multiplying black by the valid or invalid tint
    leaves it black."""
    frame = Frame()
    pixels = source.image.load()
    for y in range(CELL):
        for x in range(CELL):
            if pixels[x, y][3] == 0:
                continue
            neighbors = [(x + dx, y + dy) for dx, dy in [(1, 0), (-1, 0), (0, 1), (0, -1)]]
            edge = any(not (0 <= nx < CELL and 0 <= ny < CELL) or pixels[nx, ny][3] == 0 for nx, ny in neighbors)
            frame.image.putpixel((x, y), HOLOGRAM_EDGE if edge else HOLOGRAM_FILL)
    return frame


def sheet(frames):
    image = Image.new("RGBA", (CELL * len(frames), CELL), (0, 0, 0, 0))
    for index, frame in enumerate(frames):
        image.alpha_composite(frame.image, (index * CELL, 0))
    return image


def item_icon():
    """The packed node: its cage shut on a seam of light, roots coiled at the foot."""
    frame = Frame((12, 12))
    frame.stamp((0, 0), [
        "......k.....",
        ".....qf.....",
        "....qpff....",
        "....qLpf....",
        "....qCpf....",
        "...k.pf.k...",
        "....klhk....",
        ".....md.....",
        ".....vu.....",
        ".....hd.....",
        "...kkvvkk...",
        "..k..kk..k..",
    ])
    return frame.image


def skill_icon():
    """The open node as a light silhouette inside the device icon frame."""
    frame = Frame((12, 12))
    frame.stamp((0, 0), [
        ".FFFFFFFFFF.",
        "FBBBBBBSBBBF",
        "FBBBBQQBBBBF",
        "FBBBQBBQBBBF",
        "FBBQBLLBQBBF",
        "FBBQLWWLQBBF",
        "FBBBQCCQBBBF",
        "FBBSBSSBSBBF",
        "FBBBBSSBBBBF",
        "FBBBBSSBBBBF",
        "FBSSSBSSBSSF",
        ".FFFFFFFFFF.",
    ])
    return frame.image


def preview(directory, frames):
    """Frames on a cave backdrop with a ground strip, enlarged for inspection."""
    scale = 6
    backdrop = Image.new("RGBA", (CELL * len(frames), CELL + 6), (41, 45, 54, 255))
    for x in range(backdrop.width):
        for y in range(CELL, CELL + 6):
            backdrop.putpixel((x, y), (150, 170, 182, 255) if y > CELL else (186, 204, 212, 255))
    backdrop.alpha_composite(sheet(frames), (0, 0))
    backdrop.resize((backdrop.width * scale, backdrop.height * scale), Image.NEAREST).save(directory / "tether_sheet.png")
    icons = Image.new("RGBA", (46, 14), (41, 45, 54, 255))
    for x in range(16, 30):
        for y in range(14):
            icons.putpixel((x, y), (18, 21, 29, 255))
    icons.alpha_composite(item_icon(), (1, 1))
    icons.alpha_composite(item_icon(), (17, 1))
    icons.alpha_composite(skill_icon(), (33, 1))
    icons.resize((icons.width * 12, icons.height * 12), Image.NEAREST).save(directory / "tether_icons.png")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--preview", type=Path, help="write enlarged previews here instead of the assets")
    args = parser.parse_args()
    idle = idle_frames()
    frames = planting_frames() + idle + [hologram(idle[0])]
    if args.preview:
        args.preview.mkdir(parents=True, exist_ok=True)
        preview(args.preview, frames)
        return
    sheet(frames).save(SHEET)
    item_icon().save(ITEM)
    skill_icon().save(ICON)


if __name__ == "__main__":
    main()
