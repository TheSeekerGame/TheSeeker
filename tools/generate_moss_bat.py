#!/usr/bin/env python3
"""Draw the static moss/tundra bat with explicit native-resolution pixels.

Run from anywhere: python3 tools/generate_moss_bat.py
Requires Pillow. No image generation, resampling of source art, or randomness.
"""

from pathlib import Path
import json

from PIL import Image, ImageDraw, ImageFont, ImageOps


ROOT = Path(__file__).resolve().parents[1]
SOURCE = ROOT / "tools/source_art/moss_bat"
ASSET = ROOT / "assets/animations/bats/tundra/Static__r1c1__36x36.png"
SIZE = (36, 36)

# Every opaque color is sampled from the existing tundra assets.
PALETTE = {
    "i": "#21252c",  # opi / buzzer: deepest shadow
    "d": "#2b382c",  # buzzer: warm dark green
    "s": "#405359",  # opi: cool body shadow
    "t": "#4f7368",  # opi: teal fur
    "f": "#557c70",  # tundra terrain: lit fur
    "h": "#8ca893",  # tundra terrain: sparse fur tips
    "m": "#49573d",  # opi: deep olive
    "o": "#696e49",  # opi: light olive
    "w": "#55503f",  # opi: wing membrane
    "r": "#9d8a4d",  # buzzer: warm wing ridge
    "e": "#d5c066",  # buzzer: yellow eye
    "b": "#8c9245",  # buzzer: ochre eye shadow / inner ear
}


def blank():
    return Image.new("RGBA", SIZE, (0, 0, 0, 0))


def polygon(im, color, points):
    ImageDraw.Draw(im).polygon(points, fill=PALETTE[color])


def line(im, color, points):
    ImageDraw.Draw(im).line(points, fill=PALETTE[color], width=1)


def pixels(im, color, points):
    draw = ImageDraw.Draw(im)
    for p in points:
        draw.point(p, fill=PALETTE[color])


def stamp(im, origin, rows):
    """Literal pixel clusters; dots are transparent."""
    ox, oy = origin
    for y, row in enumerate(rows):
        for x, c in enumerate(row):
            if c != ".":
                im.putpixel((ox + x, oy + y), (*bytes.fromhex(PALETTE[c][1:]), 255))


def make_layers():
    left = blank()
    # Scalloped skin stretched between three fingers, with no antialiasing.
    polygon(left, "d", [(2, 8), (8, 9), (12, 11), (15, 15), (14, 21),
                         (12, 23), (11, 20), (9, 19), (7, 21), (7, 17),
                         (5, 16), (4, 17), (5, 13)])
    polygon(left, "w", [(3, 9), (8, 10), (12, 12), (14, 15), (13, 21),
                         (11, 18), (8, 18), (8, 16), (5, 15), (6, 13)])
    polygon(left, "m", [(4, 10), (8, 11), (12, 13), (13, 15),
                         (8, 14), (6, 13)])
    polygon(left, "o", [(3, 9), (8, 10), (11, 11), (13, 13),
                         (10, 12), (7, 11), (5, 11)])
    line(left, "o", [(12, 13), (10, 14), (8, 18), (7, 20)])
    line(left, "m", [(13, 14), (12, 17), (12, 21)])
    pixels(left, "r", [(3, 9), (4, 9), (8, 10), (12, 12)])
    right = ImageOps.mirror(left)
    # Light is from above-left; keep the mirrored construction but quieter rim.
    pixels(right, "o", [(32, 9), (31, 9), (27, 10), (23, 12)])

    ears = blank()
    stamp(ears, (12, 5), [
        "..m.......m.",
        "..fm.....mf.",
        "..fom...mob.",
        "..fbo...oob.",
        "..ftom.mots.",
        "..ftbm.mbts.",
        "...ftm.mtss.",
        "...ftt.tts..",
    ])
    body = blank()
    # Pointed cheek clumps and staggered chin tufts give the round body fur.
    stamp(body, (11, 11), [
        ".....mm.......",
        "...mfom.mmm...",
        ".mfffoomoomm..",
        "..hfffomootm..",
        ".mffftfftftsm.",
        "..fteiitiets..",
        ".mfteiitiets..",
        "..ftttssttss..",
        ".mffttfhtsssm.",
        "..tftsttstss..",
        ".mfttftttsssm.",
        "..fttstttssi..",
        "...tststssi...",
        "..mtststssm...",
        "....stssi.....",
        ".....s.s......",
    ])
    # Two warm eyes are split by a dark bridge; snout stays cool and muted.
    pixels(body, "e", [(15, 16), (20, 16)])
    pixels(body, "b", [(15, 17), (20, 17)])
    pixels(body, "i", [(16, 16), (19, 16), (17, 18), (18, 18)])
    pixels(body, "h", [(17, 19), (18, 19)])
    pixels(body, "s", [(17, 20), (18, 20)])
    return {"wing_left": left, "wing_right": right, "ears": ears, "body": body}


def font(size):
    path = Path("/System/Library/Fonts/Menlo.ttc")
    return ImageFont.truetype(str(path), size) if path.exists() else ImageFont.load_default()


def enlarged(im, scale):
    return im.resize((im.width * scale, im.height * scale), Image.Resampling.NEAREST)


def place_center(canvas, im, center, scale=1):
    im = enlarged(im, scale)
    canvas.alpha_composite(im, (int(center[0] - im.width / 2), int(center[1] - im.height / 2)))


def first_frame(path, size):
    return Image.open(ROOT / path).convert("RGBA").crop((0, 0, size, size))


def preview(sprite):
    canvas = Image.new("RGBA", (1000, 720), "#141c20")
    d = ImageDraw.Draw(canvas)
    d.text((32, 23), "MOSS BAT", font=font(24), fill="#dce5d4")
    d.text((32, 60), "STATIC SPRITE / TUNDRA / PYTHON PIXEL ART", font=font(12), fill="#8ca893")
    d.rounded_rectangle((24, 99, 562, 600), radius=8, fill="#20292b")
    d.text((46, 120), "12x / nearest-neighbor", font=font(13), fill="#91a297")
    place_center(canvas, sprite, (293, 361), 12)
    d.text((46, 560), "36 x 36 canvas / transparent PNG", font=font(13), fill="#a9b6aa")

    d.text((597, 115), "NATIVE PIXELS", font=font(14), fill="#dce5d4")
    # A small native-pixel vignette, also shown at 4x for legibility.
    scene = Image.new("RGBA", (88, 52), "#21252c")
    tiles = Image.open(ROOT / "assets/levels/tiles/ldtk_rules/TundraBackgroundTileset.png").convert("RGBA")
    # A terrain-only crop; never sample enemy art from a level screenshot.
    for x in (0, 28, 56):
        scene.alpha_composite(tiles.crop((0, 0, 28, 52)), (x, 0))
    overlay = Image.new("RGBA", scene.size, (20, 28, 28, 110))
    scene = Image.alpha_composite(scene, overlay)
    scene.alpha_composite(sprite, (26, 8))
    canvas.alpha_composite(scene, (602, 150))
    d.text((710, 168), "1x", font=font(12), fill="#91a297")
    canvas.alpha_composite(enlarged(scene, 4), (597, 220))
    d.text((597, 438), "4x / tundra backdrop study", font=font(12), fill="#91a297")

    d.text((597, 483), "EXISTING ASSETS / 3x", font=font(13), fill="#dce5d4")
    refs = [
        ("ICE BAT", "assets/animations/bats/huge/Fly__r1c5__36x36.png", 36, (634, 550)),
        ("BUZZER", "assets/animations/buzzer/AirIdle__r1c6__48x48.png", 48, (753, 550)),
        ("OPI", "assets/animations/opi/Idle__r1c1__84x84.png", 84, (891, 550)),
    ]
    for label, path, size, center in refs:
        frame = first_frame(path, size)
        frame = frame.crop(frame.getbbox())
        place_center(canvas, frame, center, 3)
        d.text((center[0] - 32, 611), label, font=font(11), fill="#91a297")
    for i, color in enumerate(PALETTE.values()):
        x = 33 + i * 38
        d.rectangle((x, 631, x + 28, 651), fill=color)
    d.text((32, 672), "Palette sampled from Opi, Buzzer and tundra tiles. Wings and body saved separately.",
           font=font(12), fill="#8ca893")
    canvas.convert("RGB").save(SOURCE / "MossBatPreview.png")
    # A clean zoom for chat without the layout shrinking the artwork.
    zoom = Image.new("RGBA", (432, 360), "#20292b")
    zoom.alpha_composite(enlarged(sprite.crop((0, 3, 36, 33)), 12))
    zoom.convert("RGB").save(SOURCE / "MossBatZoom.png")


def main():
    SOURCE.mkdir(parents=True, exist_ok=True)
    ASSET.parent.mkdir(parents=True, exist_ok=True)
    layers = make_layers()
    sprite = blank()
    for name, layer in layers.items():
        sprite = Image.alpha_composite(sprite, layer)
        layer.save(SOURCE / f"{name}.png")
    assert set(sprite.getchannel("A").getdata()) <= {0, 255}
    sprite.save(ASSET)
    metadata = {
        "canvas": list(SIZE), "visible_bounds_xyxy": list(sprite.getbbox()),
        "body_center_xy": [18, 19], "wing_hinges_xy": [[14, 16], [21, 16]],
        "palette": PALETTE, "layers_back_to_front": list(layers),
        "asset": str(ASSET.relative_to(ROOT)),
        "status": "Static design study; no gameplay or animation registration.",
    }
    (SOURCE / "sprite.json").write_text(json.dumps(metadata, indent=2) + "\n")
    preview(sprite)
    print(f"Sprite: {ASSET}\nPreview: {SOURCE / 'MossBatPreview.png'}")
    print(f"Bounds: {sprite.getbbox()}; opaque colors: {len({p for p in sprite.getdata() if p[3]})}")


if __name__ == "__main__":
    main()
