"""Grow large cave plans from big chambers in a 3x3 footprint.

A plan is a set of chambers: rooms with flat floors, steep walls for wall
climbing, and domed, uneven ceilings. Chambers join only where the player can
climb between them. Side by side, two chambers share a few end columns and the
higher floor ends in a cliff over the lower one; a narrow shaft joins a lower
chamber's end to the middle of a higher floor. Every other pair stays a shelf's
thickness apart. Port chambers come first, then the planner grows and joins
chambers until the cave is one and as open as its family asks.

Carving opens each chamber, then a sideways warp makes the walls waver while
floors stay level. Some broad floors gain a raised plateau, and a few floating
rocks with bowl-shaped undersides hang a hop from a climbable wall. Thin necks
between rock masses are cut away, convex floor edges lose their corner tile, and
concave corners ease into a small slope.
"""
from __future__ import annotations

from collections import deque
from dataclasses import dataclass, field
import random
from pathlib import Path

from PIL import Image, PngImagePlugin

import generate_level_chunks as chunks

SIZE = chunks.IMAGE_SIZE * 3
GRID = SIZE // 2
PORT_START = chunks.IMAGE_SIZE + chunks.FINAL_PORT_START
PORT_END = chunks.IMAGE_SIZE + chunks.FINAL_PORT_END
FAMILIES = ("network", "grand", "winding")
MIN_ROCK_AREA = 12
ENCOUNTER_WIDTH = 48
ENCOUNTER_HEIGHT = 12
MAX_ATTEMPTS = 200

# Measurements below are grid cells of two tiles each.
PORT_LO, PORT_HI = PORT_START // 2, PORT_END // 2  # port columns/rows, end exclusive
MIDDLE_FLOOR = PORT_HI                              # floor row under the side ports
MIN_SHELF = 8                                       # thinner rock between chambers opens up
MAX_TUNNEL = 24                                     # deepest rock a shaft between chambers bores
WARP = 3.4                                          # broad sideways wall waver
WARP_BAND = 10                                      # rows between independent waver bands
RIPPLE = 0.8                                        # fine sideways wall roughness


def noise(rng: random.Random, length: int, spacing: float) -> list[float]:
    """Smooth value noise in [-1, 1] with features about `spacing` cells apart."""
    anchors = [rng.uniform(-1.0, 1.0) for _ in range(int(length / spacing) + 3)]
    values = []
    for x in range(length):
        i, t = divmod(x / spacing, 1.0)
        t = t * t * (3.0 - 2.0 * t)
        values.append(anchors[int(i)] + (anchors[int(i) + 1] - anchors[int(i)]) * t)
    return values


@dataclass
class Chamber:
    """A room with a flat floor whose top rock row is `floor`, spanning `left..right`.

    `roof[x - left]` is the highest open row of each column; columns too low to
    stand in stay closed, so the walls close in where the roof meets the floor.
    The span may run past the room's border, which cuts the chamber off flat.
    A plateau, `(left, right, height)`, raises part of the floor flat.
    """

    left: int
    right: int
    floor: int
    roof: list[int] = field(repr=False)
    plateau: tuple[int, int, int] | None = None

    @property
    def center(self) -> int:
        return (self.left + self.right) // 2

    def opening(self, x: int) -> tuple[int, int] | None:
        """The open rows of column `x`, from the roof down to just above the floor."""
        if not self.left <= x <= self.right:
            return None
        top = self.roof[x - self.left]
        floor = self.floor
        if self.plateau and self.plateau[0] <= x <= self.plateau[1]:
            floor -= self.plateau[2]
        return (top, floor - 1) if floor - top >= 3 else None


def chamber(rng: random.Random, left: int, right: int, floor: int, height: int,
            squareness: tuple[float, float] = (3.2, 5.5)) -> Chamber:
    """A domed room: steep walls, a ceiling with broad swells and small bumps."""
    power = rng.uniform(*squareness)
    width = right - left + 1
    broad, fine = noise(rng, width, max(14.0, width / rng.uniform(2.5, 4.0))), noise(rng, width, 4)
    swell = min(4.0, rng.uniform(0.1, 0.2) * height)
    roof = []
    for offset in range(width):
        u = (offset + 0.5) / width * 2.0 - 1.0
        fraction = (1.0 - abs(u) ** power) ** (1.0 / power)
        rise = fraction * (height + broad[offset] * swell) + fine[offset] * 0.8
        roof.append(floor - max(0, round(rise)))
    return Chamber(left, right, floor, roof)


def shaft(left: int, right: int, top: int, bottom: int) -> Chamber:
    """A straight-walled vertical opening; `bottom` is its floor row."""
    return Chamber(left, right, bottom, [top] * (right - left + 1))


@dataclass
class Plan:
    chambers: list[Chamber] = field(default_factory=list)
    # Chamber pairs that always open into each other across their shared columns.
    joins: set[tuple[int, int]] = field(default_factory=set)
    # Rock regions other chambers must leave whole, as inclusive (x0, y0, x1, y1).
    keep: list[tuple[int, int, int, int]] = field(default_factory=list)
    # Shafts, which carry joins and never grow chambers of their own.
    shafts: set[int] = field(default_factory=set)


class CaveCanvas:
    """The plan at grid resolution: True is rock."""

    def __init__(self) -> None:
        self.rock = [[True] * GRID for _ in range(GRID)]

    def is_rock(self, x: int, y: int) -> bool:
        return not (0 <= x < GRID and 0 <= y < GRID) or self.rock[y][x]

    def set(self, x: int, y: int, rock: bool) -> None:
        if 0 <= x < GRID and 0 <= y < GRID:
            self.rock[y][x] = rock

    def carve(self, plan: Plan) -> None:
        """Open every chamber, merging columns where chambers meet.

        In each column, chambers separated by less than a shelf's thickness of rock
        open into one span; planned joins, such as a port shaft and its chamber,
        open across all their shared columns.
        """
        rooms = plan.chambers
        for x in range(2, GRID - 2):
            spans = sorted(((top, bottom, index) for index, room in enumerate(rooms)
                            if (span := room.opening(x)) for top, bottom in (span,)))
            merged: list[list[int]] = []
            members: list[set[int]] = []
            for top, bottom, index in spans:
                joined = merged and (top - merged[-1][1] <= MIN_SHELF
                                     or any((index, other) in plan.joins or (other, index) in plan.joins
                                            for other in members[-1]))
                if joined:
                    merged[-1][1] = max(merged[-1][1], bottom)
                    members[-1].add(index)
                else:
                    merged.append([top, bottom])
                    members.append({index})
            for top, bottom in merged:
                for y in range(max(2, top), min(GRID - 2, bottom + 1)):
                    self.rock[y][x] = False

    def broad_floors(self) -> int:
        """Floors long and tall enough for a ground encounter, with a little to spare
        for the softened corners at their ends."""
        rock = self.rock
        # Open cells directly above each cell, counted down each column.
        above = [[0] * GRID for _ in range(GRID)]
        for y in range(1, GRID):
            for x in range(GRID):
                above[y][x] = 0 if rock[y - 1][x] else above[y - 1][x] + 1
        count = 0
        for y in range(8, GRID - 2):
            run = 0
            for x in range(GRID):
                run = run + 1 if rock[y][x] and above[y][x] >= 7 else 0
                if run == ENCOUNTER_WIDTH // 2 + 3:
                    count += 1
        return count

    def warp(self, rng: random.Random) -> None:
        """Shift rows sideways by a smooth field so walls waver and floors stay flat.

        The shift fades out near the border, keeping the ports exact.
        """
        across = [noise(rng, GRID, rng.uniform(7, 11)) for _ in range(GRID // WARP_BAND + 3)]
        down, ripple = noise(rng, GRID, 12), noise(rng, GRID, 4)
        source = [row[:] for row in self.rock]
        for y in range(3, GRID - 3):
            band, t = divmod(y / WARP_BAND, 1.0)
            t = t * t * (3.0 - 2.0 * t)
            for x in range(3, GRID - 3):
                edge = min(x, y, GRID - 1 - x, GRID - 1 - y)
                fade = min(1.0, max(0.0, (edge - 3) / 8))
                field_value = across[int(band)][x] * (1 - t) + across[int(band) + 1][x] * t
                shift = round(fade * (WARP * (0.75 * field_value + 0.25 * down[y]) + RIPPLE * ripple[y]))
                self.rock[y][x] = source[y][min(GRID - 1, max(0, x - shift))]

    def ports(self, connectors: str) -> None:
        for y in range(GRID):
            for x in range(GRID):
                if min(x, y, GRID - 1 - x, GRID - 1 - y) < 2:
                    self.rock[y][x] = True
        lo, hi = PORT_LO, PORT_HI - 1
        for direction in connectors:
            x0, y0, x1, y1 = {"U": (lo, 0, hi, 3), "D": (lo, GRID - 4, hi, GRID - 1),
                              "L": (0, lo, 3, hi), "R": (GRID - 4, lo, GRID - 1, hi)}[direction]
            for y in range(y0, y1 + 1):
                for x in range(x0, x1 + 1):
                    self.rock[y][x] = False

    def cut_thin_necks(self) -> None:
        """Cut thin rock joins between solid masses so each floats or sits firmly.

        Rock more than a cell from any air forms the solid cores. A piece of thinner
        rock touching two different cores is a neck, however long; a massive rock
        never rests on one.
        """
        rock = self.rock
        inside = lambda x, y: 0 <= x < GRID and 0 <= y < GRID
        core = [[rock[y][x] and all(rock[y + dy][x + dx] for dy in (-1, 0, 1) for dx in (-1, 0, 1) if inside(x + dx, y + dy))
                 for x in range(GRID)] for y in range(GRID)]
        labels = [[0] * GRID for _ in range(GRID)]
        count = 0
        for y in range(GRID):
            for x in range(GRID):
                if core[y][x] and not labels[y][x]:
                    count += 1
                    queue = deque([(x, y)])
                    labels[y][x] = count
                    while queue:
                        cx, cy = queue.popleft()
                        for dx, dy in chunks.CARDINAL_DIRECTIONS:
                            nx, ny = cx + dx, cy + dy
                            if inside(nx, ny) and core[ny][nx] and not labels[ny][nx]:
                                labels[ny][nx] = count
                                queue.append((nx, ny))
        near_core = [[rock[y][x] and any(core[y + dy][x + dx] for dy in (-1, 0, 1) for dx in (-1, 0, 1) if inside(x + dx, y + dy))
                      for x in range(GRID)] for y in range(GRID)]
        seen = set()
        cut = []
        for y in range(2, GRID - 2):
            for x in range(2, GRID - 2):
                if not rock[y][x] or near_core[y][x] or (x, y) in seen:
                    continue
                piece, touched = [], set()
                queue = deque([(x, y)])
                seen.add((x, y))
                while queue:
                    cx, cy = queue.popleft()
                    piece.append((cx, cy))
                    for dx, dy in chunks.CARDINAL_DIRECTIONS:
                        nx, ny = cx + dx, cy + dy
                        if not inside(nx, ny) or not rock[ny][nx]:
                            continue
                        if near_core[ny][nx]:
                            touched.update(labels[ny + ddy][nx + ddx] for ddx in (-1, 0, 1) for ddy in (-1, 0, 1)
                                           if inside(nx + ddx, ny + ddy) and labels[ny + ddy][nx + ddx])
                        elif (nx, ny) not in seen:
                            seen.add((nx, ny))
                            queue.append((nx, ny))
                if len(touched) >= 2:
                    cut.extend(piece)
        for x, y in cut:
            rock[y][x] = False

    def tidy(self) -> None:
        image = self.image()
        pixels = image.load()
        # Erode the least-supported corner of a diagonal contact. Repeat since
        # removing one corner can expose another at the tip of the same spur.
        while contacts := diagonal_contacts(image):
            for a, b in contacts:
                def support(point):
                    x, y = point
                    return sum(pixels[x + dx, y + dy] == 255
                               for dx, dy in chunks.CARDINAL_DIRECTIONS
                               if 0 <= x + dx < GRID and 0 <= y + dy < GRID)
                pixels[min((a, b), key=support)] = 0
        for group in components(image, 255):
            if len(group) < MIN_ROCK_AREA:
                for point in group:
                    pixels[point] = 0
        for group in sorted(components(image, 0), key=len, reverse=True)[1:]:
            for point in group:
                pixels[point] = 255
        self.rock = [[pixels[x, y] == 255 for x in range(GRID)] for y in range(GRID)]

    def fillets(self) -> None:
        """Fill the corner cell where a floor meets a tall wall, easing the corner."""
        rock = self.is_rock
        fills = []
        for y in range(4, GRID - 4):
            for x in range(4, GRID - 4):
                if rock(x, y) or not rock(x, y + 1) or rock(x, y - 1):
                    continue
                for side in (-1, 1):
                    wall = all(rock(x + side, y - dy) for dy in range(4))
                    open_floor = all(not rock(x - side * dx, y) and rock(x - side * dx, y + 1) for dx in (1, 2, 3))
                    if wall and open_floor and not rock(x - side, y - 1):
                        fills.append((x, y))
        for x, y in fills:
            self.rock[y][x] = True

    def image(self) -> Image.Image:
        image = Image.new("L", (GRID, GRID), 0)
        image.putdata([255 if cell else 0 for row in self.rock for cell in row])
        return image

    def rgba(self) -> Image.Image:
        """The tile-resolution plan with softened corners.

        Each convex floor edge loses its outer corner tile, and each concave corner
        where a floor meets a wall gains a two-tile slope.
        """
        alpha = self.image().resize((SIZE, SIZE), Image.Resampling.NEAREST)
        tiles = alpha.load()
        rock = self.is_rock
        for y in range(3, GRID - 3):
            for x in range(3, GRID - 3):
                for side in (-1, 1):
                    near, far = 2 * x + (side > 0), 2 * x + (side < 0)
                    # Only floors at least three cells long qualify, not jogs in a wall.
                    floor_end = rock(x, y) and not rock(x + side, y) and not rock(x + side, y - 1) and all(
                        rock(x - side * step, y) and not rock(x - side * step, y - 1) for step in range(3))
                    floor_corner = rock(x + side, y) and rock(x + side, y - 1) and all(
                        not rock(x - side * step, y) and not rock(x - side * step, y - 1) and rock(x - side * step, y + 1)
                        for step in range(3))
                    if floor_end and rock(x, y + 1):
                        tiles[near, 2 * y] = 0
                    elif floor_corner:
                        tiles[near, 2 * y] = tiles[near, 2 * y + 1] = tiles[far, 2 * y + 1] = 255
        result = Image.new("RGBA", (SIZE, SIZE), (0, 0, 0, 0))
        result.putalpha(alpha)
        return result


def components(image: Image.Image, value: int) -> list[set[tuple[int, int]]]:
    pixels = image.load()
    pending = {(x, y) for y in range(image.height) for x in range(image.width) if pixels[x, y] == value}
    groups = []
    while pending:
        start = min(pending, key=lambda p: (p[1], p[0]))
        group = {start}
        queue = deque([start])
        pending.remove(start)
        while queue:
            x, y = queue.popleft()
            for dx, dy in chunks.CARDINAL_DIRECTIONS:
                point = x + dx, y + dy
                if point in pending:
                    pending.remove(point)
                    group.add(point)
                    queue.append(point)
        groups.append(group)
    return groups


def diagonal_contacts(image: Image.Image) -> list[tuple[tuple[int, int], tuple[int, int]]]:
    pixels = image.load()
    contacts = []
    for y in range(image.height - 1):
        for x in range(image.width - 1):
            square = [pixels[x, y], pixels[x + 1, y], pixels[x, y + 1], pixels[x + 1, y + 1]]
            if square == [255, 0, 0, 255]:
                contacts.append(((x, y), (x + 1, y + 1)))
            elif square == [0, 255, 255, 0]:
                contacts.append(((x + 1, y), (x, y + 1)))
    return contacts


SIZES = {
    # (chamber widths, chamber heights, share of the room left open)
    "network": ((40, 66), (24, 38), (0.34, 0.42)),
    "grand": ((40, 60), (24, 36), (0.36, 0.44)),
    "winding": ((30, 50), (20, 30), (0.30, 0.38)),
}


def trimmed(room: Chamber, left: int, right: int) -> Chamber:
    """The part of a chamber between two columns; the cut ends stay open."""
    left, right = max(left, room.left), min(right, room.right)
    return Chamber(left, right, room.floor, room.roof[left - room.left:right - room.left + 1])


def opens_into(room: Chamber, rect: tuple[int, int, int, int]) -> bool:
    x0, y0, x1, y1 = rect
    for x in range(max(x0, room.left), min(x1, room.right) + 1):
        span = room.opening(x)
        if span and span[0] <= y1 and span[1] >= y0:
            return True
    return False


def touching(a: Chamber, b: Chamber) -> bool:
    """Whether two chambers come within a shelf's thickness of each other anywhere."""
    for x in range(max(a.left, b.left, 2), min(a.right, b.right, GRID - 3) + 1):
        first, second = a.opening(x), b.opening(x)
        if first and second and max(first[0], second[0]) - min(first[1], second[1]) <= MIN_SHELF:
            return True
    # Chambers side by side need a wall as thick as a shelf too.
    for x in range(max(2, a.left), min(GRID - 3, a.right) + 1):
        span = a.opening(x)
        if not span:
            continue
        for nx in range(x - MIN_SHELF, x + MIN_SHELF + 1):
            other = b.opening(nx)
            if other and other[0] <= span[1] and other[1] >= span[0]:
                return True
    return False


def separation(a: Chamber, b: Chamber) -> float:
    gap = max(a.left - b.right, b.left - a.right, 0)
    return gap + abs(a.floor - b.floor) * 0.5


class Planner:
    """Grows a cave from chambers joined only in ways the player can always climb.

    Chambers join two ways. Side by side, two chambers share a few end columns,
    which open from the higher roof to the lower floor; the higher floor ends
    there in a cliff, because the lower chamber stops at the shared columns. A
    narrow shaft joins a lower chamber's end to the middle of a higher floor; the
    lower chamber stops at the shaft, so the shaft's wall on that side climbs from
    the lower floor to the higher one. Everything else stays a shelf's thickness
    apart, and the rock each climb needs is kept whole.
    """

    def __init__(self, rng: random.Random, family: str, connectors: str) -> None:
        self.rng, self.family, self.connectors = rng, family, connectors
        self.plan = Plan()
        # Chamber ends that already open onto a join or a port, as (index, side).
        self.used: set[tuple[int, int]] = set()
        # Columns of each chamber's floor already pierced by a shaft.
        self.holes: dict[int, list[tuple[int, int]]] = {}
        self.parent: list[int] = []
        self.need_broad = False

    # Bookkeeping.

    def root(self, index: int) -> int:
        while self.parent[index] != index:
            index = self.parent[index]
        return index

    def add(self, room: Chamber, partners: tuple[int, ...] = (), shaft: bool = False) -> int:
        index = len(self.plan.chambers)
        self.plan.chambers.append(room)
        self.parent.append(index)
        if shaft:
            self.plan.shafts.add(index)
        for partner in partners:
            self.plan.joins.add((partner, index))
            self.parent[self.root(index)] = self.root(partner)
        return index

    def fits(self, room: Chamber, partners: tuple[int, ...] = (),
             keep: tuple[tuple[int, int, int, int], ...] = ()) -> bool:
        if room.left < 3 or room.right > GRID - 4:
            return False
        if min(room.roof) < 4 or room.floor > GRID - 8 or room.right - room.left < 12:
            return False
        if any(opens_into(room, rect) for rect in self.plan.keep):
            return False
        others = [other for index, other in enumerate(self.plan.chambers) if index not in partners]
        if any(touching(room, other) for other in others):
            return False
        return not any(opens_into(other, rect) for other in self.plan.chambers for rect in keep)

    def accept(self, room: Chamber, partners: tuple[int, ...], keep, ends=()) -> int | None:
        if not self.fits(room, partners, tuple(keep)):
            return None
        self.plan.keep += keep
        index = self.add(room, partners)
        for partner, side in ends:
            self.used.add((index if partner is None else partner, side))
        return index

    @staticmethod
    def short_shaft(high_floor: int, low: Chamber, left: int, right: int) -> bool:
        """Whether a shaft from a floor down to a lower chamber tunnels through little rock."""
        roofs = [span[0] for x in range(left, right + 1) if (span := low.opening(x))]
        return bool(roofs) and max(roofs) - high_floor <= MAX_TUNNEL

    def clear_of_joins(self, index: int, left: int, right: int) -> bool:
        """Whether columns of a chamber stay clear of the columns its other joins open."""
        return not any(left - 4 <= hi and lo <= right + 4 for lo, hi in self.holes.get(index, ()))

    def opened(self, index: int, left: int, right: int) -> None:
        self.holes.setdefault(index, []).append((left, right))

    def width(self) -> int:
        """A new chamber's width; broad while the room still lacks encounter floors."""
        low, high = SIZES[self.family][0]
        return self.rng.randint(max(low, 42), max(high, 56)) if self.need_broad else self.rng.randint(low, high)

    def room(self, left: int, width: int, floor: int, height: int) -> Chamber:
        return chamber(self.rng, left, left + width - 1, floor, max(6, min(height, floor - 6)))

    def components(self) -> dict[int, list[int]]:
        groups: dict[int, list[int]] = {}
        for index in range(len(self.plan.chambers)):
            if index not in self.plan.shafts:
                groups.setdefault(self.root(index), []).append(index)
        return groups

    # Ports.

    def ports(self) -> list[int]:
        """Chambers that open onto each port; the rock each port's climb needs stays whole."""
        rng, plan = self.rng, self.plan
        widths, heights, _ = SIZES[self.family]
        anchors = []
        if "L" in self.connectors:
            index = self.add(chamber(rng, -rng.randint(6, 24), rng.randint(36, 60), MIDDLE_FLOOR, rng.randint(16, 28)))
            plan.keep.append((2, MIDDLE_FLOOR, 6, MIDDLE_FLOOR + 4))
            self.used.add((index, -1))
            anchors.append(index)
        if "R" in self.connectors:
            index = self.add(chamber(rng, GRID - 1 - rng.randint(36, 60), GRID - 1 + rng.randint(6, 24), MIDDLE_FLOOR,
                                     rng.randint(16, 28)))
            plan.keep.append((GRID - 7, MIDDLE_FLOOR, GRID - 3, MIDDLE_FLOOR + 4))
            self.used.add((index, 1))
            anchors.append(index)
        if "U" in self.connectors:
            # The chimney's outer wall lines up with the port's edge and climbs
            # unbroken from the chamber floor, so nothing may open beside it.
            for _ in range(24):
                floor = rng.randint(24, 44)
                width, height = rng.randint(max(widths[0], 42), max(widths[1], 56)), min(rng.randint(*heights), floor - 6)
                spare, reach = rng.randint(0, 10), rng.randint(0, 5)
                if rng.randrange(2):
                    room = trimmed(chamber(rng, PORT_HI - width, PORT_HI - 1 + spare, floor, height), 0, PORT_HI - 1)
                    chimney = shaft(PORT_LO - reach, PORT_HI - 1, 2, floor)
                    keep, side = (PORT_HI, 2, PORT_HI + 5, floor + MIN_SHELF), 1
                else:
                    room = trimmed(chamber(rng, PORT_LO - spare, PORT_LO + width - 1, floor, height), PORT_LO, GRID)
                    chimney = shaft(PORT_LO, PORT_HI - 1 + reach, 2, floor)
                    keep, side = (PORT_LO - 6, 2, PORT_LO - 1, floor + MIN_SHELF), -1
                if self.fits(room, keep=(keep,)):
                    index = self.add(room)
                    self.add(chimney, (index,), shaft=True)
                    plan.keep.append(keep)
                    self.used.add((index, side))
                    self.holes[index] = [(chimney.left - 2, chimney.right + 2)]
                    anchors.append(index)
                    break
            else:
                return []
        if "D" in self.connectors:
            for _ in range(24):
                floor = rng.randint(118, 132)
                width = rng.randint(max(widths[0], 42), max(widths[1], 56))
                left = rng.randint(PORT_HI + 8 - width, PORT_LO - 8)
                room = chamber(rng, left, left + width - 1, floor, min(rng.randint(*heights), floor - 8))
                keep = [(PORT_LO - 4, floor, PORT_LO - 1, GRID - 3), (PORT_HI, floor, PORT_HI + 3, GRID - 3)]
                if self.fits(room, keep=tuple(keep)):
                    index = self.add(room)
                    self.add(shaft(PORT_LO, PORT_HI - 1, floor - 3, GRID), (index,), shaft=True)
                    plan.keep += keep
                    self.holes[index] = [(PORT_LO - 2, PORT_HI + 1)]
                    anchors.append(index)
                    break
            else:
                return []
        return anchors

    # Joins.

    def beside(self, index: int, side: int, floor: int | None = None,
               size: tuple[int, int] | None = None) -> int | None:
        """A new chamber sharing the end columns on `side` of chamber `index`.

        Near the room's edge the new chamber narrows to fit.
        """
        rng, here = self.rng, self.plan.chambers[index]
        if (index, side) in self.used:
            return None
        widths, heights, _ = SIZES[self.family]
        width, height = size or (self.width(), rng.randint(*heights))
        if floor is None:
            floor = here.floor + rng.choice((-1, 1)) * rng.randint(0, 36)
        floor = max(height + 6, min(GRID - 9, floor))
        shared = max(6, min(24, round(abs(floor - here.floor) * 0.35) + rng.randint(6, 10),
                            width // 2, (here.right - here.left) // 2))
        room_edge = GRID - 4 - (here.right - shared + 1) + 1 if side > 0 else here.left + shared - 1 - 3 + 1
        width = min(width, room_edge)
        if width < max(24, 2 * shared):
            return None
        left = here.right - shared + 1 if side > 0 else here.left + shared - width
        room = self.room(left, width, floor, height)
        start, end = max(room.left, here.left), min(room.right, here.right)
        if not self.clear_of_joins(index, start, end):
            return None
        new = self.accept(room, (index,), self.cliff(here, room), ((index, side), (None, -side)))
        if new is not None:
            self.opened(index, start, end)
            self.opened(new, start, end)
        return new

    @staticmethod
    def cliff(a: Chamber, b: Chamber) -> list[tuple[int, int, int, int]]:
        """The rock under the higher of two side-by-side chambers, beside their shared columns."""
        high, low = sorted((a, b), key=lambda room: room.floor)
        if high.floor == low.floor:
            return []
        if high.center > low.center:
            x = min(a.right, b.right) + 1
            return [(x, high.floor, x + 3, low.floor)]
        x = max(a.left, b.left) - 1
        return [(x - 3, high.floor, x, low.floor)]

    def below(self, index: int, target: Chamber | None = None) -> int | None:
        """A new chamber under chamber `index`, whose end rises into a shaft through its floor."""
        rng, here = self.rng, self.plan.chambers[index]
        widths, heights, _ = SIZES[self.family]
        width = rng.randint(4, 6)
        if here.right - here.left < 2 * 10 + width + 4:
            return None
        right = rng.randint(here.left + 10 + width, here.right - 10)
        left = right - width + 1
        if not self.clear_of_joins(index, left, right):
            return None
        # The new roof stays just a shelf below this floor, keeping the shaft short.
        height = rng.randint(*heights)
        if target is not None:
            height = max(height, min(56, target.floor - here.floor - MIN_SHELF - 6))
        floor = here.floor + MIN_SHELF + rng.randint(5, 9) + height
        if floor > GRID - 9:
            return None
        room_width = self.width()
        # The chamber stops at the shaft on its outer side and reaches the other way.
        outer = rng.choice((-1, 1)) if target is None else (-1 if target.center > right else 1)
        if outer > 0:
            room = self.room(right - room_width + 1, room_width, floor, height)
            keep = [(right + 1, here.floor, right + 3, floor)]
        else:
            room = self.room(left, room_width, floor, height)
            keep = [(left - 3, here.floor, left - 1, floor)]
        if not self.short_shaft(here.floor, room, left, right):
            return None
        new = self.accept(room, (), keep, ((None, outer),))
        if new is None:
            return None
        self.add(shaft(left, right, here.floor - 3, floor), (index, new), shaft=True)
        self.opened(index, left, right)
        self.opened(new, left, right)
        return new

    def above(self, index: int, target: Chamber | None = None) -> int | None:
        """A new chamber over chamber `index`, which rises at its end into a shaft through
        the new chamber's floor."""
        rng, here = self.rng, self.plan.chambers[index]
        widths, heights, _ = SIZES[self.family]
        side = rng.choice((-1, 1))
        if target is not None and rng.random() < 0.7:
            side = 1 if target.center > here.center else -1
        if (index, side) in self.used:
            return None
        width = rng.randint(4, 6)
        right = here.right if side > 0 else here.left + width - 1
        left = right - width + 1
        if not self.clear_of_joins(index, left, right):
            return None
        floor = min(here.roof) - rng.randint(MIN_SHELF + 2, MIN_SHELF + 10)
        room_width, height = self.width(), rng.randint(*heights)
        if room_width < width + 24 or floor - 6 < 12:
            return None
        room = self.room(rng.randint(right + 11 - room_width, left - 10), room_width, floor, height)
        if not self.short_shaft(floor, here, left, right):
            return None
        keep = [(right + 1, floor, right + 3, here.floor)] if side > 0 else [(left - 3, floor, left - 1, here.floor)]
        new = self.accept(room, (), keep)
        if new is None:
            return None
        self.used.add((index, side))
        self.add(shaft(left, right, floor - 3, here.floor), (index, new), shaft=True)
        self.opened(index, left, right)
        self.opened(new, left, right)
        return new

    def meet(self, a: int, b: int) -> bool:
        """Join two existing chambers whose facing ends already share a few columns."""
        first, second = self.plan.chambers[a], self.plan.chambers[b]
        start, end = second.left, first.right
        if (a, 1) in self.used or (b, -1) in self.used or not 6 <= end - start + 1 <= 24:
            return False
        if first.left >= start or second.right <= end:
            return False
        if not (self.clear_of_joins(a, start, end) and self.clear_of_joins(b, start, end)):
            return False
        keep = self.cliff(first, second)
        others = [room for index, room in enumerate(self.plan.chambers) if index not in (a, b)]
        if any(opens_into(room, rect) for room in others for rect in keep):
            return False
        self.plan.keep += keep
        self.plan.joins.add((a, b))
        self.parent[self.root(a)] = self.root(b)
        self.used |= {(a, 1), (b, -1)}
        self.opened(a, start, end)
        self.opened(b, start, end)
        return True

    def cut_drop(self, upper: int, lower: int) -> bool:
        """Trim a lower chamber so its end sits under the middle of a higher floor,
        then join them with a shaft there.

        Only a part holding no other join or port is trimmed away.
        """
        rng = self.rng
        high, low = self.plan.chambers[upper], self.plan.chambers[lower]
        if low.floor - high.floor < MIN_SHELF + 16:
            return False
        for _ in range(6):
            side = rng.choice((-1, 1))
            if (lower, side) in self.used:
                continue
            # The kept part of the lower chamber stays at least thirty cells wide.
            start = max(high.left + 5, low.left + (30 if side > 0 else 0))
            end = min(high.right - 5, low.right - (0 if side > 0 else 30))
            if start > end:
                continue
            cut = rng.randint(start, end)
            removed = (cut + 1, low.right) if side > 0 else (low.left, cut - 1)
            if any(removed[0] <= hi and lo <= removed[1] for lo, hi in self.holes.get(lower, ())):
                continue
            width = rng.randint(4, 6)
            left, right = (cut - width + 1, cut) if side > 0 else (cut, cut + width - 1)
            if not self.clear_of_joins(upper, left, right):
                continue
            kept = trimmed(low, low.left, cut) if side > 0 else trimmed(low, cut, low.right)
            if not self.short_shaft(high.floor, kept, left, right):
                continue
            keep = (right + 1, high.floor, right + 3, low.floor) if side > 0 else (left - 3, high.floor, left - 1, low.floor)
            link = shaft(left, right, high.floor - 3, low.floor)
            others = [room for index, room in enumerate(self.plan.chambers) if index not in (upper, lower)]
            if any(touching(link, room) or opens_into(room, keep) for room in others) \
                    or any(opens_into(link, rect) for rect in self.plan.keep):
                continue
            self.plan.chambers[lower] = kept
            self.plan.keep.append(keep)
            self.used.add((lower, side))
            self.opened(upper, left, right)
            self.opened(lower, left, right)
            self.add(link, (upper, lower), shaft=True)
            return True
        return False

    def drop(self, upper: int, lower: int) -> bool:
        """A shaft from the middle of one existing chamber's floor down to another's end."""
        high, low = self.plan.chambers[upper], self.plan.chambers[lower]
        if low.floor - high.floor < MIN_SHELF + 16:
            return False
        for side in (1, -1):
            if (lower, side) in self.used:
                continue
            width = self.rng.randint(4, 6)
            right = low.right if side > 0 else low.left + width - 1
            left = right - width + 1
            if not (high.left + 5 <= left and right <= high.right - 5):
                continue
            if not (self.clear_of_joins(upper, left, right) and self.clear_of_joins(lower, left, right)):
                continue
            if not self.short_shaft(high.floor, low, left, right):
                continue
            keep = (right + 1, high.floor, right + 3, low.floor) if side > 0 else (left - 3, high.floor, left - 1, low.floor)
            link = shaft(left, right, high.floor - 3, low.floor)
            others = [room for index, room in enumerate(self.plan.chambers) if index not in (upper, lower)]
            if any(touching(link, room) or opens_into(room, keep) for room in others) \
                    or any(opens_into(link, rect) for rect in self.plan.keep):
                continue
            self.plan.keep.append(keep)
            self.used.add((lower, side))
            self.opened(upper, left, right)
            self.opened(lower, left, right)
            self.add(link, (upper, lower), shaft=True)
            return True
        return False

    def bridge(self, a: int, b: int) -> bool:
        """A chamber sharing end columns with both `a` and `b`, which face each other."""
        rng = self.rng
        first, second = self.plan.chambers[a], self.plan.chambers[b]
        if (a, 1) in self.used or (b, -1) in self.used or first.right >= second.left:
            return False
        for _ in range(6):
            one, two = rng.randint(6, 16), rng.randint(6, 16)
            left, right = first.right - one + 1, second.left + two - 1
            if right - left < 24 or right - left > 90:
                return False
            low, high = sorted((first.floor, second.floor), reverse=True)
            floor = low if rng.randrange(2) else rng.randint(high, low)
            height = rng.randint(*SIZES[self.family][1])
            room = chamber(rng, left, right, floor, max(6, min(height, floor - 6)))
            if not (self.clear_of_joins(a, left, first.right) and self.clear_of_joins(b, second.left, right)):
                continue
            keep = self.cliff(first, room) + self.cliff(room, second)
            new = self.accept(room, (a, b), keep, ((a, 1), (b, -1)))
            if new is not None:
                self.opened(a, left, first.right)
                self.opened(b, second.left, right)
                self.opened(new, left, first.right)
                self.opened(new, second.left, right)
                return True
        return False

    # Growth.

    def grow(self, index: int) -> int | None:
        choice = self.rng.random()
        if choice < 0.75:
            return self.beside(index, self.rng.choice((-1, 1)))
        if choice < 0.9:
            return self.below(index)
        return self.above(index)

    def connect(self) -> bool:
        """Join the parts of the cave, growing parts toward each other where no join fits."""
        chambers = self.plan.chambers
        for _ in range(80):
            groups = list(self.components().values())
            if len(groups) <= 1:
                return True
            pairs = sorted(((a, b) for number, group in enumerate(groups) for other in groups[number + 1:]
                            for a in group for b in other),
                           key=lambda pair: separation(chambers[pair[0]], chambers[pair[1]]))
            if any(self.drop(a, b) or self.drop(b, a)
                   or self.meet(*sorted((a, b), key=lambda index: chambers[index].center))
                   or self.cut_drop(*sorted((a, b), key=lambda index: chambers[index].floor))
                   or self.bridge(*sorted((a, b), key=lambda index: chambers[index].center))
                   for a, b in pairs[:24]):
                continue
            group = min(groups, key=len)
            nearest = sorted(((a, b) for a in group for b in range(len(chambers))
                              if b not in group and b not in self.plan.shafts),
                             key=lambda pair: separation(chambers[pair[0]], chambers[pair[1]]))
            near, far = self.rng.choice(nearest[:4])
            here, there = chambers[near], chambers[far]
            for _ in range(8):
                if there.floor - here.floor > 30 and self.rng.random() < 0.6:
                    grown = self.below(near, there)
                elif here.floor - there.floor > 30 and self.rng.random() < 0.6:
                    grown = self.above(near, there)
                else:
                    step = max(-36, min(36, there.floor - here.floor))
                    side = 1 if there.center > here.center else -1
                    grown = self.beside(near, side, here.floor + round(step * self.rng.uniform(0.3, 1.0)))
                if grown is not None:
                    break
            else:
                for _ in range(8):
                    if self.grow(self.rng.choice(group)) is not None:
                        break
        return len(self.components()) <= 1

    def open_share(self) -> float:
        cells = 0
        for x in range(2, GRID - 2):
            rows = set()
            for room in self.plan.chambers:
                if span := room.opening(x):
                    rows.update(range(max(2, span[0]), span[1] + 1))
            cells += len(rows)
        return cells / (GRID * GRID)

    def broad_count(self) -> int:
        canvas = CaveCanvas()
        canvas.carve(self.plan)
        return canvas.broad_floors()

    def fill(self) -> None:
        """Grow chambers off the cave until the room is as open as its family wants,
        with at least three floors broad enough for ground encounters."""
        _, _, share = SIZES[self.family]
        target = self.rng.uniform(*share)
        rooms_seen = -1
        for _ in range(120):
            if len(self.plan.chambers) != rooms_seen:
                rooms_seen = len(self.plan.chambers)
                self.need_broad = self.broad_count() < 3
                if not self.need_broad and self.open_share() >= target:
                    break
            rooms = [index for index in range(len(self.plan.chambers)) if index not in self.plan.shafts]
            index = self.rng.choice(rooms)
            if self.need_broad and self.rng.random() < 0.5:
                # Carry an existing floor level on into a new chamber.
                self.beside(index, self.rng.choice((-1, 1)), self.plan.chambers[index].floor)
            else:
                self.grow(index)
        self.need_broad = False

    def plateaus(self) -> None:
        """Raise a flat plateau across part of some broad floors.

        A plateau stays clear of the chamber's ends and of shafts through its
        floor, rises two to four cells, leaves headroom above it, and never costs
        the room one of its three broad floors.
        """
        rng = self.rng
        for index, room in enumerate(self.plan.chambers):
            lo, hi = max(room.left, 3), min(room.right, GRID - 4)
            if index in self.plan.shafts or hi - lo < 50 or rng.random() < 0.35:
                continue
            size = min(round((hi - lo) * rng.uniform(0.3, 0.5)), hi - lo - 29)
            left = rng.randint(lo + 14, hi - 14 - size)
            right = left + size
            if any(left - 6 <= end and start <= right + 6 for start, end in self.holes.get(index, ())):
                continue
            height = rng.randint(2, 4)
            if any(room.floor - height - room.roof[x - room.left] < 10 for x in range(left, right + 1)):
                continue
            before = self.broad_count()
            room.plateau = (left, right, height)
            if self.broad_count() < min(3, before):
                room.plateau = None


def grand_hall(planner: Planner) -> bool:
    """One huge hall for the grand family, grown off a port's chamber."""
    rng = planner.rng
    rooms = [index for index in range(len(planner.plan.chambers)) if index not in planner.plan.shafts]
    for _ in range(48):
        size = rng.randint(80, 112), rng.randint(40, 58)
        index = rng.choice(rooms)
        here = planner.plan.chambers[index]
        floor = here.floor + rng.randint(-20, 60)
        if planner.beside(index, rng.choice((-1, 1)), floor, size) is not None:
            return True
    return False


def slab(canvas: CaveCanvas, rng: random.Random, x0: int, top: int, width: int, depth: int) -> None:
    """A floating rock with a flat top and a deep, uneven, bowl-shaped underside.

    The deepest point sits off center, and the underside swells and dips so no
    two slabs hang alike; the ends keep two cells of rock.
    """
    power = rng.uniform(1.6, 2.6)
    lean = rng.uniform(-0.35, 0.35)
    bumps = noise(rng, width, rng.uniform(3, 6))
    for offset in range(width):
        u = (offset + 0.5) / width * 2.0 - 1.0
        u = (u - lean) / (1.0 + abs(lean)) if u > lean else (u - lean) / (1.0 + abs(lean))
        fraction = max(0.0, 1.0 - abs(u) ** power) ** (1.0 / power)
        hang = max(2, round(depth * fraction + bumps[offset] * depth * 0.18))
        for y in range(top, top + hang):
            canvas.set(x0 + offset, y, True)


def floating_slabs(canvas: CaveCanvas, rng: random.Random, count: int) -> None:
    """Broad floating rocks high in open space, a hop from a climbable wall.

    A slab's top sits at least twelve cells above the floor beneath it, too high
    to jump to; climbing the wall beside one of its ends reaches it.
    """
    rock = canvas.is_rock
    placed = 0
    for _ in range(400):
        if placed >= count:
            return
        width = rng.randint(14, 26)
        depth = round(width * rng.uniform(0.3, 0.45))
        x0, top = rng.randint(6, GRID - 7 - width), rng.randint(10, GRID - 20)
        if any(rock(x, y) for y in range(top - 7, top + depth + 4) for x in range(x0 - 3, x0 + width + 3)):
            continue
        ground = []
        for x in range(x0, x0 + width):
            y = top + depth
            while not rock(x, y):
                y += 1
            ground.append(y)
        # Keep a ground encounter's headroom under the slab's deepest point.
        if min(ground) - top < max(12, depth + 8):
            continue
        side = rng.choice((-1, 1))
        end = x0 - 1 if side < 0 else x0 + width
        floor = ground[0] if side < 0 else ground[-1]

        def wall_distance(y: int) -> int | None:
            for distance in range(1, 9):
                if rock(end + side * distance, y):
                    return distance
            return None

        reach = [wall_distance(y) for y in range(top, floor)]
        if None in reach or not 3 <= reach[0] <= 6 or max(reach) - min(reach) > 3:
            continue
        slab(canvas, rng, x0, top, width, depth)
        placed += 1


def compose(connectors: str, rng: random.Random, family: str) -> Image.Image | None:
    """One layout, or None when the chambers do not join into one cave that is
    open enough and has three broad floors."""
    planner = Planner(rng, family, connectors)
    if not planner.ports():
        return None
    if family == "grand" and not grand_hall(planner):
        return None
    if not planner.connect():
        return None
    planner.fill()
    if planner.broad_count() < 3 or planner.open_share() < SIZES[family][2][0] - 0.06:
        return None
    planner.plateaus()
    canvas = CaveCanvas()
    canvas.carve(planner.plan)
    canvas.warp(rng)
    canvas.ports(connectors)
    floating_slabs(canvas, rng, rng.randint(1, 3) if family == "grand" else rng.randint(0, 2))
    canvas.cut_thin_necks()
    canvas.tidy()
    canvas.fillets()
    return canvas.rgba()


def build(connectors: str, seed: int, family: str) -> Image.Image:
    """The first layout from `seed` that passes validation; reproducible per seed."""
    if family not in FAMILIES:
        raise ValueError(f"unknown large-room family: {family}")
    errors = ("chambers broke into a port's climbing rock",)
    for attempt in range(MAX_ATTEMPTS):
        image = compose(connectors, random.Random(chunks.mixed_seed(seed, attempt)), family)
        if image is None:
            continue
        report = validate(image, connectors)
        if report.valid:
            return image
        errors = report.errors
    raise RuntimeError(f"{connectors} {family}: no valid layout in {MAX_ATTEMPTS} attempts; last: {'; '.join(errors)}")


def validate(image: Image.Image, connectors: str) -> chunks.ValidationReport:
    errors = []
    if image.size != (SIZE, SIZE):
        return chunks.ValidationReport((f"expected {SIZE}x{SIZE} pixels",), 0, 0)
    rgba = list(image.convert("RGBA").getdata())
    if set(rgba) - {(0, 0, 0, 0), (0, 0, 0, 255)}:
        errors.append("large room palette must be transparent air or opaque black rock")
    solid = [bytearray(rgba[y * SIZE + x][3] >= 128 for x in range(SIZE)) for y in range(SIZE)]
    for direction in "UDLR":
        for position in range(SIZE):
            x, y = {"U": (position, 0), "D": (position, SIZE - 1),
                    "L": (0, position), "R": (SIZE - 1, position)}[direction]
            expected_air = direction in connectors and PORT_START <= position < PORT_END
            if bool(solid[y][x]) == expected_air:
                errors.append(f"{direction} edge differs from its middle-only twelve-tile port")
                break
    for y in range(0, SIZE, 2):
        for x in range(0, SIZE, 2):
            # Each two-tile block is uniform, apart from a softened corner.
            if sum(solid[yy][xx] for yy in (y, y + 1) for xx in (x, x + 1)) in (1, 2):
                errors.append("rock or air is thinner than the two-pixel authoring grid")
                break
    air = {(x, y) for y in range(SIZE) for x in range(SIZE) if not solid[y][x]}
    reached = {next(iter(air))} if air else set()
    queue = deque(reached)
    while queue:
        x, y = queue.popleft()
        for dx, dy in chunks.CARDINAL_DIRECTIONS:
            cell = (x + dx, y + dy)
            if cell in air and cell not in reached:
                reached.add(cell)
                queue.append(cell)
    if reached != air:
        errors.append(f"{len(air - reached)} air cells are disconnected")
    # A cell is rock when at least three of its tiles are: softened corners keep their cell.
    coarse = Image.new("L", (GRID, GRID), 0)
    coarse.putdata([255 if solid[2 * y][2 * x] + solid[2 * y][2 * x + 1] + solid[2 * y + 1][2 * x]
                    + solid[2 * y + 1][2 * x + 1] >= 3 else 0 for y in range(GRID) for x in range(GRID)])
    if diagonal_contacts(coarse):
        errors.append("rock masses touch only at a corner")
    if any(len(group) < MIN_ROCK_AREA for group in components(coarse, 255)):
        errors.append("isolated rock fragments are smaller than the minimum rock mass")
    labels = [bytearray(SIZE) for _ in range(SIZE)]
    profile = chunks.MovementProfile()
    platforms = chunks.extract_platforms(solid, labels, profile)
    if encounter_floor_count(solid, platforms) < 3:
        errors.append("fewer than three broad, clear floors for ground encounters")
    graph, faces = climbing_graph(solid, coarse, platforms, profile)
    return_graph = [set() for _ in graph]
    for source, targets in enumerate(graph):
        for target in targets:
            return_graph[target].add(source)
    exits = {}
    for direction in connectors:
        nodes = {index for index, p in enumerate(platforms) if (
            direction == "L" and p.x0 == 0 and PORT_START <= p.y < PORT_END
            or direction == "R" and p.x1 == SIZE - 1 and PORT_START <= p.y < PORT_END
            or direction == "U" and p.y <= 15 and p.x1 >= PORT_START and p.x0 < PORT_END
            or direction == "D" and p.y >= SIZE - profile.max_rise - 1
            and p.x1 >= PORT_START - profile.max_horizontal and p.x0 < PORT_END + profile.max_horizontal)}
        # A wall rising to the upward port, or reaching the downward one, is an exit too.
        nodes |= {len(platforms) + index for index, cells in enumerate(faces) if any(
            direction == "U" and y <= 15
            and PORT_START - profile.max_horizontal <= x < PORT_END + profile.max_horizontal
            or direction == "D" and y >= SIZE - profile.max_rise - 1
            and PORT_START - profile.max_horizontal <= x < PORT_END + profile.max_horizontal
            for x, y in cells)}
        exits[direction] = nodes
    # Standing spots narrower than three tiles are texture on a wavering wall, not places.
    every_platform = {index for index, platform in enumerate(platforms) if platform.x1 - platform.x0 >= 2}
    for direction, starts in exits.items():
        visited = chunks.reachable_nodes(graph, starts)
        if not starts or any(not visited & targets for targets in exits.values()):
            errors.append(f"movement cannot connect every port from {direction}")
        missing = every_platform - visited
        if missing:
            errors.append(f"{len(missing)} platforms are unreachable from {direction}")
        stranded = every_platform - chunks.reachable_nodes(return_graph, starts)
        if stranded:
            errors.append(f"{len(stranded)} platforms have no return route to {direction}")
    return chunks.ValidationReport(tuple(dict.fromkeys(errors)), len(platforms), len(air) / (SIZE * SIZE))


def wall_faces(coarse: Image.Image) -> list[list[tuple[int, int]]]:
    """Climbable wall faces, as tile positions of the air beside each wall.

    A face is a vertical run of rock with standing room next to it; it may drift a
    cell per row, so gently wavering walls still count. Jumping repeatedly against a
    wall climbs it, so a player can be anywhere along a face.
    """
    pixels = coarse.load()
    rock = lambda x, y: not (0 <= x < GRID and 0 <= y < GRID) or pixels[x, y] >= 128
    faces: list[list[tuple[int, int]]] = []
    for side in (-1, 1):
        open_faces: dict[int, int] = {}
        for y in range(1, GRID):
            current: dict[int, int] = {}
            for x in range(GRID):
                if rock(x, y) or rock(x, y - 1) or not (rock(x + side, y) and rock(x + side, y - 1)):
                    continue
                index = next((open_faces[x + d] for d in (0, -1, 1) if x + d in open_faces), None)
                if index is None:
                    index = len(faces)
                    faces.append([])
                faces[index].append((x * 2, y * 2 + 1))
                current[x] = index
            open_faces = current
    return faces


def climbing_graph(solid: list[bytearray], coarse: Image.Image, platforms: list[chunks.Platform],
                   profile: chunks.MovementProfile) -> tuple[list[set[int]], list[list[tuple[int, int]]]]:
    """Platforms and wall faces as one graph: jumps between platforms, plus wall climbs.

    A platform reaches a face it can jump to or drop beside; a face reaches every
    platform within a jump or a fall of any point along it.
    """
    graph = chunks.platform_graph(solid, platforms, profile)
    faces = wall_faces(coarse)
    graph = graph + [set() for _ in faces]

    def near(x: int, platform: chunks.Platform) -> bool:
        return platform.x0 - profile.max_horizontal <= x <= platform.x1 + profile.max_horizontal

    rows = []
    for cells in faces:
        by_row: dict[int, list[int]] = {}
        for x, y in cells:
            by_row.setdefault(y, []).append(x)
        rows.append(by_row)
    spans = [(min(x for x, _ in c), max(x for x, _ in c), min(y for _, y in c), max(y for _, y in c)) for c in faces]
    for a, cells in enumerate(faces):
        ax0, ax1, ay0, ay1 = spans[a]
        for b in range(len(faces)):
            bx0, bx1, by0, by1 = spans[b]
            if a == b or bx0 > ax1 + profile.max_horizontal or ax0 > bx1 + profile.max_horizontal \
                    or by1 < ay0 - profile.max_rise or by0 > ay1 + profile.max_drop:
                continue
            # Jump from anywhere along one wall to a point on another within reach.
            if any(abs(x - bx) <= profile.max_horizontal
                   for x, y in cells for row in range(y - profile.max_rise, y + profile.max_drop + 1)
                   for bx in rows[b].get(row, ())):
                graph[len(platforms) + a].add(len(platforms) + b)
    for face_index, cells in enumerate(faces):
        node = len(platforms) + face_index
        for platform_index, platform in enumerate(platforms):
            if any(near(x, platform) and platform.y - profile.max_rise <= y <= platform.y + profile.max_drop
                   for x, y in cells):
                graph[platform_index].add(node)
            if any(near(x, platform) and y - profile.max_rise <= platform.y <= y + profile.max_drop
                   for x, y in cells):
                graph[node].add(platform_index)
    return graph, faces


def encounter_floor_count(solid: list[bytearray], platforms: list[chunks.Platform]) -> int:
    count = 0
    for platform in platforms:
        run = 0
        for x in range(platform.x0, platform.x1 + 1):
            clear = platform.y >= ENCOUNTER_HEIGHT - 1 and all(
                not solid[platform.y - dy][x] for dy in range(ENCOUNTER_HEIGHT))
            run = run + 1 if clear else 0
            if run >= ENCOUNTER_WIDTH:
                count += 1
                break
    return count


def validate_file(path: Path) -> chunks.ValidationReport:
    with Image.open(path) as image:
        return validate(image, chunks.connectors_from_filename(path))


def generate(args) -> int:
    if args.prefix != "generated_chunk" and args.prefix != "large_chunk_3x3":
        raise ValueError("3x3 chunks use the large_chunk_3x3 filename prefix recognized by the runtime")
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=True)
    paths = []
    count = args.count if args.count is not None else len(args.connectors)
    for offset in range(count):
        connectors = chunks.canonical_connectors(args.connectors[offset % len(args.connectors)])
        family = FAMILIES[(offset // len(args.connectors)) % len(FAMILIES)]
        seed = args.seed + offset
        image = build(connectors, seed, family)
        report = validate(image, connectors)
        if not report.valid:
            raise RuntimeError(f"{connectors} {family}: {'; '.join(report.errors)}")
        path = output / f"large_chunk_3x3_{args.start_index + offset:04d}_{connectors}.png"
        if path.exists() and not args.overwrite:
            raise FileExistsError(f"refusing to overwrite {path}; pass --overwrite")
        metadata = PngImagePlugin.PngInfo()
        metadata.add_text("seed", str(seed))
        metadata.add_text("family", family)
        metadata.add_text("footprint", "3")
        image.save(path, pnginfo=metadata)
        paths.append(path)
        print(f"wrote {path.name}: {family}, {report.air_fraction:.1%} air, {report.platform_count} reachable platforms")
    if args.preview:
        chunks.create_preview(paths, args.preview.resolve(), scale=3)
    return 0
