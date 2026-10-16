#!/usr/bin/env python3
"""Generate and validate deterministic 96x96 level-plan chunks.

The generator works on a 48x48 macro grid and expands every macro cell to a
2x2 pixel block. This makes one-pixel floor and wall features impossible while
keeping the output compatible with the existing level compiler.

Examples:

    python3 tools/generate_level_chunks.py generate \
        --output assets/levels/chunks \
        --prefix generated_test \
        --seed 20260825 \
        --connectors LR UD UDLR LD UR R \
        --preview /tmp/generated_level_chunks.png

    python3 tools/generate_level_chunks.py validate \
        assets/levels/chunks/generated_test_*.png
"""

from __future__ import annotations

import argparse
from collections import deque
from dataclasses import dataclass
import json
import math
from pathlib import Path
import random
import re
import sys
from typing import Iterable, Sequence

from PIL import Image, ImageDraw, ImageFont, PngImagePlugin


IMAGE_SIZE = 96
MACRO_SCALE = 2
MACRO_SIZE = IMAGE_SIZE // MACRO_SCALE
CONNECTOR_ORDER = "UDLR"
PORT_START = 21
PORT_END = 27
FINAL_PORT_START = PORT_START * MACRO_SCALE
FINAL_PORT_END = PORT_END * MACRO_SCALE
CORE = 1
SPECIAL = 2
SPIKE_COLOR = (255, 255, 255, 255)
MIN_SPIKE_GROUP_SIZE = 4
MIN_GENERATED_SPIKE_GROUP_SIZE = 10
MAX_GENERATED_SPIKE_GROUP_SIZE = 56
MIN_SPIKE_GROUP_GAP = 4
MIN_SPIKE_GROUPS = 5
MAX_SPIKE_GROUPS = 8
MIN_CURVED_SPIKE_GROUPS = 3
SPIKE_SURFACE_BAND_DEPTH = 2
SPIKE_EDGE_MARGIN = 4
CARDINAL_DIRECTIONS = ((-1, 0), (1, 0), (0, -1), (0, 1))
DIAGONAL_DIRECTIONS = ((-1, -1), (1, -1), (-1, 1), (1, 1))


@dataclass(frozen=True)
class MovementProfile:
    """Conservative tile-space limits for the player's base movement."""

    body_height: int = 2
    max_rise: int = 10
    max_drop: int = 24
    max_horizontal: int = 14
    max_apex_lift: int = 12


@dataclass(frozen=True)
class Platform:
    y: int
    x0: int
    x1: int
    special: bool


@dataclass
class ChunkGeometry:
    solid: list[bytearray]
    spikes: list[bytearray]
    labels: list[bytearray]
    connectors: str
    seed: int
    special_regions: list[tuple[int, int, int, int]]


@dataclass(frozen=True)
class ValidationReport:
    errors: tuple[str, ...]
    platform_count: int
    air_fraction: float

    @property
    def valid(self) -> bool:
        return not self.errors


class MacroCanvas:
    def __init__(self) -> None:
        self.solid = [bytearray([1]) * MACRO_SIZE for _ in range(MACRO_SIZE)]
        self.labels = [bytearray(MACRO_SIZE) for _ in range(MACRO_SIZE)]

    def carve_rect(
        self,
        x0: int,
        y0: int,
        x1: int,
        y1: int,
        label: int = CORE,
    ) -> None:
        x0 = max(0, min(MACRO_SIZE, x0))
        y0 = max(0, min(MACRO_SIZE, y0))
        x1 = max(0, min(MACRO_SIZE, x1))
        y1 = max(0, min(MACRO_SIZE, y1))
        for y in range(y0, y1):
            for x in range(x0, x1):
                self.solid[y][x] = 0
                self.labels[y][x] = label

    def carve_ellipse(
        self,
        cx: float,
        cy: float,
        rx: float,
        ry: float,
        label: int = CORE,
    ) -> None:
        x0 = max(0, math.floor(cx - rx))
        y0 = max(0, math.floor(cy - ry))
        x1 = min(MACRO_SIZE, math.ceil(cx + rx) + 1)
        y1 = min(MACRO_SIZE, math.ceil(cy + ry) + 1)
        for y in range(y0, y1):
            for x in range(x0, x1):
                dx = (x + 0.5 - cx) / rx
                dy = (y + 0.5 - cy) / ry
                if dx * dx + dy * dy <= 1.0:
                    self.solid[y][x] = 0
                    self.labels[y][x] = label

    def fill_rect(self, x0: int, y0: int, x1: int, y1: int) -> None:
        for y in range(max(0, y0), min(MACRO_SIZE, y1)):
            for x in range(max(0, x0), min(MACRO_SIZE, x1)):
                self.solid[y][x] = 1
                self.labels[y][x] = 0

    def label_rect(
        self,
        x0: int,
        y0: int,
        x1: int,
        y1: int,
        label: int,
    ) -> None:
        for y in range(max(0, y0), min(MACRO_SIZE, y1)):
            for x in range(max(0, x0), min(MACRO_SIZE, x1)):
                if not self.solid[y][x]:
                    self.labels[y][x] = label

    def carve_tunnel(
        self,
        start: tuple[int, int],
        end: tuple[int, int],
        width: int = 6,
    ) -> list[tuple[int, int]]:
        points = raster_line(start, end)
        low = width // 2
        high = width - low
        for x, y in points:
            self.carve_rect(x - low, y - low, x + high, y + high)
        return points

    def expand_tight_vertical_spaces(self) -> None:
        """Open bounded one-cell air pockets to two macro cells of clearance."""

        while True:
            cells_to_carve: dict[tuple[int, int], int] = {}
            for x in range(MACRO_SIZE):
                y = 0
                while y < MACRO_SIZE:
                    if self.solid[y][x]:
                        y += 1
                        continue
                    start = y
                    while y < MACRO_SIZE and not self.solid[y][x]:
                        y += 1
                    if start > 0 and y < MACRO_SIZE and y - start == 1:
                        target_y = start - 1 if start > 1 else y
                        label = self.labels[start][x] or CORE
                        cells_to_carve[(x, target_y)] = label
            if not cells_to_carve:
                return
            for (x, y), label in cells_to_carve.items():
                self.solid[y][x] = 0
                self.labels[y][x] = label

    def separate_fragile_solid_connections(self) -> None:
        """Detach heavy rock masses joined through a one-cell solid neck."""

        for _ in range(64):
            connection = find_fragile_solid_connection(self.solid)
            if connection is None:
                return
            x, y = connection
            self.solid[y][x] = 0
            self.labels[y][x] = CORE
            self.expand_tight_vertical_spaces()
        raise RuntimeError("fragile-connection cleanup did not converge")


def raster_line(
    start: tuple[int, int],
    end: tuple[int, int],
) -> list[tuple[int, int]]:
    x0, y0 = start
    x1, y1 = end
    dx = abs(x1 - x0)
    dy = -abs(y1 - y0)
    step_x = 1 if x0 < x1 else -1
    step_y = 1 if y0 < y1 else -1
    error = dx + dy
    points = []
    while True:
        points.append((x0, y0))
        if x0 == x1 and y0 == y1:
            return points
        doubled = error * 2
        if doubled >= dy:
            error += dy
            x0 += step_x
        if doubled <= dx:
            error += dx
            y0 += step_y


def solid_neighbors(
    solid: Sequence[Sequence[int]],
    x: int,
    y: int,
) -> list[tuple[int, int]]:
    height = len(solid)
    width = len(solid[0]) if height else 0
    return [
        (nx, ny)
        for nx, ny in ((x - 1, y), (x + 1, y), (x, y - 1), (x, y + 1))
        if 0 <= nx < width and 0 <= ny < height and solid[ny][nx]
    ]


def find_fragile_solid_connection(
    solid: Sequence[Sequence[int]],
    minimum_mass: int = 12,
) -> tuple[int, int] | None:
    """Find an interior one-cell neck supporting two substantial masses."""

    if sys.getrecursionlimit() < 10_000:
        sys.setrecursionlimit(10_000)
    height = len(solid)
    width = len(solid[0]) if height else 0
    remaining = {
        (x, y)
        for y in range(height)
        for x in range(width)
        if solid[y][x]
    }
    while remaining:
        start = min(remaining, key=lambda point: (point[1], point[0]))
        component = {start}
        queue = deque([start])
        remaining.remove(start)
        while queue:
            point = queue.popleft()
            for neighbor in solid_neighbors(solid, *point):
                if neighbor not in remaining:
                    continue
                remaining.remove(neighbor)
                component.add(neighbor)
                queue.append(neighbor)
        if len(component) < minimum_mass * 2 + 1:
            continue

        discovery: dict[tuple[int, int], int] = {}
        low: dict[tuple[int, int], int] = {}
        subtree_size: dict[tuple[int, int], int] = {}
        parent: dict[tuple[int, int], tuple[int, int]] = {}
        separated: dict[tuple[int, int], list[int]] = {}
        clock = 0

        def visit(point: tuple[int, int]) -> None:
            nonlocal clock
            clock += 1
            discovery[point] = clock
            low[point] = clock
            subtree_size[point] = 1
            for neighbor in solid_neighbors(solid, *point):
                if neighbor not in component:
                    continue
                if neighbor not in discovery:
                    parent[neighbor] = point
                    visit(neighbor)
                    subtree_size[point] += subtree_size[neighbor]
                    low[point] = min(low[point], low[neighbor])
                    if low[neighbor] >= discovery[point]:
                        separated.setdefault(point, []).append(
                            subtree_size[neighbor]
                        )
                elif parent.get(point) != neighbor:
                    low[point] = min(low[point], discovery[neighbor])

        visit(start)
        for x, y in sorted(component, key=lambda point: (point[1], point[0])):
            if not (2 <= x < width - 2 and 2 <= y < height - 2):
                continue
            if not 2 <= len(solid_neighbors(solid, x, y)) <= 3:
                continue
            partitions = list(separated.get((x, y), ()))
            remainder = len(component) - 1 - sum(partitions)
            if remainder > 0:
                partitions.append(remainder)
            if sum(size >= minimum_mass for size in partitions) >= 2:
                return x, y
    return None


def canonical_connectors(value: str) -> str:
    letters = value.upper()
    if not letters or any(letter not in CONNECTOR_ORDER for letter in letters):
        raise ValueError(f"invalid connectors {value!r}; use U, D, L, and R")
    if len(set(letters)) != len(letters):
        raise ValueError(f"connector directions must not repeat: {value!r}")
    return "".join(letter for letter in CONNECTOR_ORDER if letter in letters)


def connectors_from_filename(path: Path) -> str:
    match = re.search(r"([UDLR]+)$", path.stem, flags=re.IGNORECASE)
    if match is None:
        raise ValueError(f"filename has no trailing connector suffix: {path.name}")
    return canonical_connectors(match.group(1))


def mixed_seed(seed: int, attempt: int) -> int:
    value = (seed + attempt * 0x9E3779B97F4A7C15) & 0xFFFFFFFFFFFFFFFF
    value ^= value >> 30
    value = (value * 0xBF58476D1CE4E5B9) & 0xFFFFFFFFFFFFFFFF
    value ^= value >> 27
    value = (value * 0x94D049BB133111EB) & 0xFFFFFFFFFFFFFFFF
    value ^= value >> 31
    return value


def add_rock_platform(
    canvas: MacroCanvas,
    x0: int,
    y0: int,
    width: int,
    depth: int,
    attachment: str,
    variant: int,
) -> None:
    """Build a flat-topped rock ledge with an asymmetric lower silhouette."""

    for row in range(depth):
        if row < 2:
            trim_left = 0
            trim_right = 0
        else:
            taper = row - 1
            if attachment == "left":
                trim_left = 0
                trim_right = taper + ((variant + row) & 1)
            elif attachment == "right":
                trim_left = taper + ((variant + row) & 1)
                trim_right = 0
            else:
                bias = variant & 1
                trim_left = (taper + bias) // 2
                trim_right = taper - trim_left
                if row == depth - 1 and width - trim_left - trim_right > 4:
                    if (variant >> 1) & 1:
                        trim_left += 1
                    else:
                        trim_right += 1

        maximum_trim = max(0, width - 3)
        trim_left = min(trim_left, maximum_trim)
        trim_right = min(trim_right, maximum_trim - trim_left)
        canvas.fill_rect(
            x0 + trim_left,
            y0 + row,
            x0 + width - trim_right,
            y0 + row + 1,
        )


def add_vertical_ledges(
    canvas: MacroCanvas,
    x0: int,
    x1: int,
    y0: int,
    y1: int,
    phase: int,
) -> None:
    width = x1 - x0
    ledge_width = max(3, width - 2)
    for index, y in enumerate(range(y0, y1, 5)):
        if (index + phase) % 2 == 0:
            add_rock_platform(
                canvas,
                x0,
                y,
                ledge_width,
                3,
                "left",
                phase + index,
            )
        else:
            add_rock_platform(
                canvas,
                x1 - ledge_width,
                y,
                ledge_width,
                3,
                "right",
                phase + index,
            )


def add_hub_traversal_ledges(
    canvas: MacroCanvas,
    hub: tuple[int, int, int, int],
    phase: int,
) -> None:
    """Continue an upper connector's conservative route through a tall hub."""

    x0, y0, x1, y1 = hub
    center = (x0 + x1) // 2
    width = 7
    for index, y in enumerate(range(y0 + 2, y1 - 2, 5)):
        if (index + phase) % 2 == 0:
            ledge_x0 = center - width
        else:
            ledge_x0 = center
        add_rock_platform(
            canvas,
            ledge_x0,
            y,
            width,
            3,
            "floating",
            phase + index,
        )


def add_route_chambers(
    canvas: MacroCanvas,
    rng: random.Random,
    route_points: Sequence[tuple[int, int]],
    count: int,
) -> None:
    usable = [
        point
        for point in route_points
        if 7 <= point[0] < MACRO_SIZE - 7
        and 7 <= point[1] < MACRO_SIZE - 7
    ]
    if not usable:
        return
    rng.shuffle(usable)
    for cx, cy in usable[:count]:
        canvas.carve_ellipse(
            cx + rng.uniform(-1.0, 1.0),
            cy + rng.uniform(-1.0, 1.0),
            rng.uniform(4.5, 7.0),
            rng.uniform(3.5, 5.5),
        )


def add_dead_end_lobes(
    canvas: MacroCanvas,
    rng: random.Random,
    hub: tuple[int, int, int, int],
    count: int,
) -> None:
    """Add a few broad, base-accessible rooms without cellular noise."""

    hub_x0, hub_y0, hub_x1, hub_y1 = hub
    corners = [
        (rng.randint(8, 11), rng.randint(8, 11)),
        (rng.randint(36, 39), rng.randint(8, 11)),
        (rng.randint(8, 11), rng.randint(36, 39)),
        (rng.randint(36, 39), rng.randint(36, 39)),
    ]
    rng.shuffle(corners)
    for target_x, target_y in corners[:count]:
        start_x = max(hub_x0 + 3, min(hub_x1 - 4, target_x))
        start_y = max(hub_y0 + 3, min(hub_y1 - 4, target_y))
        canvas.carve_tunnel((start_x, start_y), (target_x, target_y), width=6)
        canvas.carve_ellipse(
            target_x,
            target_y,
            rng.uniform(4.5, 6.0),
            rng.uniform(4.0, 5.5),
        )


def add_special_platform(
    canvas: MacroCanvas,
    hub: tuple[int, int, int, int],
) -> tuple[int, int, int, int] | None:
    x0, y0, x1, y1 = hub
    if x1 - x0 < 23 or y1 - y0 < 18:
        return None

    width = 5
    platform_x0 = (x0 + x1 - width) // 2
    platform_y = y0 + 5
    add_rock_platform(
        canvas,
        platform_x0,
        platform_y,
        width,
        5,
        "floating",
        platform_x0 + platform_y,
    )
    canvas.label_rect(
        platform_x0 - 2,
        platform_y - 4,
        platform_x0 + width + 2,
        platform_y,
        SPECIAL,
    )
    return (
        (platform_x0 - 2) * MACRO_SCALE,
        (platform_y - 4) * MACRO_SCALE,
        (platform_x0 + width + 2) * MACRO_SCALE,
        platform_y * MACRO_SCALE,
    )


def build_candidate(
    connectors: str,
    seed: int,
    special_branch: bool,
) -> ChunkGeometry:
    rng = random.Random(seed)
    canvas = MacroCanvas()

    hub_width = rng.randint(23, 28)
    hub_height = rng.randint(19, 24)
    center_x = 24 + rng.randint(-2, 2)
    center_y = 24 + rng.randint(-2, 2)
    hub_x0 = max(9, center_x - hub_width // 2)
    hub_y0 = max(9, center_y - hub_height // 2)
    hub_x1 = min(MACRO_SIZE - 9, hub_x0 + hub_width)
    hub_y1 = min(MACRO_SIZE - 9, hub_y0 + hub_height)
    hub_x0 = hub_x1 - hub_width
    hub_y0 = hub_y1 - hub_height
    hub = (hub_x0, hub_y0, hub_x1, hub_y1)

    canvas.carve_ellipse(
        (hub_x0 + hub_x1) / 2,
        (hub_y0 + hub_y1) / 2,
        (hub_x1 - hub_x0) / 2,
        (hub_y1 - hub_y0) / 2,
    )
    canvas.carve_rect(
        hub_x0 + 3,
        hub_y0 + 1,
        hub_x1 - 3,
        hub_y1 - 1,
    )

    route_points: list[tuple[int, int]] = []
    vertical_shafts: list[tuple[int, int, int, int, int]] = []
    port_center = (PORT_START + PORT_END - 1) // 2
    hub_center_x = (hub_x0 + hub_x1) // 2
    side_target_y = hub_y1 - 4

    if "L" in connectors:
        canvas.carve_rect(0, PORT_START, 7, PORT_END)
        route_points.extend(
            canvas.carve_tunnel((7, port_center), (hub_x0 + 2, side_target_y))
        )
    if "R" in connectors:
        canvas.carve_rect(MACRO_SIZE - 7, PORT_START, MACRO_SIZE, PORT_END)
        route_points.extend(
            canvas.carve_tunnel(
                (MACRO_SIZE - 8, port_center),
                (hub_x1 - 3, side_target_y),
            )
        )
    if "U" in connectors:
        canvas.carve_rect(PORT_START, 0, PORT_END, 7)
        canvas.carve_rect(
            hub_center_x - 3,
            6,
            hub_center_x + 3,
            hub_y0 + 3,
        )
        route_points.extend(
            canvas.carve_tunnel((port_center, 7), (hub_center_x, hub_y0 + 2))
        )
        shaft_x0 = min(port_center, hub_center_x) - 3
        shaft_x1 = max(port_center, hub_center_x) + 3
        vertical_shafts.append((shaft_x0, shaft_x1, 7, hub_y0, seed & 1))
    if "D" in connectors:
        canvas.carve_rect(PORT_START, MACRO_SIZE - 7, PORT_END, MACRO_SIZE)
        canvas.carve_rect(
            hub_center_x - 3,
            hub_y1 - 3,
            hub_center_x + 3,
            MACRO_SIZE - 6,
        )
        route_points.extend(
            canvas.carve_tunnel(
                (hub_center_x, hub_y1 - 3),
                (port_center, MACRO_SIZE - 8),
            )
        )
        shaft_x0 = min(port_center, hub_center_x) - 3
        shaft_x1 = max(port_center, hub_center_x) + 3
        vertical_shafts.append(
            (
                shaft_x0,
                shaft_x1,
                hub_y1,
                MACRO_SIZE - 7,
                (seed >> 1) & 1,
            )
        )

    add_route_chambers(canvas, rng, route_points, rng.randint(1, 3))
    lobe_roll = rng.random()
    lobe_count = 0 if lobe_roll < 0.20 else 1 if lobe_roll < 0.78 else 2
    add_dead_end_lobes(canvas, rng, hub, lobe_count)
    canvas.carve_ellipse(
        hub_center_x + rng.uniform(-3.0, 3.0),
        hub_y0 + rng.uniform(1.5, 4.0),
        rng.uniform(5.0, 8.0),
        rng.uniform(3.0, 5.0),
    )

    # Add ledges after all broad carving so decorative chambers cannot erase
    # the guaranteed base-movement route through a vertical connector.
    for shaft in vertical_shafts:
        add_vertical_ledges(canvas, *shaft)
    if "U" in connectors:
        add_hub_traversal_ledges(canvas, hub, (seed >> 2) & 1)

    special_regions = []
    if special_branch and "U" not in connectors:
        region = add_special_platform(canvas, hub)
        if region is not None:
            special_regions.append(region)

    canvas.expand_tight_vertical_spaces()
    canvas.separate_fragile_solid_connections()
    canvas.expand_tight_vertical_spaces()
    solid, labels = upscale_canvas(canvas)
    spikes = [bytearray(IMAGE_SIZE) for _ in range(IMAGE_SIZE)]
    return ChunkGeometry(
        solid,
        spikes,
        labels,
        connectors,
        seed,
        special_regions,
    )


def upscale_canvas(canvas: MacroCanvas) -> tuple[list[bytearray], list[bytearray]]:
    solid = [bytearray(IMAGE_SIZE) for _ in range(IMAGE_SIZE)]
    labels = [bytearray(IMAGE_SIZE) for _ in range(IMAGE_SIZE)]
    for macro_y in range(MACRO_SIZE):
        for macro_x in range(MACRO_SIZE):
            for offset_y in range(MACRO_SCALE):
                y = macro_y * MACRO_SCALE + offset_y
                start = macro_x * MACRO_SCALE
                end = start + MACRO_SCALE
                solid[y][start:end] = bytes(
                    [canvas.solid[macro_y][macro_x]] * MACRO_SCALE
                )
                labels[y][start:end] = bytes(
                    [canvas.labels[macro_y][macro_x]] * MACRO_SCALE
                )
    return solid, labels


def geometry_image(geometry: ChunkGeometry) -> Image.Image:
    image = Image.new("RGBA", (IMAGE_SIZE, IMAGE_SIZE), (0, 0, 0, 0))
    image.putdata(
        [
            SPIKE_COLOR
            if geometry.spikes[y][x]
            else (0, 0, 0, 255)
            if geometry.solid[y][x]
            else (0, 0, 0, 0)
            for y in range(IMAGE_SIZE)
            for x in range(IMAGE_SIZE)
        ]
    )
    return image


def expected_edge_open(direction: str, position: int) -> bool:
    return FINAL_PORT_START <= position < FINAL_PORT_END


def verify_edges(solid: Sequence[Sequence[int]], connectors: str) -> list[str]:
    errors = []
    for direction in CONNECTOR_ORDER:
        values = []
        for position in range(IMAGE_SIZE):
            if direction == "U":
                values.append(bool(solid[0][position]))
            elif direction == "D":
                values.append(bool(solid[IMAGE_SIZE - 1][position]))
            elif direction == "L":
                values.append(bool(solid[position][0]))
            else:
                values.append(bool(solid[position][IMAGE_SIZE - 1]))

        for position, is_solid in enumerate(values):
            should_be_open = direction in connectors and expected_edge_open(
                direction, position
            )
            if is_solid == should_be_open:
                errors.append(
                    f"{direction} edge pixel {position} is "
                    f"{'solid' if is_solid else 'open'}"
                )
                break
    return errors


def verify_macro_blocks(solid: Sequence[Sequence[int]]) -> list[str]:
    for y in range(0, IMAGE_SIZE, MACRO_SCALE):
        for x in range(0, IMAGE_SIZE, MACRO_SCALE):
            value = solid[y][x]
            if any(
                solid[y + dy][x + dx] != value
                for dy in range(MACRO_SCALE)
                for dx in range(MACRO_SCALE)
            ):
                return [f"geometry is not 2x2 aligned at ({x}, {y})"]
    return []


def verify_vertical_clearance(
    solid: Sequence[Sequence[int]],
    minimum_clearance: int = 3,
) -> list[str]:
    """Reject floor-to-ceiling gaps too short for viable traversal."""

    for x in range(IMAGE_SIZE):
        y = 0
        while y < IMAGE_SIZE:
            if solid[y][x]:
                y += 1
                continue
            start = y
            while y < IMAGE_SIZE and not solid[y][x]:
                y += 1
            if (
                start > 0
                and y < IMAGE_SIZE
                and y - start < minimum_clearance
            ):
                return [
                    f"vertical clearance at x={x}, y={start}..{y - 1} is "
                    f"{y - start} tile(s); minimum is {minimum_clearance}"
                ]
    return []


def verify_horizontal_clearance(
    solid: Sequence[Sequence[int]],
    minimum_clearance: int = 2,
) -> list[str]:
    """Reject vertical shafts narrower than the player's tile footprint."""

    for y in range(IMAGE_SIZE):
        x = 0
        while x < IMAGE_SIZE:
            if solid[y][x]:
                x += 1
                continue
            start = x
            while x < IMAGE_SIZE and not solid[y][x]:
                x += 1
            if (
                start > 0
                and x < IMAGE_SIZE
                and x - start < minimum_clearance
            ):
                return [
                    f"horizontal clearance at y={y}, x={start}..{x - 1} is "
                    f"{x - start} tile(s); minimum is {minimum_clearance}"
                ]
    return []


def verify_solid_connections(
    solid: Sequence[Sequence[int]],
) -> list[str]:
    macro_solid = [
        bytearray(
            1 if solid[y * MACRO_SCALE][x * MACRO_SCALE] else 0
            for x in range(MACRO_SIZE)
        )
        for y in range(MACRO_SIZE)
    ]
    connection = find_fragile_solid_connection(macro_solid)
    if connection is None:
        return []
    x, y = connection
    return [
        f"fragile solid connection at macro ({x}, {y}) joins substantial masses"
    ]


def verify_air_connected(solid: Sequence[Sequence[int]]) -> list[str]:
    air = [
        (x, y)
        for y in range(IMAGE_SIZE)
        for x in range(IMAGE_SIZE)
        if not solid[y][x]
    ]
    if not air:
        return ["chunk contains no traversable space"]
    visited = {air[0]}
    queue = deque([air[0]])
    while queue:
        x, y = queue.popleft()
        for nx, ny in ((x - 1, y), (x + 1, y), (x, y - 1), (x, y + 1)):
            if (
                0 <= nx < IMAGE_SIZE
                and 0 <= ny < IMAGE_SIZE
                and not solid[ny][nx]
                and (nx, ny) not in visited
            ):
                visited.add((nx, ny))
                queue.append((nx, ny))
    if len(visited) != len(air):
        return [f"{len(air) - len(visited)} open pixels are disconnected"]
    return []


def grid_cell(grid: Sequence[Sequence[int]], x: int, y: int) -> bool:
    return 0 <= x < IMAGE_SIZE and 0 <= y < IMAGE_SIZE and bool(grid[y][x])


def spike_outward_directions(
    solid: Sequence[Sequence[int]],
    x: int,
    y: int,
) -> tuple[tuple[int, int], ...]:
    if not grid_cell(solid, x, y):
        return ()
    return tuple(
        (dx, dy)
        for dx, dy in CARDINAL_DIRECTIONS
        if not grid_cell(solid, x + dx, y + dy)
    )


def concave_corner_diagonal(
    solid: Sequence[Sequence[int]],
    x: int,
    y: int,
) -> tuple[int, int] | None:
    if spike_outward_directions(solid, x, y):
        return None
    diagonal_air = [
        (dx, dy)
        for dx, dy in DIAGONAL_DIRECTIONS
        if not grid_cell(solid, x + dx, y + dy)
    ]
    return diagonal_air[0] if len(diagonal_air) == 1 else None


def terrain_surface_depths(
    solid: Sequence[Sequence[int]],
) -> list[list[int]]:
    unreachable = IMAGE_SIZE * 2
    depths = [[unreachable] * IMAGE_SIZE for _ in range(IMAGE_SIZE)]
    queue = deque()
    for y in range(IMAGE_SIZE):
        for x in range(IMAGE_SIZE):
            if solid[y][x]:
                continue
            depths[y][x] = 0
            queue.append((x, y))

    while queue:
        x, y = queue.popleft()
        next_depth = depths[y][x] + 1
        for dx, dy in CARDINAL_DIRECTIONS:
            neighbor_x = x + dx
            neighbor_y = y + dy
            if not (
                0 <= neighbor_x < IMAGE_SIZE
                and 0 <= neighbor_y < IMAGE_SIZE
            ):
                continue
            if next_depth >= depths[neighbor_y][neighbor_x]:
                continue
            depths[neighbor_y][neighbor_x] = next_depth
            queue.append((neighbor_x, neighbor_y))
    return depths


def spike_cell_is_reserved(geometry: ChunkGeometry, x: int, y: int) -> bool:
    if (
        x < SPIKE_EDGE_MARGIN
        or y < SPIKE_EDGE_MARGIN
        or x >= IMAGE_SIZE - SPIKE_EDGE_MARGIN
        or y >= IMAGE_SIZE - SPIKE_EDGE_MARGIN
    ):
        return True

    connector_margin = 18
    connector_span_start = FINAL_PORT_START - 14
    connector_span_end = FINAL_PORT_END + 14
    if (
        "L" in geometry.connectors
        and x < connector_margin
        and connector_span_start <= y < connector_span_end
    ):
        return True
    if (
        "R" in geometry.connectors
        and x >= IMAGE_SIZE - connector_margin
        and connector_span_start <= y < connector_span_end
    ):
        return True
    if (
        "U" in geometry.connectors
        and y < connector_margin
        and connector_span_start <= x < connector_span_end
    ):
        return True
    if (
        "D" in geometry.connectors
        and y >= IMAGE_SIZE - connector_margin
        and connector_span_start <= x < connector_span_end
    ):
        return True

    special_margin = 6
    return any(
        x0 - special_margin <= x < x1 + special_margin
        and y0 - special_margin <= y < y1 + special_margin
        for x0, y0, x1, y1 in geometry.special_regions
    )


def spike_surface_graph(
    geometry: ChunkGeometry,
) -> tuple[
    dict[tuple[int, int], tuple[tuple[int, int], ...]],
    list[list[int]],
]:
    """Build a two-cell surface band so paths can bridge pixel-grid corners."""

    depths = terrain_surface_depths(geometry.solid)
    surface_cells = set()
    corner_bridges: dict[
        tuple[int, int],
        tuple[tuple[int, int], tuple[int, int]],
    ] = {}
    for y in range(IMAGE_SIZE):
        for x in range(IMAGE_SIZE):
            if spike_cell_is_reserved(geometry, x, y):
                continue
            if not geometry.solid[y][x]:
                continue
            if depths[y][x] == 1:
                surface_cells.add((x, y))
                continue
            if depths[y][x] != SPIKE_SURFACE_BAND_DEPTH:
                continue
            diagonal = concave_corner_diagonal(geometry.solid, x, y)
            if diagonal is None:
                continue
            diagonal_x, diagonal_y = diagonal
            bridge_neighbors = (
                (x + diagonal_x, y),
                (x, y + diagonal_y),
            )
            if any(
                depths[neighbor_y][neighbor_x] != 1
                for neighbor_x, neighbor_y in bridge_neighbors
            ):
                continue
            surface_cells.add((x, y))
            corner_bridges[(x, y)] = bridge_neighbors

    graph = {}
    for x, y in sorted(surface_cells, key=lambda cell: (cell[1], cell[0])):
        graph[(x, y)] = tuple(
            (x + dx, y + dy)
            for dx, dy in CARDINAL_DIRECTIONS
            if (x + dx, y + dy) in surface_cells
            and (
                (x, y) not in corner_bridges
                or (x + dx, y + dy) in corner_bridges[(x, y)]
            )
            and (
                (x + dx, y + dy) not in corner_bridges
                or (x, y) in corner_bridges[(x + dx, y + dy)]
            )
        )
    return graph, depths


def spike_path_would_branch(
    path_cells: set[tuple[int, int]],
    current: tuple[int, int],
    candidate: tuple[int, int],
) -> bool:
    adjacent_path_cells = sum(
        (candidate[0] + dx, candidate[1] + dy) in path_cells
        for dx, dy in CARDINAL_DIRECTIONS
    )
    return adjacent_path_cells != 1 or current not in {
        (candidate[0] + dx, candidate[1] + dy)
        for dx, dy in CARDINAL_DIRECTIONS
    }


def spike_path_would_fill_block(
    path_cells: set[tuple[int, int]],
    candidate: tuple[int, int],
) -> bool:
    cells = path_cells | {candidate}
    x, y = candidate
    return any(
        all(
            (origin_x + dx, origin_y + dy) in cells
            for dx in (0, 1)
            for dy in (0, 1)
        )
        for origin_x in (x - 1, x)
        for origin_y in (y - 1, y)
    )


def weighted_spike_neighbor(
    options: Sequence[tuple[int, int]],
    current: tuple[int, int],
    previous: tuple[int, int] | None,
    depths: Sequence[Sequence[int]],
    rng: random.Random,
) -> tuple[int, int]:
    weights = []
    previous_direction = None
    if previous is not None:
        previous_direction = (
            current[0] - previous[0],
            current[1] - previous[1],
        )
    for option_x, option_y in options:
        direction = (option_x - current[0], option_y - current[1])
        weight = 3.0 if depths[option_y][option_x] == 1 else 1.0
        if direction == previous_direction:
            weight *= 1.25
        weights.append(weight)

    choice = rng.random() * sum(weights)
    for option, weight in zip(options, weights):
        choice -= weight
        if choice <= 0.0:
            return option
    return options[-1]


def grow_spike_path(
    graph: dict[tuple[int, int], tuple[tuple[int, int], ...]],
    depths: Sequence[Sequence[int]],
    start: tuple[int, int],
    target_length: int,
    rng: random.Random,
) -> tuple[tuple[int, int], ...]:
    """Grow one simple contour path without branches or filled 2x2 blocks."""

    path = [start]
    path_cells = {start}
    while len(path) < target_length:
        current = path[-1]
        options = [
            neighbor
            for neighbor in graph[current]
            if neighbor not in path_cells
            and not spike_path_would_branch(path_cells, current, neighbor)
            and not spike_path_would_fill_block(path_cells, neighbor)
        ]
        if not options:
            break
        previous = path[-2] if len(path) > 1 else None
        next_cell = weighted_spike_neighbor(
            options,
            current,
            previous,
            depths,
            rng,
        )
        path.append(next_cell)
        path_cells.add(next_cell)
    return tuple(path)


def generated_spike_group_length(rng: random.Random) -> int:
    roll = rng.random()
    if roll < 0.08:
        return rng.randint(36, MAX_GENERATED_SPIKE_GROUP_SIZE)
    if roll < 0.35:
        return rng.randint(20, 35)
    return rng.randint(MIN_GENERATED_SPIKE_GROUP_SIZE, 19)


def spike_path_turn_count(path: Sequence[tuple[int, int]]) -> int:
    directions = [
        (right[0] - left[0], right[1] - left[1])
        for left, right in zip(path, path[1:])
    ]
    return sum(
        direction != previous
        for previous, direction in zip(directions, directions[1:])
    )


def spike_path_is_curved(path: Sequence[tuple[int, int]]) -> bool:
    return spike_path_turn_count(path) >= 2


def spike_path_follows_surface(
    solid: Sequence[Sequence[int]],
    path: Sequence[tuple[int, int]],
) -> bool:
    for path_index, (x, y) in enumerate(path):
        if spike_outward_directions(solid, x, y):
            continue
        diagonal = concave_corner_diagonal(solid, x, y)
        if diagonal is None or path_index in (0, len(path) - 1):
            return False
        previous = path[path_index - 1]
        following = path[path_index + 1]
        neighbor_offsets = {
            (previous[0] - x, previous[1] - y),
            (following[0] - x, following[1] - y),
        }
        expected_offsets = {(diagonal[0], 0), (0, diagonal[1])}
        if neighbor_offsets != expected_offsets:
            return False
    return True


def spike_group_candidates(
    geometry: ChunkGeometry,
    rng: random.Random,
) -> list[tuple[bool, tuple[tuple[int, int], ...]]]:
    graph, depths = spike_surface_graph(geometry)
    candidates = []
    seen: set[frozenset[tuple[int, int]]] = set()
    starts = list(graph)
    rng.shuffle(starts)
    for start in starts:
        for _ in range(2):
            path = grow_spike_path(
                graph,
                depths,
                start,
                generated_spike_group_length(rng),
                rng,
            )
            if len(path) < MIN_GENERATED_SPIKE_GROUP_SIZE:
                continue
            if not spike_path_follows_surface(geometry.solid, path):
                continue
            key = frozenset(path)
            if key in seen:
                continue
            seen.add(key)
            candidates.append((spike_path_is_curved(path), path))
    rng.shuffle(candidates)
    return candidates


def spike_components(
    spikes: Sequence[Sequence[int]],
) -> list[set[tuple[int, int]]]:
    remaining = {
        (x, y)
        for y in range(IMAGE_SIZE)
        for x in range(IMAGE_SIZE)
        if spikes[y][x]
    }
    components = []
    while remaining:
        start = remaining.pop()
        component = {start}
        queue = [start]
        while queue:
            x, y = queue.pop()
            for neighbor in (
                (x - 1, y),
                (x + 1, y),
                (x, y - 1),
                (x, y + 1),
            ):
                if neighbor in remaining:
                    remaining.remove(neighbor)
                    component.add(neighbor)
                    queue.append(neighbor)
        components.append(component)
    return components


def spike_group_has_clearance(
    cells: Sequence[tuple[int, int]],
    spikes: Sequence[Sequence[int]],
) -> bool:
    existing = [
        (x, y)
        for y in range(IMAGE_SIZE)
        for x in range(IMAGE_SIZE)
        if spikes[y][x]
    ]
    return all(
        max(abs(x - other_x), abs(y - other_y)) > MIN_SPIKE_GROUP_GAP
        for x, y in cells
        for other_x, other_y in existing
    )


def spike_navigation_masks(
    geometry: ChunkGeometry,
) -> tuple[list[bytearray], list[bytearray]]:
    support = [bytearray(row) for row in geometry.solid]
    blocked = [bytearray(row) for row in geometry.solid]
    for y in range(IMAGE_SIZE):
        for x in range(IMAGE_SIZE):
            if not geometry.spikes[y][x]:
                continue
            support[y][x] = 0
            for out_x, out_y in spike_outward_directions(
                geometry.solid,
                x,
                y,
            ):
                danger_x = x + out_x
                danger_y = y + out_y
                if 0 <= danger_x < IMAGE_SIZE and 0 <= danger_y < IMAGE_SIZE:
                    blocked[danger_y][danger_x] = 1
    return support, blocked


def ordered_spike_component(
    component: set[tuple[int, int]],
) -> tuple[tuple[int, int], ...]:
    endpoints = [
        cell
        for cell in component
        if sum(
            (cell[0] + dx, cell[1] + dy) in component
            for dx, dy in CARDINAL_DIRECTIONS
        )
        == 1
    ]
    if len(endpoints) != 2:
        return ()

    path = [min(endpoints)]
    previous = None
    while len(path) < len(component):
        current = path[-1]
        options = [
            (current[0] + dx, current[1] + dy)
            for dx, dy in CARDINAL_DIRECTIONS
            if (current[0] + dx, current[1] + dy) in component
            and (current[0] + dx, current[1] + dy) != previous
        ]
        if len(options) != 1:
            return ()
        previous, next_cell = current, options[0]
        path.append(next_cell)
    return tuple(path)


def spike_component_is_curved(component: set[tuple[int, int]]) -> bool:
    path = ordered_spike_component(component)
    return bool(path) and spike_path_is_curved(path)


def verify_spike_layout(geometry: ChunkGeometry) -> list[str]:
    errors = []
    for y in range(IMAGE_SIZE):
        for x in range(IMAGE_SIZE):
            if geometry.spikes[y][x] and not geometry.solid[y][x]:
                errors.append(f"spike at ({x}, {y}) does not replace terrain")
                return errors

    components = spike_components(geometry.spikes)
    for component in components:
        if len(component) < MIN_SPIKE_GROUP_SIZE:
            errors.append(
                f"spike group has {len(component)} cells; minimum is "
                f"{MIN_SPIKE_GROUP_SIZE}"
            )
            break
        path = ordered_spike_component(component)
        if not path:
            errors.append("spike group must be a non-branching one-tile-wide path")
            break
        if not spike_path_follows_surface(geometry.solid, path):
            errors.append(
                "subsurface spike is not a perpendicular concave-corner bridge"
            )
            break
        if any(
            all(
                (origin_x + dx, origin_y + dy) in component
                for dx in (0, 1)
                for dy in (0, 1)
            )
            for origin_x, origin_y in component
        ):
            errors.append("spike group contains a filled two-by-two block")
            break

    for index, first in enumerate(components):
        for second in components[index + 1 :]:
            distance = min(
                max(abs(x - other_x), abs(y - other_y))
                for x, y in first
                for other_x, other_y in second
            )
            if distance <= MIN_SPIKE_GROUP_GAP:
                errors.append(
                    f"spike groups have only {distance - 1} intervening tiles"
                )
                return errors
    return errors


def spike_aware_traversal(
    geometry: ChunkGeometry,
    profile: MovementProfile,
) -> tuple[list[str], int]:
    support, blocked = spike_navigation_masks(geometry)
    return verify_traversal(
        support,
        geometry.labels,
        geometry.connectors,
        profile,
        blocked,
    )


def place_spike_groups(
    geometry: ChunkGeometry,
    profile: MovementProfile = MovementProfile(),
) -> int:
    geometry.spikes = [bytearray(IMAGE_SIZE) for _ in range(IMAGE_SIZE)]
    rng = random.Random(geometry.seed ^ 0x5A17_E5A5)
    target_count = rng.randint(MIN_SPIKE_GROUPS, MAX_SPIKE_GROUPS)
    candidates = spike_group_candidates(geometry, rng)

    accepted = 0
    accepted_curved = 0
    while accepted < target_count:
        prefer_curved = accepted_curved < min(
            MIN_CURVED_SPIKE_GROUPS,
            target_count,
        )
        candidate_index = next(
            (
                index
                for index, (curved, cells) in enumerate(candidates)
                if (curved or not prefer_curved)
                and spike_group_has_clearance(cells, geometry.spikes)
            ),
            None,
        )
        if candidate_index is None:
            candidate_index = next(
                (
                    index
                    for index, (_, cells) in enumerate(candidates)
                    if spike_group_has_clearance(cells, geometry.spikes)
                ),
                None,
            )
        if candidate_index is None:
            break

        curved, cells = candidates.pop(candidate_index)
        for x, y in cells:
            geometry.spikes[y][x] = 1
        traversal_errors, _ = spike_aware_traversal(geometry, profile)
        if traversal_errors:
            for x, y in cells:
                geometry.spikes[y][x] = 0
            continue
        accepted += 1
        accepted_curved += int(curved)
    return accepted


def extract_platforms(
    support: Sequence[Sequence[int]],
    labels: Sequence[Sequence[int]],
    profile: MovementProfile,
    blocked: Sequence[Sequence[int]] | None = None,
) -> list[Platform]:
    blocked = support if blocked is None else blocked
    platforms = []
    size = len(support)
    for y in range(profile.body_height - 1, size - 1):
        x = 0
        while x < size:
            special = labels[y][x] == SPECIAL
            if not is_standable(support, blocked, x, y, profile):
                x += 1
                continue
            x0 = x
            while (
                x + 1 < size
                and is_standable(support, blocked, x + 1, y, profile)
                and (labels[y][x + 1] == SPECIAL) == special
            ):
                x += 1
            platforms.append(Platform(y, x0, x, special))
            x += 1
    return platforms


def is_standable(
    support: Sequence[Sequence[int]],
    blocked: Sequence[Sequence[int]],
    x: int,
    foot_y: int,
    profile: MovementProfile,
) -> bool:
    if not (profile.body_height - 1 <= foot_y < len(support) - 1):
        return False
    return support[foot_y + 1][x] and all(
        not blocked[foot_y - offset][x]
        for offset in range(profile.body_height)
    )


def body_clear(
    blocked: Sequence[Sequence[int]],
    x: int,
    foot_y: int,
    profile: MovementProfile,
) -> bool:
    if not (
        0 <= x < len(blocked[0])
        and profile.body_height - 1 <= foot_y < len(blocked)
    ):
        return False
    return all(
        not blocked[foot_y - offset][x]
        for offset in range(profile.body_height)
    )


def candidate_x_positions(platform: Platform) -> tuple[int, ...]:
    middle = (platform.x0 + platform.x1) // 2
    return tuple(dict.fromkeys((platform.x0, middle, platform.x1)))


def jump_position_pairs(source: Platform, target: Platform) -> Iterable[tuple[int, int]]:
    # A broad floor can be reached near a ledge even when its ends and midpoint
    # are far away. Sample around the nearest projection in both directions;
    # small lateral offsets also let a falling body clear the source's edge.
    pairs = [(x, y) for x in candidate_x_positions(source) for y in candidate_x_positions(target)]
    for start, end, reverse in ((source, target, False), (target, source, True)):
        for x in candidate_x_positions(start):
            closest = max(end.x0, min(end.x1, x))
            for offset in (0, -1, 1, -2, 2, -4, 4, -8, 8):
                y = max(end.x0, min(end.x1, closest + offset))
                pairs.append((y, x) if reverse else (x, y))
    return dict.fromkeys(pairs)


def jump_clear(
    blocked: Sequence[Sequence[int]],
    source: tuple[int, int],
    target: tuple[int, int],
    profile: MovementProfile,
) -> bool:
    source_x, source_y = source
    target_x, target_y = target
    if target_y > source_y:
        # Walking off a ledge starts almost level before gravity takes over.
        # Giving every descent a jump apex can incorrectly collide with the
        # underside of the platform above the source.
        control_y = source_y - 1.0
    else:
        rise = source_y - target_y
        apex_lift = min(profile.max_apex_lift, max(rise + 2, 4))
        apex_y = min(source_y, target_y) - max(2, apex_lift - rise)
        control_y = 2.0 * apex_y - 0.5 * (source_y + target_y)
    samples = max(12, abs(target_x - source_x) * 3, abs(target_y - source_y) * 3)
    for step in range(1, samples):
        t = step / samples
        x = round(source_x + (target_x - source_x) * t)
        y = round(
            (1.0 - t) * (1.0 - t) * source_y
            + 2.0 * (1.0 - t) * t * control_y
            + t * t * target_y
        )
        if not body_clear(blocked, x, y, profile):
            return False
    return True


def can_reach_platform(
    blocked: Sequence[Sequence[int]],
    source: Platform,
    target: Platform,
    profile: MovementProfile,
) -> bool:
    if source == target:
        return True
    vertical_delta = source.y - target.y
    if vertical_delta > profile.max_rise or -vertical_delta > profile.max_drop:
        return False

    horizontal_gap = max(
        0,
        target.x0 - source.x1,
        source.x0 - target.x1,
    )
    if horizontal_gap <= 1 and abs(vertical_delta) <= 2:
        # Adjacent contour runs on a tapered rock are a trivial step or a
        # short hop. A full jump arc starts inside the shared rock silhouette
        # and would incorrectly reject this local movement.
        return True
    allowed_horizontal = profile.max_horizontal + max(0, -vertical_delta) // 3
    if horizontal_gap > allowed_horizontal:
        return False

    for source_x, target_x in jump_position_pairs(source, target):
        if abs(target_x - source_x) > allowed_horizontal:
            continue
        if jump_clear(
            blocked,
            (source_x, source.y),
            (target_x, target.y),
            profile,
        ):
            return True
    return False


def platform_graph(
    blocked: Sequence[Sequence[int]],
    platforms: Sequence[Platform],
    profile: MovementProfile,
) -> list[set[int]]:
    graph = [set() for _ in platforms]
    for source_index, source in enumerate(platforms):
        for target_index, target in enumerate(platforms):
            if source_index == target_index:
                continue
            if can_reach_platform(blocked, source, target, profile):
                graph[source_index].add(target_index)
    return graph


def reachable_nodes(graph: Sequence[set[int]], starts: Iterable[int]) -> set[int]:
    visited = set(starts)
    queue = deque(visited)
    while queue:
        node = queue.popleft()
        for target in graph[node]:
            if target not in visited:
                visited.add(target)
                queue.append(target)
    return visited


def exit_platforms(
    platforms: Sequence[Platform],
    direction: str,
) -> set[int]:
    matches = set()
    for index, platform in enumerate(platforms):
        if platform.special:
            continue
        overlaps_port = (
            platform.x1 >= FINAL_PORT_START and platform.x0 < FINAL_PORT_END
        )
        if direction == "L" and platform.x0 <= 5 and 38 <= platform.y <= 58:
            matches.add(index)
        elif direction == "R" and platform.x1 >= 90 and 38 <= platform.y <= 58:
            matches.add(index)
        elif direction == "U" and platform.y <= 15 and overlaps_port:
            matches.add(index)
        elif direction == "D" and platform.y >= 75 and overlaps_port:
            matches.add(index)
    return matches


def verify_traversal(
    support: Sequence[Sequence[int]],
    labels: Sequence[Sequence[int]],
    connectors: str,
    profile: MovementProfile,
    blocked: Sequence[Sequence[int]] | None = None,
) -> tuple[list[str], int]:
    blocked = support if blocked is None else blocked
    platforms = extract_platforms(support, labels, profile, blocked)
    if not platforms:
        return ["chunk contains no standable surfaces"], 0
    graph = platform_graph(blocked, platforms, profile)
    exits = {
        direction: exit_platforms(platforms, direction)
        for direction in connectors
    }
    errors = [
        f"{direction} connector has no reachable staging platform"
        for direction, nodes in exits.items()
        if not nodes
    ]
    if errors:
        return errors, len(platforms)

    core_nodes = {index for index, platform in enumerate(platforms) if not platform.special}
    special_nodes = {index for index, platform in enumerate(platforms) if platform.special}
    for direction, starts in exits.items():
        reached = reachable_nodes(graph, starts)
        for other_direction, targets in exits.items():
            if direction != other_direction and reached.isdisjoint(targets):
                errors.append(
                    f"base movement cannot traverse from {direction} to {other_direction}"
                )
        missing_core = core_nodes - reached
        if missing_core:
            errors.append(
                f"{len(missing_core)} core platforms are unreachable from {direction}"
            )
            break

    if special_nodes and connectors:
        core_reached = reachable_nodes(graph, exits[connectors[0]])
        reachable_special = special_nodes & core_reached
        if reachable_special:
            errors.append(
                "special-mobility platform is reachable by the conservative base profile"
            )
    return errors, len(platforms)


def validate_geometry(
    geometry: ChunkGeometry,
    profile: MovementProfile = MovementProfile(),
    require_spikes: bool = True,
) -> ValidationReport:
    errors = []
    errors.extend(verify_edges(geometry.solid, geometry.connectors))
    errors.extend(verify_macro_blocks(geometry.solid))
    errors.extend(verify_vertical_clearance(geometry.solid))
    errors.extend(verify_horizontal_clearance(geometry.solid))
    errors.extend(verify_solid_connections(geometry.solid))
    errors.extend(verify_air_connected(geometry.solid))
    errors.extend(verify_spike_layout(geometry))
    spike_group_count = len(spike_components(geometry.spikes))
    if require_spikes and spike_group_count < MIN_SPIKE_GROUPS:
        errors.append(
            f"chunk has {spike_group_count} spike groups; minimum is "
            f"{MIN_SPIKE_GROUPS}"
        )
    curved_group_count = sum(
        spike_component_is_curved(component)
        for component in spike_components(geometry.spikes)
    )
    if require_spikes and curved_group_count < MIN_CURVED_SPIKE_GROUPS:
        errors.append(
            f"chunk has {curved_group_count} curved spike groups; minimum is "
            f"{MIN_CURVED_SPIKE_GROUPS}"
        )
    if geometry.special_regions and not any(
        platform.special
        for platform in extract_platforms(
            geometry.solid,
            geometry.labels,
            profile,
        )
    ):
        errors.append("special-mobility region has no standable destination")
    air_pixels = sum(
        1
        for row in geometry.solid
        for is_solid in row
        if not is_solid
    )
    air_fraction = air_pixels / (IMAGE_SIZE * IMAGE_SIZE)
    if not 0.22 <= air_fraction <= 0.50:
        errors.append(
            f"air fraction {air_fraction:.3f} is outside the 0.22..0.50 target"
        )
    traversal_errors, platform_count = spike_aware_traversal(geometry, profile)
    errors.extend(traversal_errors)
    return ValidationReport(tuple(errors), platform_count, air_fraction)


def generate_chunk(
    connectors: str,
    seed: int,
    special_branch_rate: float,
    max_attempts: int,
) -> tuple[ChunkGeometry, ValidationReport, int]:
    connectors = canonical_connectors(connectors)
    wants_special_branch = (
        random.Random(seed ^ 0xA11B17E).random() < special_branch_rate
    )
    last_report = None
    for attempt in range(max_attempts):
        candidate_seed = mixed_seed(seed, attempt)
        geometry = build_candidate(
            connectors,
            candidate_seed,
            wants_special_branch,
        )
        base_report = validate_geometry(geometry, require_spikes=False)
        if not base_report.valid:
            last_report = base_report
            continue
        place_spike_groups(geometry)
        report = validate_geometry(geometry)
        if report.valid:
            return geometry, report, attempt + 1
        last_report = report
    assert last_report is not None
    details = "; ".join(last_report.errors[:5])
    raise RuntimeError(
        f"failed to generate {connectors} chunk after {max_attempts} attempts: {details}"
    )


def save_geometry(path: Path, geometry: ChunkGeometry) -> None:
    metadata = PngImagePlugin.PngInfo()
    metadata.add_text("generator", "tools/generate_level_chunks.py")
    metadata.add_text("seed", str(geometry.seed))
    metadata.add_text("connectors", geometry.connectors)
    metadata.add_text("special_regions", json.dumps(geometry.special_regions))
    metadata.add_text(
        "spike_groups",
        str(len(spike_components(geometry.spikes))),
    )
    geometry_image(geometry).save(path, pnginfo=metadata, optimize=True)


def labels_from_regions(
    solid: Sequence[Sequence[int]],
    regions: Sequence[Sequence[int]],
) -> list[bytearray]:
    labels = [
        bytearray(CORE if not solid[y][x] else 0 for x in range(IMAGE_SIZE))
        for y in range(IMAGE_SIZE)
    ]
    for region in regions:
        if len(region) != 4:
            continue
        x0, y0, x1, y1 = (int(value) for value in region)
        for y in range(max(0, y0), min(IMAGE_SIZE, y1)):
            for x in range(max(0, x0), min(IMAGE_SIZE, x1)):
                if not solid[y][x]:
                    labels[y][x] = SPECIAL
    return labels


def load_geometry(path: Path) -> ChunkGeometry:
    with Image.open(path) as source:
        image = source.convert("RGBA")
        info = dict(source.info)
    if image.size != (IMAGE_SIZE, IMAGE_SIZE):
        raise ValueError(
            f"{path}: expected {IMAGE_SIZE}x{IMAGE_SIZE}, found {image.width}x{image.height}"
        )
    pixels = list(image.getdata())
    unexpected = [
        pixel
        for pixel in pixels
        if pixel not in ((0, 0, 0, 0), (0, 0, 0, 255), SPIKE_COLOR)
    ]
    if unexpected:
        raise ValueError(
            f"{path}: generated chunks may only contain transparent, opaque black, "
            "or opaque white pixels"
        )
    solid = [
        bytearray(
            1 if pixels[y * IMAGE_SIZE + x][3] else 0
            for x in range(IMAGE_SIZE)
        )
        for y in range(IMAGE_SIZE)
    ]
    spikes = [
        bytearray(
            1 if pixels[y * IMAGE_SIZE + x] == SPIKE_COLOR else 0
            for x in range(IMAGE_SIZE)
        )
        for y in range(IMAGE_SIZE)
    ]
    try:
        regions = json.loads(info.get("special_regions", "[]"))
    except json.JSONDecodeError as error:
        raise ValueError(f"{path}: invalid special_regions metadata") from error
    labels = labels_from_regions(solid, regions)
    return ChunkGeometry(
        solid,
        spikes,
        labels,
        connectors_from_filename(path),
        int(info.get("seed", "0")),
        [tuple(int(value) for value in region) for region in regions],
    )


def create_preview(
    paths: Sequence[Path],
    destination: Path,
    scale: int = 4,
) -> None:
    cell_width = IMAGE_SIZE * scale
    label_height = 18
    columns = min(3, len(paths))
    rows = math.ceil(len(paths) / columns)
    preview = Image.new(
        "RGBA",
        (columns * cell_width, rows * (cell_width + label_height)),
        (34, 38, 45, 255),
    )
    draw = ImageDraw.Draw(preview)
    font = ImageFont.load_default()
    for index, path in enumerate(paths):
        with Image.open(path) as source:
            chunk = source.convert("RGBA")
        background = Image.new("RGBA", chunk.size, (225, 231, 238, 255))
        background.alpha_composite(chunk)
        chunk = background.resize(
            (cell_width, cell_width),
            Image.Resampling.NEAREST,
        )
        column = index % columns
        row = index // columns
        x = column * cell_width
        y = row * (cell_width + label_height)
        preview.alpha_composite(chunk, (x, y))
        draw.text((x + 4, y + cell_width + 3), path.name, fill=(255, 255, 255), font=font)
    destination.parent.mkdir(parents=True, exist_ok=True)
    preview.save(destination)


def command_generate(args: argparse.Namespace) -> int:
    if args.footprint == 3:
        import large_level_chunks
        return large_level_chunks.generate(args)
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=True)
    generated_paths = []
    output_count = args.count if args.count is not None else len(args.connectors)
    requested_masks = [
        args.connectors[offset % len(args.connectors)]
        for offset in range(output_count)
    ]
    for offset, requested in enumerate(requested_masks):
        index = args.start_index + offset
        connectors = canonical_connectors(requested)
        path = output / f"{args.prefix}_{index:04d}_{connectors}.png"
        if path.exists() and not args.overwrite:
            raise FileExistsError(f"refusing to overwrite {path}; pass --overwrite")
        geometry, report, attempts = generate_chunk(
            connectors,
            args.seed + offset,
            args.special_branch_rate,
            args.max_attempts,
        )
        save_geometry(path, geometry)
        generated_paths.append(path)
        branch_text = " with mobility branch" if geometry.special_regions else ""
        spike_count = len(spike_components(geometry.spikes))
        print(
            f"wrote {path} ({report.air_fraction:.1%} air, "
            f"{report.platform_count} platforms, {spike_count} spike groups, "
            f"{attempts} attempt(s){branch_text})"
        )

    if args.preview is not None:
        create_preview(generated_paths, args.preview.resolve())
        print(f"wrote preview {args.preview.resolve()}")
    return 0


def command_regenerate(args: argparse.Namespace) -> int:
    regenerated_paths = []
    for path in args.paths:
        source = load_geometry(path)
        geometry = build_candidate(
            source.connectors,
            source.seed,
            bool(source.special_regions),
        )
        if geometry.solid != source.solid:
            raise RuntimeError(
                f"{path}: stored seed no longer reproduces the chunk geometry"
            )
        if geometry.special_regions != source.special_regions:
            raise RuntimeError(
                f"{path}: stored seed no longer reproduces special regions"
            )
        base_report = validate_geometry(geometry, require_spikes=False)
        if not base_report.valid:
            details = "; ".join(base_report.errors[:5])
            raise RuntimeError(f"{path}: regenerated base is invalid: {details}")
        place_spike_groups(geometry)
        report = validate_geometry(geometry)
        if not report.valid:
            details = "; ".join(report.errors[:5])
            raise RuntimeError(f"{path}: regenerated chunk is invalid: {details}")
        save_geometry(path, geometry)
        regenerated_paths.append(path)
        print(
            f"regenerated {path} ({report.air_fraction:.1%} air, "
            f"{report.platform_count} platforms, "
            f"{len(spike_components(geometry.spikes))} spike groups)"
        )

    if args.preview is not None:
        create_preview(regenerated_paths, args.preview.resolve())
        print(f"wrote preview {args.preview.resolve()}")
    return 0


def command_validate(args: argparse.Namespace) -> int:
    failed = False
    for path in args.paths:
        try:
            with Image.open(path) as image:
                large = image.size == (288, 288)
            if large:
                import large_level_chunks
                report = large_level_chunks.validate_file(path)
                if not report.valid:
                    failed = True
                print(f"{'valid' if report.valid else 'invalid'} {path}: {report.platform_count} platforms; " + "; ".join(report.errors))
                continue
            geometry = load_geometry(path)
            report = validate_geometry(geometry)
        except (OSError, ValueError) as error:
            failed = True
            print(error, file=sys.stderr)
            continue
        if report.valid:
            spike_count = len(spike_components(geometry.spikes))
            print(
                f"valid {path} ({report.air_fraction:.1%} air, "
                f"{report.platform_count} platforms, {spike_count} spike groups)"
            )
        else:
            failed = True
            print(f"invalid {path}:", file=sys.stderr)
            for error in report.errors:
                print(f"  - {error}", file=sys.stderr)
    return 1 if failed else 0


def command_self_test(args: argparse.Namespace) -> int:
    connector_sets = (
        "U",
        "D",
        "L",
        "R",
        "UD",
        "LR",
        "UL",
        "UR",
        "DL",
        "DR",
        "UDL",
        "UDR",
        "ULR",
        "DLR",
        "UDLR",
    )
    for index, connectors in enumerate(connector_sets):
        first, first_report, _ = generate_chunk(
            connectors,
            args.seed + index,
            0.35,
            args.max_attempts,
        )
        second, second_report, _ = generate_chunk(
            connectors,
            args.seed + index,
            0.35,
            args.max_attempts,
        )
        if not first_report.valid or not second_report.valid:
            raise AssertionError(f"self-test generated invalid {connectors} chunk")
        if geometry_image(first).tobytes() != geometry_image(second).tobytes():
            raise AssertionError(f"generation is not deterministic for {connectors}")
        if len(spike_components(first.spikes)) < MIN_SPIKE_GROUPS:
            raise AssertionError(f"generation omitted spikes for {connectors}")
        palette = set(geometry_image(first).getdata())
        expected_palette = {(0, 0, 0, 0), (0, 0, 0, 255), SPIKE_COLOR}
        if not palette <= expected_palette:
            raise AssertionError(f"generation emitted an invalid color for {connectors}")
    probe, _, _ = generate_chunk("LR", args.seed ^ 0xBAD5EED, 0.0, args.max_attempts)
    probe.solid[0][0] = 0
    if validate_geometry(probe).valid:
        raise AssertionError("validator accepted an undeclared edge opening")
    clearance_probe = [bytearray([1]) * IMAGE_SIZE for _ in range(IMAGE_SIZE)]
    clearance_probe[10][20] = 0
    clearance_probe[11][20] = 0
    if not verify_vertical_clearance(clearance_probe):
        raise AssertionError("validator accepted a two-tile vertical gap")
    shaft_probe = [bytearray([1]) * IMAGE_SIZE for _ in range(IMAGE_SIZE)]
    shaft_probe[20][30] = 0
    if not verify_horizontal_clearance(shaft_probe):
        raise AssertionError("validator accepted a one-tile vertical shaft")
    fragile_probe = [bytearray(MACRO_SIZE) for _ in range(MACRO_SIZE)]
    for y in range(10, 16):
        for x in range(2, 8):
            fragile_probe[y][x] = 1
        for x in range(10, 16):
            fragile_probe[y][x] = 1
    fragile_probe[12][8] = 1
    fragile_probe[12][9] = 1
    if find_fragile_solid_connection(fragile_probe) is None:
        raise AssertionError("validator accepted a fragile solid connection")

    spike_solid = [bytearray([1]) * IMAGE_SIZE for _ in range(IMAGE_SIZE)]
    for y in range(20):
        spike_solid[y] = bytearray(IMAGE_SIZE)
    valid_spikes = [bytearray(IMAGE_SIZE) for _ in range(IMAGE_SIZE)]
    valid_spikes[20][10:14] = bytes([1]) * MIN_SPIKE_GROUP_SIZE
    spike_probe = ChunkGeometry(
        spike_solid,
        valid_spikes,
        [bytearray(IMAGE_SIZE) for _ in range(IMAGE_SIZE)],
        "L",
        1,
        [],
    )
    if verify_spike_layout(spike_probe):
        raise AssertionError("validator rejected a valid four-cell spike group")

    corner_solid = [bytearray(IMAGE_SIZE) for _ in range(IMAGE_SIZE)]
    for y in range(20, IMAGE_SIZE):
        corner_solid[y][20:] = bytes([1]) * (IMAGE_SIZE - 20)
    corner_spikes = [bytearray(IMAGE_SIZE) for _ in range(IMAGE_SIZE)]
    corner_spikes[20][20:25] = bytes([1]) * 5
    for y in range(21, 25):
        corner_spikes[y][20] = 1
    corner_probe = ChunkGeometry(
        corner_solid,
        corner_spikes,
        [bytearray(IMAGE_SIZE) for _ in range(IMAGE_SIZE)],
        "L",
        2,
        [],
    )
    if verify_spike_layout(corner_probe):
        raise AssertionError("validator rejected a one-tile-wide convex-corner path")
    corner_component = spike_components(corner_spikes)[0]
    corner_path = ordered_spike_component(corner_component)
    if spike_path_turn_count(corner_path) != 1:
        raise AssertionError("corner spike path did not preserve its single turn")

    concave_solid = [bytearray([1]) * IMAGE_SIZE for _ in range(IMAGE_SIZE)]
    for y in range(20):
        concave_solid[y][:20] = bytes(20)
    concave_spikes = [bytearray(IMAGE_SIZE) for _ in range(IMAGE_SIZE)]
    concave_spikes[20][15:21] = bytes([1]) * 6
    for y in range(15, 20):
        concave_spikes[y][20] = 1
    concave_probe = ChunkGeometry(
        concave_solid,
        concave_spikes,
        [bytearray(IMAGE_SIZE) for _ in range(IMAGE_SIZE)],
        "L",
        3,
        [],
    )
    if verify_spike_layout(concave_probe):
        raise AssertionError("validator rejected a valid concave-corner bridge")

    spike_probe.spikes[20][13] = 0
    if not verify_spike_layout(spike_probe):
        raise AssertionError("validator accepted a three-cell spike group")
    spike_probe.spikes[20][13] = 1

    spike_probe.spikes[10][30] = 1
    if not verify_spike_layout(spike_probe):
        raise AssertionError("validator accepted a spike outside terrain")
    spike_probe.spikes[10][30] = 0

    spike_probe.spikes[20][17:21] = bytes([1]) * MIN_SPIKE_GROUP_SIZE
    if not verify_spike_layout(spike_probe):
        raise AssertionError("validator accepted spike groups less than four tiles apart")
    spike_probe.spikes[20][17:21] = bytes(4)

    spike_probe.spikes[20][10:14] = bytes(4)
    spike_probe.spikes[21][10:14] = bytes([1]) * MIN_SPIKE_GROUP_SIZE
    if not verify_spike_layout(spike_probe):
        raise AssertionError("validator accepted a subsurface non-corner spike group")
    print(f"self-test passed for {len(connector_sets)} connector combinations")
    return 0


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description=__doc__)
    subparsers = parser.add_subparsers(dest="command", required=True)

    generate = subparsers.add_parser(
        "generate",
        help="generate validated level chunks",
    )
    generate.add_argument(
        "--output",
        type=Path,
        default=Path("assets/levels/chunks"),
        help="destination directory (default: assets/levels/chunks)",
    )
    generate.add_argument(
        "--prefix",
        default="generated_chunk",
        help="filename prefix before the index and connector suffix",
    )
    generate.add_argument("--seed", type=int, default=1)
    generate.add_argument("--footprint", type=int, choices=(1, 3), default=1,
                          help="base-chunk footprint: 1 produces 96px, 3 produces 288px rooms")
    generate.add_argument(
        "--count",
        type=int,
        help="number of chunks; connector masks cycle when count exceeds the pool",
    )
    generate.add_argument(
        "--start-index",
        type=int,
        default=1,
        help="first numeric filename index (default: 1)",
    )
    generate.add_argument(
        "--connectors",
        nargs="+",
        required=True,
        metavar="MASK",
        help="connector masks to generate, for example LR UD UDLR",
    )
    generate.add_argument(
        "--special-branch-rate",
        type=float,
        default=0.75,
        help="chance per chunk of an optional mobility-gated dead end",
    )
    generate.add_argument("--max-attempts", type=int, default=250)
    generate.add_argument("--overwrite", action="store_true")
    generate.add_argument(
        "--preview",
        type=Path,
        help="optional contact-sheet PNG destination",
    )
    generate.set_defaults(handler=command_generate)

    regenerate = subparsers.add_parser(
        "regenerate",
        help="rebuild generated chunks in place from their stored seeds",
    )
    regenerate.add_argument("paths", type=Path, nargs="+")
    regenerate.add_argument(
        "--preview",
        type=Path,
        help="optional contact-sheet PNG destination",
    )
    regenerate.set_defaults(handler=command_regenerate)

    validate = subparsers.add_parser(
        "validate",
        help="validate generated chunk PNGs",
    )
    validate.add_argument("paths", type=Path, nargs="+")
    validate.set_defaults(handler=command_validate)

    self_test = subparsers.add_parser(
        "self-test",
        help="exercise every non-empty connector combination",
    )
    self_test.add_argument("--seed", type=int, default=730241)
    self_test.add_argument("--max-attempts", type=int, default=250)
    self_test.set_defaults(handler=command_self_test)
    return parser


def main() -> int:
    parser = build_parser()
    args = parser.parse_args()
    if hasattr(args, "special_branch_rate") and not 0.0 <= args.special_branch_rate <= 1.0:
        parser.error("--special-branch-rate must be between 0 and 1")
    if hasattr(args, "max_attempts") and args.max_attempts < 1:
        parser.error("--max-attempts must be positive")
    if hasattr(args, "count") and args.count is not None and args.count < 1:
        parser.error("--count must be positive")
    if hasattr(args, "start_index") and args.start_index < 1:
        parser.error("--start-index must be positive")
    return args.handler(args)


if __name__ == "__main__":
    raise SystemExit(main())
