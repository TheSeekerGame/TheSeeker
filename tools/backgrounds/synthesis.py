"""Exemplar-driven background detailing: image analogies on an opening mask.

An exemplar pairs a context (which cells are wall and which are openings, plus optional
solid terrain) with the artist's IntGrid values. Synthesis reads a target context and
copies values from the exemplar cell whose surroundings match best, preferring to continue
the copies of already-written neighbours so the artist's strokes and clusters stay whole.
Cells deep inside a wall take the wall's base value; cells deep inside an opening stay 0.

`game/src/levelgen/background_synthesis.rs` implements the same method for the near plane.
"""
from __future__ import annotations

import numpy as np
from scipy import ndimage

RADIUS = 4
INNER = 2
# Written neighbours of a cell: rows above it and cells to its left, within two cells.
# Cells with equal 2y + x never depend on each other and are written together.
CAUSAL = [(dy, dx) for dy in (-2, -1, 0) for dx in range(-2, 3) if 2 * dy + dx < 0]
CAUSAL_WEIGHT = np.array([6 if max(abs(dy), abs(dx)) == 1 else 3 for dy, dx in CAUSAL])
SAMPLES = 10
COHERENCE = 2.0
OPENING_REACH = 2


def signed_distance(wall, limit):
    """Chessboard distance to the other side: positive in walls, negative in openings."""
    inside = ndimage.distance_transform_cdt(wall, metric="chessboard") if not wall.all() else np.full(wall.shape, limit)
    outside = ndimage.distance_transform_cdt(~wall, metric="chessboard") if wall.any() else np.full(wall.shape, limit)
    return np.clip(np.where(wall, inside, -outside), -limit, limit).astype(np.int16)


def windows(mask):
    """Each cell's surrounding mask bits: (inner 5x5, outer ring to RADIUS, 3x3)."""
    height, width = mask.shape
    padded = np.pad(mask, RADIUS, constant_values=True)
    words = [np.zeros((height, width), np.uint64) for _ in range(3)]
    counts = [0, 0, 0]
    for dy in range(-RADIUS, RADIUS + 1):
        for dx in range(-RADIUS, RADIUS + 1):
            bit = padded[RADIUS + dy:RADIUS + dy + height, RADIUS + dx:RADIUS + dx + width].astype(np.uint64)
            ring = max(abs(dy), abs(dx))
            for word in ([0, 2] if ring <= 1 else [0] if ring <= INNER else [1]):
                words[word] |= bit << np.uint64(counts[word])
                counts[word] += 1
    return [word.ravel() for word in words]


class Context:
    """What synthesis matches: walls, openings, and solid terrain around each cell."""

    def __init__(self, wall, solid=None, limit=32):
        self.shape = wall.shape
        self.wall = wall
        self.solid = np.zeros(wall.shape, bool) if solid is None else solid
        self.distance = signed_distance(wall, limit).ravel()
        self.inner, self.outer, self.small = windows(wall)
        self.solid_inner, self.solid_outer, self.solid_small = windows(~self.solid)

    def keys(self, cells, distance):
        """Exact, coarse, and distance-only lookup keys."""
        d = distance.astype(np.uint64) + np.uint64(64)
        exact = self.inner[cells] | self.solid_inner[cells] << np.uint64(25) | d << np.uint64(50)
        coarse = self.small[cells] | self.solid_small[cells] << np.uint64(9) | d << np.uint64(18)
        return exact, coarse, d

    def mismatch(self, cells, other, candidates):
        """Weighted count of differing context bits between target cells and exemplar candidates."""
        count = lambda a, b: np.bitwise_count(a[candidates] ^ b[cells][:, None]).astype(np.int32)
        return (2 * count(other.inner, self.inner) + count(other.outer, self.outer)
                + 2 * count(other.solid_inner, self.solid_inner) + count(other.solid_outer, self.solid_outer))


class Exemplar:
    def __init__(self, values, known, wall, solid=None, reach=None, most=12):
        self.width = values.shape[1]
        self.values = np.where(known, values, -1).astype(np.int16).ravel()
        self.context = Context(wall, solid)
        distance = self.context.distance.reshape(values.shape)
        solid = self.context.solid
        self.base = int(np.bincount(values[known & wall & (distance > 6) & ~solid]).argmax())
        differs = known & ~solid & wall & (values != self.base)
        self.reach = int(min(most, reach or np.percentile(distance[differs], 99)))
        window_known = ndimage.binary_erosion(known, np.ones((2 * RADIUS + 1,) * 2), border_value=0)
        valid = window_known & ~solid & (distance <= self.reach) & (distance >= -OPENING_REACH)
        self.valid = valid.ravel()
        cells = np.flatnonzero(valid)
        self.pools = []
        for key in self.context.keys(cells, self.context.distance[cells]):
            order = np.argsort(key, kind="stable")
            self.pools.append((key[order], cells[order]))

    def samples(self, keys, rng):
        """SAMPLES exemplar cells per target cell from the most specific key with any match."""
        chosen = np.full((len(keys[0]), SAMPLES), -1, np.int64)
        pending = np.ones(len(keys[0]), bool)
        for key, (sorted_keys, cells) in zip(keys, self.pools):
            low = np.searchsorted(sorted_keys, key, "left")
            count = np.searchsorted(sorted_keys, key, "right") - low
            use = pending & (count > 0)
            if use.any():
                picks = low[use, None] + (rng.random((use.sum(), SAMPLES)) * count[use, None]).astype(np.int64)
                chosen[use] = cells[picks]
            pending &= ~use
        return chosen


def synthesize(target, exemplar, rng):
    """IntGrid values for `target`: 0 in openings, the exemplar's values near their edges."""
    height, width = target.shape
    ex = exemplar
    distance = np.clip(target.distance, -OPENING_REACH - 1, ex.reach + 1)
    out = np.where(target.wall.ravel(), ex.base, 0).astype(np.int16)
    band = (distance <= ex.reach) & (distance >= -OPENING_REACH) & ~target.solid.ravel()
    cells = np.flatnonzero(band)
    ys, xs = np.divmod(cells, width)
    order = np.argsort(2 * ys + xs, kind="stable")
    cells, ys, xs = cells[order], ys[order], xs[order]
    fronts = 2 * ys + xs
    keys = target.keys(cells, distance[cells])
    source = np.full(height * width, -1, np.int64)
    out[cells] = -1
    ex_distance = np.clip(ex.context.distance, -OPENING_REACH - 1, ex.reach + 1)
    edges = np.flatnonzero(np.diff(fronts)) + 1
    for start, stop in zip(np.r_[0, edges], np.r_[edges, len(cells)]):
        batch = slice(start, stop)
        cell, y, x = cells[batch], ys[batch], xs[batch]
        neighbours = []
        coherent = np.full((len(cell), len(CAUSAL)), -1, np.int64)
        for index, (dy, dx) in enumerate(CAUSAL):
            inside = (y + dy >= 0) & (x + dx >= 0) & (x + dx < width)
            neighbour = np.where(inside, cell + dy * width + dx, 0)
            neighbours.append((inside, neighbour))
            copied = np.where(inside, source[neighbour], -1)
            shifted = copied - (dy * ex.width + dx)
            ok = copied >= 0
            ok[ok] = ex.valid[shifted[ok]]
            coherent[:, index] = np.where(ok, shifted, -1)
        sampled = ex.samples(tuple(key[batch] for key in keys), rng)
        candidates = np.concatenate([coherent, sampled], axis=1)
        valid = candidates >= 0
        safe = np.where(valid, candidates, 0)
        score = target.mismatch(cell, ex.context, safe)
        score += np.abs(ex_distance[safe] - distance[cell][:, None])
        for (inside, neighbour), (dy, dx), weight in zip(neighbours, CAUSAL, CAUSAL_WEIGHT):
            written = np.where(inside, out[neighbour], ex.base)
            copied = ex.values[safe + dy * ex.width + dx]
            score += weight * ((copied != written[:, None]) & (written[:, None] >= 0))
        score = np.where(valid, score + rng.random(score.shape) * 0.5, np.inf)
        own = len(CAUSAL)
        best_sample = own + np.argmin(score[:, own:], axis=1)
        best_coherent = np.argmin(score[:, :own], axis=1)
        rows = np.arange(len(cell))
        keep = score[rows, best_coherent] <= score[rows, best_sample] + COHERENCE
        best = np.where(keep, best_coherent, best_sample)
        chosen = candidates[rows, best]
        found = chosen >= 0
        out[cell] = np.where(found, ex.values[np.maximum(chosen, 0)], np.where(target.wall.ravel()[cell], ex.base, 0))
        source[cell] = chosen
    return out.reshape(height, width)
