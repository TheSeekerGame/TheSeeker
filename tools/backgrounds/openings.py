"""Where the middle and far cave walls open.

The artist cuts openings in horizontal strata, packs them with three-tile walls between
neighbours, and lines the far openings up behind the middle ones so light reaches through.
Every opening is one of the artist's own, taken from the exemplars and flipped for variety.
Strata live in view space, where the camera moves; each plane samples them through its
parallax depth, so the planes' openings line up wherever the camera stands. Each plane
covers the map the same way: the area a camera inside the map can see through it.
"""
from __future__ import annotations

import numpy as np
from scipy import ndimage

MIN_OPENING = 10
# Wall between neighbouring openings and between an opening and the covered area's edge.
GAP = 3
MARGIN = 3
STRATA_SPACING = (26, 40)
STRATA_THICKNESS = (12, 26)
STRATA_PRESENT = 0.8
STRATA_WOBBLE = 5
# Per biome: the share of stratum cells the middle plane opens, and of the view behind middle
# openings the far plane opens. The artist opens tundra walls sparingly.
FILL = {"ice": (0.7, 0.8), "tundra": (0.3, 0.6)}
FAR_FOLLOW = 2
# Openings placed anywhere, off the strata.
ROAMING = 0.15
PLACEMENT_TRIES = 60
PLACEMENT_FAILURES = 300
# Half the widest gameplay view in tiles: about 70,000 six-pixel-tile game pixels
# (`game/src/camera/scale.rs`) reach about 34 tiles from the centre at 21:9.
VIEW_HALF = 40


def opening_mask(values, known):
    """The exemplar's openings large enough to place; smaller ones belong to its detailing."""
    opening = (values == 0) & known
    labels, _ = ndimage.label(opening)
    sizes = np.bincount(labels.ravel())
    return opening & (sizes[labels] >= MIN_OPENING)


def stamps(opening):
    labels, _ = ndimage.label(opening)
    return [labels[area] == index for index, area in enumerate(ndimage.find_objects(labels), 1)]


def value_noise(length, scale, rng):
    lattice = rng.random(int(length / scale) + 3)
    position = np.arange(length) / scale
    cell = np.floor(position).astype(int)
    t = position - cell
    t = t * t * (3 - 2 * t)
    return lattice[cell] + (lattice[cell + 1] - lattice[cell]) * t


def strata(shape, rng):
    """Wavy horizontal bands of varying thickness, broken into segments."""
    height, width = shape
    band = np.zeros(shape, bool)
    rows = np.arange(height)[:, None]
    y = rng.uniform(0, STRATA_SPACING[1])
    while y < height + STRATA_SPACING[1]:
        centre = y + (value_noise(width, 55, rng) - .5) * 2 * STRATA_WOBBLE
        low, high = STRATA_THICKNESS
        half = (low + (high - low) * value_noise(width, 35, rng)) / 2
        gate = value_noise(width, 45, rng)
        present = gate <= np.quantile(gate, STRATA_PRESENT)
        band |= (np.abs(rows - centre) <= half) & present
        y += rng.uniform(*STRATA_SPACING)
    return band


def resample(mask, scale, outside=None):
    """Cell q of the result reads cell c + scale (q - c) of `mask`, about the shared centre.

    A cell reading beyond `mask` takes its nearest edge cell, or `outside` when given.
    """
    height, width = mask.shape
    y = np.round(height / 2 + scale * (np.arange(height) + .5 - height / 2) - .5).astype(int)
    x = np.round(width / 2 + scale * (np.arange(width) + .5 - width / 2) - .5).astype(int)
    result = mask[np.ix_(np.clip(y, 0, height - 1), np.clip(x, 0, width - 1))]
    if outside is not None:
        inside = ((y >= 0) & (y < height))[:, None] & ((x >= 0) & (x < width))[None, :]
        result = np.where(inside, result, outside)
    return result


def covered(allowed, depth):
    """The cells of a plane at parallax `depth` that a camera inside `allowed` can see.

    A camera at view position v shows the plane's cell c + (v - c) / depth at the screen's
    centre, so the plane holds the map drawn `depth` times nearer the centre. The rest of the
    screen shows the plane's cells beside that one, so a point of the map up to half a view
    away needs plane cells up to that distance times (1 - 1 / depth) farther out.
    """
    margin = int(np.ceil(VIEW_HALF * (1 - 1 / depth)))
    return ndimage.maximum_filter(resample(allowed, depth, outside=False), size=2 * margin + 1)


def pack(allowed, band, library, rng, fill):
    """Openings from `library`, largest first, until `fill` of the band is open."""
    height, width = allowed.shape
    usable = ndimage.binary_erosion(allowed, np.ones((2 * MARGIN + 1,) * 2), border_value=0)
    opening = np.zeros(allowed.shape, bool)
    blocked = ~usable
    in_band = np.flatnonzero(band & usable)
    anywhere = np.flatnonzero(usable)
    if not len(in_band):
        return opening
    target = fill * len(in_band)
    sizes = np.array([stamp.sum() for stamp in library])
    queue = rng.integers(len(library), size=max(1, int(target / sizes.mean())))
    queue = list(queue[np.argsort(-sizes[queue], kind="stable")])
    spacing = np.ones((2 * GAP + 1,) * 2, bool)
    area = failures = 0
    while area < target and failures < PLACEMENT_FAILURES:
        stamp = library[queue.pop(0) if queue else rng.integers(len(library))]
        stamp = stamp[::rng.choice([-1, 1]), ::rng.choice([-1, 1])]
        roaming = rng.random() < ROAMING
        placed = place(stamp, rng.choice(anywhere if roaming else in_band, PLACEMENT_TRIES),
                       width, blocked, None if roaming else band)
        if placed is None:
            failures += 1
            continue
        y0, x0 = placed
        rows, cols = slice(y0, y0 + stamp.shape[0]), slice(x0, x0 + stamp.shape[1])
        opening[rows, cols] |= stamp
        area += stamp.sum()
        claim = ndimage.binary_dilation(np.pad(stamp, GAP), spacing)
        top, left = y0 - GAP, x0 - GAP
        cy0, cx0 = max(0, top), max(0, left)
        cy1, cx1 = min(height, top + claim.shape[0]), min(width, left + claim.shape[1])
        blocked[cy0:cy1, cx0:cx1] |= claim[cy0 - top:cy1 - top, cx0 - left:cx1 - left]
        failures = 0
    return opening


def place(stamp, centres, width, blocked, band, inside=0.55):
    """The first centre where the stamp fits, mostly within `band` when given."""
    height = blocked.shape[0]
    for centre in centres:
        y, x = divmod(int(centre), width)
        y0, x0 = y - stamp.shape[0] // 2, x - stamp.shape[1] // 2
        if y0 < 0 or x0 < 0 or y0 + stamp.shape[0] > height or x0 + stamp.shape[1] > width:
            continue
        rows, cols = slice(y0, y0 + stamp.shape[0]), slice(x0, x0 + stamp.shape[1])
        if blocked[rows, cols][stamp].any():
            continue
        if band is not None and band[rows, cols][stamp].mean() < inside:
            continue
        return y0, x0
    return None


def plan(allowed, libraries, fill, depths, rng):
    """Opening masks for a middle/far pair, each within the plane's view of `allowed`."""
    (middle_library, far_library), (middle_fill, far_fill), (middle_depth, far_depth) = libraries, fill, depths
    band = strata(allowed.shape, rng)
    middle = pack(covered(allowed, middle_depth), resample(band, middle_depth), middle_library, rng, middle_fill)
    behind = ndimage.binary_dilation(resample(middle, 1 / middle_depth), np.ones((2 * FAR_FOLLOW + 1,) * 2))
    far = pack(covered(allowed, far_depth), resample(behind, far_depth), far_library, rng, far_fill)
    return middle, far
