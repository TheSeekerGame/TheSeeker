#!/usr/bin/env python3
"""Extract the artist's background layers from an LDtk level into exemplar PNGs.

Background generation copies its openings and detailing from these exemplars. Each
exemplar is an RGBA image of one layer's IntGrid values: `VALUE_COLORS` gives each
value's color, black marks solid terrain (near exemplars only), and transparent cells
lie outside the artist's painted area.

    tools/backgrounds/.venv/bin/python tools/backgrounds/exemplars.py LevelReference/levels/level.01.ldtk
"""
from __future__ import annotations

import argparse
import json
from pathlib import Path

import numpy as np
from PIL import Image
from scipy import ndimage

ROOT = Path(__file__).resolve().parents[2]
SOURCE = Path(__file__).resolve().parent / "source" / "exemplars"
NEAR = ROOT / "assets/levels/tiles"

VALUE_COLORS = {0: (255, 255, 255), 1: (220, 90, 90), 2: (90, 150, 220), 3: (150, 150, 150), 4: (120, 190, 120)}
SOLID = (0, 0, 0)

# Offline middle/far exemplars and runtime near exemplars: (output, ice layer, tundra layer, biome).
LAYERS = [
    (SOURCE / "ice_middle.png", "MiddleBackground", "TundraMiddleBackground", "ice"),
    (SOURCE / "ice_far.png", "FarBackground", "TundraFarBackground", "ice"),
    (SOURCE / "tundra_middle.png", "MiddleBackground", "TundraMiddleBackground", "tundra"),
    (SOURCE / "tundra_far.png", "FarBackground", "TundraFarBackground", "tundra"),
    (NEAR / "ice_biome/near_bg/exemplar.png", "NearBackground", "TundraNearBackground", "ice"),
    (NEAR / "tundra_biome/near_bg/exemplar.png", "NearBackground", "TundraNearBackground", "tundra"),
]
TERRAIN = ["Collisions", "TundraMain"]


def encode(values, known, solid=None):
    rgba = np.zeros((*values.shape, 4), dtype=np.uint8)
    for value, color in VALUE_COLORS.items():
        rgba[known & (values == value)] = (*color, 255)
    if solid is not None:
        rgba[known & solid] = (*SOLID, 255)
    return rgba


def decode(path):
    """(values, known, solid): values are -1 outside the painted area."""
    rgba = np.asarray(Image.open(path).convert("RGBA"))
    known = rgba[:, :, 3] >= 128
    values = np.full(rgba.shape[:2], -1, dtype=np.int16)
    for value, color in VALUE_COLORS.items():
        values[known & np.all(rgba[:, :, :3] == color, axis=2)] = value
    solid = known & np.all(rgba[:, :, :3] == SOLID, axis=2)
    unknown = known & ~solid & (values < 0)
    if unknown.any():
        raise ValueError(f"{path}: {int(unknown.sum())} cells have no exemplar color")
    return values, known & ~unknown, solid


def level_grids(project, level_name):
    level = next(level for level in project["levels"] if level["identifier"] == level_name)
    layer_ids = {layer["uid"]: layer["identifier"] for layer in project["defs"]["layers"]}
    grids = {}
    for instance in level["layerInstances"]:
        if instance["intGridCsv"]:
            name = layer_ids.get(instance["layerDefUid"], instance["__identifier"])
            grids[name] = np.array(instance["intGridCsv"], dtype=np.int16).reshape(instance["__cHei"], instance["__cWid"])
    return grids


def painted(values, reach=0):
    """The artist's painted area, including openings enclosed within `reach` tiles."""
    area = values != 0
    if reach:
        area = ndimage.binary_closing(np.pad(area, reach), np.ones((2 * reach + 1,) * 2))[reach:-reach, reach:-reach]
    return ndimage.binary_fill_holes(area)


def crop(rgba):
    rows, cols = np.nonzero(rgba[:, :, 3])
    return rgba[rows.min():rows.max() + 1, cols.min():cols.max() + 1]


def exemplar(grids, ice, tundra, biome, interior):
    # Tundra rooms' near rings stay open on one side.
    tundra_area = painted(grids[tundra], 6 if ice.startswith("Near") else 0)
    terrain = np.any([grids[name] != 0 for name in TERRAIN if name in grids], axis=0)
    near = ice.startswith("Near")
    if biome == "tundra":
        values, known = grids[tundra], tundra_area
    else:
        values = grids[ice]
        # Ice layers stop at the tundra areas painted over them.
        known = (interior if near else painted(values)) & ~ndimage.binary_dilation(tundra_area)
    if not near:
        return encode(values, known)
    # The artist leaves the near plane unpainted under terrain; terrain hides it there anyway.
    return encode(values, known, terrain)


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("project", type=Path, help="LDtk project holding the artist's level")
    parser.add_argument("--level", default="Level_0")
    parser.add_argument("--interior", default="32,3,236,143",
                        help="x0,y0,x1,y1 tile bounds of the level's playable interior, for the near plane")
    args = parser.parse_args()
    grids = level_grids(json.loads(args.project.read_text()), args.level)
    x0, y0, x1, y1 = (int(value) for value in args.interior.split(","))
    interior = np.zeros(next(iter(grids.values())).shape, dtype=bool)
    interior[y0:y1, x0:x1] = True
    for path, ice, tundra, biome in LAYERS:
        path.parent.mkdir(parents=True, exist_ok=True)
        Image.fromarray(crop(exemplar(grids, ice, tundra, biome, interior))).save(path)
        print(f"{path.relative_to(ROOT)}")


if __name__ == "__main__":
    main()
