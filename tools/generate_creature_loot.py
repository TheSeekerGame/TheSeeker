#!/usr/bin/env python3
"""Draw the 12x12 materials that creatures leave behind.

Each material is colored from the creature that drops it and lit from the upper
left without outlines, like the plant and ore drops beside it.

Run from anywhere: python3 tools/generate_creature_loot.py [--preview DIR]
Requires Pillow. Every pixel is placed explicitly; there is no randomness.
"""

from pathlib import Path
import argparse

from PIL import Image


ROOT = Path(__file__).resolve().parents[1]
DROPS = ROOT / "assets/items/drops"

# name: (palette, rows). Dots are transparent.
LOOT = {
    # Bats: a folded membrane between finger bones.
    "BatWing": ({
        "h": "#a3b2bd",
        "b": "#76879a",
        "n": "#3c4856",
        "m": "#2c353e",
        "s": "#1c2228",
    }, [
        "............",
        "..........h.",
        "........hhb.",
        "......hbbnb.",
        "....hbnnnnb.",
        ".hhbbnnbnnb.",
        ".bnnnbnnbnm.",
        "..nnbnnmbmm.",
        "..mbnmmbmm..",
        "..b.mb..b...",
        "....b.......",
        "............",
    ]),
    # Ice spiders: a crystal grown from their pale carapace.
    "IceShard": ({
        "w": "#eef8fb",
        "a": "#b6cad5",
        "b": "#8aa0b2",
        "c": "#5e7088",
        "d": "#353e50",
    }, [
        "............",
        "......w.....",
        ".....wa.....",
        ".....wab....",
        "....wabc....",
        "....wabc.w..",
        "...wabcc.ab.",
        "...wabc.wabc",
        "...abcc.abc.",
        "..dbcdddbcd.",
        "...ddd.ddd..",
        "............",
    ]),
    # Tundra spiders: moss rooted in a chip of dark carapace.
    "MossFragment": ({
        "h": "#8ca893",
        "g": "#5f8a74",
        "t": "#4f7368",
        "o": "#49573d",
        "s": "#3a4a45",
        "d": "#232a2c",
    }, [
        "............",
        "............",
        "....h..h....",
        "...hgh.gh...",
        "..hgtghgtg..",
        "..gtgtgttg..",
        "..ttottotg..",
        "..sotsotos..",
        "...sddsdd...",
        "....dd.d....",
        "............",
        "............",
    ]),
    # Opi: a sheaf of its stalk fibers, bound in the middle.
    "PlantFiber": ({
        "l": "#9c9a68",
        "o": "#696e49",
        "m": "#49573d",
        "b": "#8f563b",
        "c": "#5e3a2b",
        "d": "#2b3226",
    }, [
        "............",
        "...l..l..l..",
        "...lo.lo.o..",
        "....lolo.o..",
        ".....oloo...",
        "....bbbbc...",
        ".....omo....",
        "....omomm...",
        "...om.m.m...",
        "...m..m..d..",
        "..d...d.....",
        "............",
    ]),
    # Squid: a bead of ink, black with the squid's violet sheen.
    "Ink": ({
        "k": "#060609",
        "i": "#14121c",
        "v": "#3d2c45",
        "p": "#644263",
        "w": "#b9a6c0",
    }, [
        "............",
        "............",
        "......k.....",
        ".....ik.....",
        "....kivk....",
        "...kipwik...",
        "...kvpvik...",
        "...kivvik...",
        "...kiiikk...",
        "....kkkk.k..",
        "..k.........",
        "............",
    ]),
    # Buzzers: the gland that brews their orbs, still glowing through its rusty skin.
    "AcidGland": ({
        "r": "#965531",
        "s": "#5e3624",
        "k": "#0f0e16",
        "g": "#375735",
        "a": "#9fb973",
        "y": "#b6c121",
        "w": "#e1ff92",
    }, [
        "............",
        "......s.....",
        ".....rs.....",
        "....rgga....",
        "...rgaywa...",
        "...rgyywg...",
        "...rgayyg...",
        "...sggaag...",
        "....sggs....",
        ".....kk.....",
        "............",
        "............",
    ]),
    # Tentacles: a severed tip, still hooked to strike, with a row of suckers.
    "TentacleTip": ({
        "h": "#c8cdcc",
        "l": "#949c9c",
        "m": "#6f7676",
        "d": "#4f5457",
        "s": "#34383a",
    }, [
        "............",
        ".....lll....",
        "....l...md..",
        "....s....md.",
        ".........lm.",
        "........hmd.",
        ".......lmmd.",
        "......hmmd..",
        ".....lmmds..",
        "...hlmmdd...",
        "..lmmmdd....",
        "..hmdds.....",
    ]),
    # The Void: a pearl of nothing, ringed by the light it swallows.
    "NullPearl": ({
        "k": "#000000",
        "r": "#2c2638",
        "p": "#6f6488",
        "q": "#b4abcb",
        "w": "#ffffff",
    }, [
        "............",
        "............",
        "....qppr....",
        "...qkkkkr...",
        "..qkkkkkkr..",
        "..pkkkkwkr..",
        "..pkkkkkkr..",
        "..rkkkkkkr..",
        "...rkkkkr...",
        "....rrrr....",
        "............",
        "............",
    ]),
}


def draw(palette, rows):
    image = Image.new("RGBA", (12, 12), (0, 0, 0, 0))
    for y, row in enumerate(rows):
        for x, code in enumerate(row):
            if code != ".":
                image.putpixel((x, y), (*bytes.fromhex(palette[code][1:]), 255))
    return image


def preview(directory, icons):
    """Each material in an inventory well, on a cave backdrop, and inside its world drop cube."""
    cube = Image.open(DROPS / "ItemCube.png").convert("RGBA")
    width = 18 * len(icons)
    image = Image.new("RGBA", (width, 54), (41, 45, 54, 255))
    for index, icon in enumerate(icons.values()):
        x = index * 18 + 2
        for wx in range(x - 1, x + 13):
            for wy in range(1, 15):
                image.putpixel((wx, wy), (18, 21, 29, 255))
        image.alpha_composite(icon, (x, 1))
        image.alpha_composite(icon, (x, 19))
        image.alpha_composite(cube, (x - 2, 36))
        image.alpha_composite(icon, (x, 36))
    image.resize((width * 8, 54 * 8), Image.NEAREST).save(directory / "creature_loot.png")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--preview", type=Path, help="write an enlarged preview here instead of the assets")
    args = parser.parse_args()
    icons = {name: draw(palette, rows) for name, (palette, rows) in LOOT.items()}
    if args.preview:
        args.preview.mkdir(parents=True, exist_ok=True)
        preview(args.preview, icons)
        return
    for name, icon in icons.items():
        icon.save(DROPS / f"{name}.png")


if __name__ == "__main__":
    main()
