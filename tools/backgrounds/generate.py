#!/usr/bin/env python3
"""Build three paired middle/far cave backgrounds from authored map footprints.

Openings and their detailing come from the artist's exemplars: openings.py arranges the
artist's openings in strata, and synthesis.py copies the artist's detailing around them.
Run from any directory with the pinned requirements; outputs are chunk textures, coverage
masks, and a manifest in assets/levels/background_bank/<map>/<variant>-<layer>.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import math
from pathlib import Path
import re

import numpy as np
from PIL import Image, ImageDraw

import exemplars
import openings
import synthesis

TOOL = Path(__file__).resolve().parent
ROOT = TOOL.parents[1]
SOURCE = TOOL / "source"
OUTPUT = ROOT / "assets/levels/background_bank"
CHUNK = 96
ART = 6
PAD = 2
# Middle and far parallax depths: `parallax_depth` in game/src/levelgen/background_surface.rs.
DEPTHS = (1 / 0.85, 1 / 0.75)
PLANES = ("middle", "far")
LDTK_LAYERS = {"ice": ("MiddleBackground", "FarBackground"),
               "tundra": ("TundraMiddleBackground", "TundraFarBackground")}
BIOME_SEEDS = {"ice": 0, "tundra": 104729}
TUNDRA_START_CHUNKS = 4
TUNDRA_END_CHUNKS = 6


def load_rgba(path):
    return np.asarray(Image.open(path).convert("RGBA"))


def map_layout(path):
    rgba = load_rgba(path)
    colors = rgba[:, :, :3]
    occupied = (rgba[:, :, 3] >= 128) & np.any(
        np.stack([np.all(colors == color, axis=2) for color in [(0, 0, 0), (255, 255, 255), (69, 255, 0)]]), axis=0)
    yy, xx = np.nonzero(occupied)
    if not len(xx):
        raise ValueError(f"{path}: no occupied map chunks")
    x0, y0, x1, y1 = xx.min(), yy.min(), xx.max() + 1, yy.max() + 1
    footprint = occupied[y0:y1, x0:x1]
    spawn = np.argwhere(np.all(colors == 255, axis=2) & (rgba[:, :, 3] >= 128))
    if len(spawn) != 1:
        raise ValueError(f"{path}: expected one white spawn marker")
    return footprint, (int(spawn[0, 1] - x0), int(spawn[0, 0] - y0))


def coverage(footprint):
    height, width = np.array(footprint.shape) * CHUNK + 2 * PAD
    allowed = np.zeros((height, width), dtype=bool)
    for y, x in np.argwhere(footprint):
        allowed[y * CHUNK:(y + 1) * CHUNK + 2 * PAD, x * CHUNK:(x + 1) * CHUNK + 2 * PAD] = True
    return allowed


class Style:
    """A biome's exemplars and the openings the layout may place in each plane."""

    def __init__(self, biome):
        self.biome = biome
        self.exemplars, self.libraries = [], []
        for plane in PLANES:
            values, known, _ = exemplars.decode(SOURCE / "exemplars" / f"{biome}_{plane}.png")
            opening = openings.opening_mask(values, known)
            self.exemplars.append(synthesis.Exemplar(values, known, ~opening))
            self.libraries.append(openings.stamps(opening))
        if biome == "tundra":
            # The artist painted few tundra openings; both planes share them.
            shared = self.libraries[0] + self.libraries[1]
            self.libraries = [shared, shared]


def logical_pair(allowed, variant, style):
    """IntGrid values of the middle and far planes of one variant."""
    rng = np.random.default_rng(731 + variant * 193 + BIOME_SEEDS[style.biome])
    planned = openings.plan(allowed, style.libraries, openings.FILL[style.biome], DEPTHS, rng)
    grids = []
    for opening, exemplar, depth in zip(planned, style.exemplars, DEPTHS):
        # The covered area's edge is a plain wall, as the artist leaves it.
        grid = synthesis.synthesize(synthesis.Context(~opening), exemplar, rng)
        grid[~openings.covered(allowed, depth)] = 0
        grids.append(grid)
    return grids


def shifted(array, dx, dy, fill=0):
    """Sample a grid at (x+dx,y+dy), with explicit world-edge values."""
    height, width = array.shape
    result = np.full_like(array, fill)
    x0, x1 = max(0, -dx), min(width, width - dx)
    y0, y1 = max(0, -dy), min(height, height - dy)
    if x0 < x1 and y0 < y1:
        result[y0:y1, x0:x1] = array[y0+dy:y1+dy, x0+dx:x1+dx]
    return result


def expected_match(field, expected):
    if expected == 1000001:
        return field != 0
    if expected == -1000001:
        return field == 0
    if -1000000 < expected < 0:
        return field != -expected
    if 0 < expected < 1000000:
        return field == expected
    return np.ones_like(field, dtype=bool)


def select_tiles(grid, layer, rng):
    tiles = np.full(grid.shape, -1, dtype=np.int32)
    flip_flags = np.zeros(grid.shape, dtype=np.uint8)
    resolved = np.zeros(grid.shape, dtype=bool)
    for priority in ["fog", "wall"]:
        stage = np.full(grid.shape, -1, dtype=np.int32)
        stopped = resolved.copy()
        for group in layer.get("autoRuleGroups", []):
            if not group.get("active", True) or priority not in group["name"].lower():
                continue
            for rule in group["rules"]:
                choices = [tile for rect in rule.get("tileRectsIds", []) for tile in rect]
                if not rule.get("active", True) or not choices:
                    continue
                chosen = stopped.copy()
                size = rule["size"]
                for fy in ([1, -1] if rule.get("flipY") else [1]):
                    for fx in ([1, -1] if rule.get("flipX") else [1]):
                        valid = (grid != 0) & ~chosen
                        for i, expected in enumerate(rule["pattern"]):
                            if expected:
                                valid &= expected_match(shifted(grid, (i % size - size // 2) * fx, (i // size - size // 2) * fy), expected)
                        valid &= rng.random(grid.shape) < rule.get("chance", 1)
                        yy, xx = np.nonzero(valid)
                        stage[yy, xx] = rng.choice(choices, size=len(xx))
                        flip_flags[yy, xx] = (fx < 0) + 2 * (fy < 0)
                        chosen |= valid
                        if rule.get("breakOnMatch", True):
                            stopped |= valid
        found = stage >= 0
        tiles[found] = stage[found]
        resolved |= found
    return tiles, flip_flags


def blur_linear(rgba, radius):
    """The runtime Gaussian kernel: separable, linear color, premultiplied alpha."""
    values = rgba.astype(np.float32) / 255
    rgb = np.where(values[:, :, :3] <= .04045, values[:, :, :3] / 12.92,
                   ((values[:, :, :3] + .055) / 1.055) ** 2.4)
    values[:, :, :3] = rgb * values[:, :, 3:4]
    support = math.ceil(radius * 1.5)
    weights = np.exp(-np.arange(-support, support + 1, dtype=np.float32)**2 / (2 * (radius * .5)**2))
    weights /= weights.sum()
    for axis in [1, 0]:
        padding = [(0, 0), (0, 0), (0, 0)]
        padding[axis] = (support, support)
        padded = np.pad(values, padding, mode="edge")
        values = sum(weight * np.take(padded, np.arange(values.shape[axis]) + i, axis=axis) for i, weight in enumerate(weights))
    alpha = values[:, :, 3:4]
    rgb = values[:, :, :3] / np.maximum(alpha, .0001)
    values[:, :, :3] = np.where(rgb <= .0031308, rgb * 12.92, 1.055 * np.maximum(rgb, 0) ** (1 / 2.4) - .055)
    return np.clip(values * 255 + .5, 0, 255).astype(np.uint8)


def raster(tiles, flags, sheet, region):
    x0, y0, x1, y1 = region
    output = Image.new("RGBA", ((x1 - x0) * ART, (y1 - y0) * ART))
    columns = sheet.width // ART
    cache = {}
    for y, x in np.argwhere(tiles[y0:y1, x0:x1] >= 0):
        tile, flip = int(tiles[y + y0, x + x0]), int(flags[y + y0, x + x0])
        if (tile, flip) not in cache:
            tx, ty = tile % columns * ART, tile // columns * ART
            art = sheet.crop((tx, ty, tx + ART, ty + ART))
            if flip & 1:
                art = art.transpose(Image.Transpose.FLIP_LEFT_RIGHT)
            if flip & 2:
                art = art.transpose(Image.Transpose.FLIP_TOP_BOTTOM)
            cache[tile, flip] = art
        output.paste(cache[tile, flip], (int(x) * ART, int(y) * ART))
    return output


def clear_spawn_chunk(image, origin, spawn):
    """The surface backdrop (tools/surface_art) shows through the spawn chunk."""
    if spawn is None:
        return
    chunk = ((spawn[0] * CHUNK + PAD) * ART - origin[0], (spawn[1] * CHUNK + PAD) * ART - origin[1])
    ImageDraw.Draw(image).rectangle((chunk[0], chunk[1], chunk[0] + CHUNK * ART - 1, chunk[1] + CHUNK * ART - 1), fill=0)


def ldtk_layer(name):
    rules = json.loads((SOURCE / "authoring.ldtk").read_text())["defs"]
    layer = next(layer for layer in rules["layers"] if layer["identifier"] == name)
    tileset = next(ts for ts in rules["tilesets"] if ts["uid"] == layer["tilesetDefUid"])
    return layer, Image.open(SOURCE / tileset["relPath"]).convert("RGBA")


def tile_layer(grid, name, rng):
    layer, sheet = ldtk_layer(name)
    return (*select_tiles(grid, layer, rng), sheet)


def depth_tundra(shape, spawn, depth):
    """Depth bands in camera/view space keep parallax artwork aligned with world biomes."""
    yy, xx = np.indices(shape)
    center = (shape[0] - 2 * PAD) / 2
    world_y = center + (yy - PAD - center) * depth
    start = (spawn[1] + TUNDRA_START_CHUNKS) * CHUNK
    end = (spawn[1] + TUNDRA_END_CHUNKS) * CHUNK
    # Coarse patches make a gradual transition while leaving surface and deep bands pure.
    roll = ((xx // 8 * 19349663) ^ (yy // 8 * 73856093)) % (end - start)
    return (world_y >= end) | ((world_y >= start) & (roll < world_y - start))


def depth_layers(allowed, spawn, variant, ice_grids, rng):
    """Both authored tilesets share one native-size atlas; no runtime biome path is needed."""
    tundra_grids = logical_pair(allowed, variant, Style("tundra"))
    layers = []
    for group, (ice, tundra) in enumerate(zip(ice_grids, tundra_grids)):
        ice_tiles, ice_flags, ice_sheet = tile_layer(ice, LDTK_LAYERS["ice"][group], rng)
        tundra_tiles, tundra_flags, tundra_sheet = tile_layer(tundra, LDTK_LAYERS["tundra"][group], rng)
        columns = max(ice_sheet.width, tundra_sheet.width) // ART
        atlas = Image.new("RGBA", (columns * ART, ice_sheet.height + tundra_sheet.height))
        atlas.paste(ice_sheet)
        atlas.paste(tundra_sheet, (0, ice_sheet.height))
        ice_columns, tundra_columns = ice_sheet.width // ART, tundra_sheet.width // ART
        ice_ids = np.where(ice_tiles < 0, -1, ice_tiles // ice_columns * columns + ice_tiles % ice_columns)
        tundra_ids = np.where(tundra_tiles < 0, -1, (tundra_tiles // tundra_columns + ice_sheet.height // ART) * columns + tundra_tiles % tundra_columns)
        mask = depth_tundra(allowed.shape, spawn, DEPTHS[group])
        layers.append((np.where(mask, tundra_ids, ice_ids), np.where(mask, tundra_flags, ice_flags), atlas))
    return layers


def write_layer(destination, tiles, flags, sheet, spawn, group, source_hash, map_hash):
    height, width = tiles.shape
    destination.mkdir(parents=True, exist_ok=True)
    manifest = dict(version=2, source_hash=source_hash, map_hash=map_hash, size=[width, height], world_size=[width * ART, height * ART], tile_px=ART, patches=[])
    radius = 3 if group == 1 else 5
    margin = math.ceil((math.ceil(radius * 1.5) + 1) / ART)
    for y0 in range(0, height, CHUNK):
        for x0 in range(0, width, CHUNK):
            x1, y1 = min(x0 + CHUNK, width), min(y0 + CHUNK, height)
            padded = (max(0, x0 - margin), max(0, y0 - margin), min(width, x1 + margin), min(height, y1 + margin))
            image = raster(tiles, flags, sheet, padded)
            origin = (padded[0] * ART, padded[1] * ART)
            clear_spawn_chunk(image, origin, spawn)
            raw = np.asarray(image)
            cx, cy = x0 * ART - origin[0], y0 * ART - origin[1]
            cover = raw[cy:cy+(y1-y0)*ART, cx:cx+(x1-x0)*ART, 3] > 0
            blurred = blur_linear(raw, radius)
            tmin = [max(0, x0 * ART - 1), max(0, y0 * ART - 1)]
            tmax = [min(width * ART, x1 * ART + 1), min(height * ART, y1 * ART + 1)]
            color = blurred[tmin[1]-origin[1]:tmax[1]-origin[1], tmin[0]-origin[0]:tmax[0]-origin[0]]
            if not np.any(color[:, :, 3]) and not np.any(cover):
                continue
            index = len(manifest["patches"])
            Image.fromarray(color).save(destination / f"{index}.png")
            cover_rgba = np.full((*cover.shape, 4), 255, dtype=np.uint8)
            cover_rgba[:, :, :3] = cover[:, :, None] * 255
            Image.fromarray(cover_rgba).save(destination / f"{index}-coverage.png")
            manifest["patches"].append(dict(min=[x0, y0], max=[x1, y1], texture_min=tmin, texture_max=tmax))
    (destination / "manifest.json").write_text(json.dumps(manifest, indent=2) + "\n")
    # Only remove obsolete numbered products owned by this generator.
    for path in destination.glob("*.png"):
        match = re.fullmatch(r"(\d+)(?:-coverage)?", path.stem)
        if match and int(match[1]) >= len(manifest["patches"]):
            path.unlink()
    print(f"{destination.relative_to(ROOT)}: {len(manifest['patches'])} textures", flush=True)


def write_preview(path, layers):
    """Unblurred far and middle planes over the sky, as the planes line up at the map centre."""
    far, middle = (raster(tiles, flags, sheet, (0, 0, tiles.shape[1], tiles.shape[0])) for tiles, flags, sheet in layers[::-1])
    sky = Image.new("RGBA", far.size, (236, 242, 246, 255))
    for image in (far, middle):
        sky.alpha_composite(image)
    path.parent.mkdir(parents=True, exist_ok=True)
    sky.save(path)
    print(f"preview {path}", flush=True)


def build(name, allowed, spawn, style, variants, source_hash, map_hash, preview=None, banks=True, depth_progression=False):
    output = OUTPUT if style.biome == "ice" else OUTPUT / style.biome
    for variant in variants:
        rng = np.random.default_rng(97 + variant)
        grids = logical_pair(allowed, variant, style)
        layers = depth_layers(allowed, spawn, variant, grids, rng) if depth_progression and spawn is not None else [
            tile_layer(grid, layer, rng) for grid, layer in zip(grids, LDTK_LAYERS[style.biome])]
        if preview:
            write_preview(preview / f"{name.replace('/', '-')}-{style.biome}-{variant}.png", layers)
        if not banks:
            continue
        for group, layer in enumerate(layers, 1):
            write_layer(output / name / f"{variant}-{group}", *layer, spawn, group, source_hash, map_hash)


def file_hash(path):
    value = 0xcbf29ce484222325
    for byte in path.read_bytes():
        value = ((value ^ byte) * 0x100000001b3) & ((1 << 64) - 1)
    return value


def source_fingerprint():
    files = sorted((SOURCE / "exemplars").glob("*.png")) + [SOURCE / "authoring.ldtk"]
    project = json.loads((SOURCE / "authoring.ldtk").read_text())
    files += sorted({(SOURCE / ts["relPath"]).resolve() for ts in project["defs"]["tilesets"] if ts.get("relPath")})
    files += sorted(path for path in TOOL.glob("*.py") if not path.name.startswith("test_"))
    return hashlib.sha256(b"".join(p.read_bytes() for p in files)).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--map", type=Path, action="append", help="Map PNG; default: all assets/levels/maps/*.png")
    parser.add_argument("--level", type=Path, action="append", help="Custom level PNG (yellow player marker); output under custom/<stem>")
    parser.add_argument("--variant", type=int, choices=range(3), action="append")
    parser.add_argument("--biome", choices=("depth", "ice", "tundra"), default="depth",
                        help="Map depth progression (default), or one biome; tundra banks use background_bank/tundra/")
    parser.add_argument("--preview", type=Path, help="Also write unblurred composites of each pair to this directory")
    parser.add_argument("--preview-only", action="store_true", help="Write previews without banks")
    args = parser.parse_args()
    if args.preview_only and not args.preview:
        parser.error("--preview-only needs --preview")
    maps = args.map or ([] if args.level else sorted((ROOT / "assets/levels/maps").glob("*.png")))
    source_hash = source_fingerprint()
    style = Style("ice" if args.biome == "depth" else args.biome)
    variants = args.variant or range(3)
    options = dict(preview=args.preview, banks=not args.preview_only)
    for path in maps:
        footprint, spawn = map_layout(path)
        build(path.stem, coverage(footprint), spawn, style, variants, source_hash, file_hash(path),
              depth_progression=args.biome == "depth", **options)
    for path in args.level or []:
        rgba = load_rgba(path)
        height, width = rgba.shape[:2]
        footprint = np.ones((math.ceil(height / CHUNK), math.ceil(width / CHUNK)), dtype=bool)
        allowed = coverage(footprint)[:height + 2 * PAD, :width + 2 * PAD]
        players = np.argwhere(np.all(rgba[:, :, :3] == (255, 255, 0), axis=2) & (rgba[:, :, 3] >= 128))
        spawn = (int(players[0, 1] // CHUNK), int(players[0, 0] // CHUNK)) if len(players) else None
        build(f"custom/{path.stem}", allowed, spawn, style, variants, source_hash, file_hash(path), **options)


if __name__ == "__main__":
    main()
