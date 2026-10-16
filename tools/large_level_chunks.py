"""Carve connected cave chambers and wall-supported climbs in a 3x3 footprint.

Level encounter floors sit inside thick rock masses. Shape variation belongs to
ceilings and room outlines; changes in elevation use dedicated edge passages.
"""
from __future__ import annotations

from collections import deque
from dataclasses import dataclass
import math
import random
from pathlib import Path

from PIL import Image, ImageDraw, PngImagePlugin

import generate_level_chunks as chunks

SIZE = chunks.IMAGE_SIZE * 3
GRID = SIZE // 2
PORT_START = chunks.IMAGE_SIZE + chunks.FINAL_PORT_START
PORT_END = chunks.IMAGE_SIZE + chunks.FINAL_PORT_END
FAMILIES = ("winding", "braided", "caverns")
MIN_ROCK_AREA = 12
ENCOUNTER_WIDTH = 48
ENCOUNTER_HEIGHT = 12


@dataclass(frozen=True)
class Chamber:
    left: int
    right: int
    floor: int
    height: int


@dataclass(frozen=True)
class Climb:
    x: int
    top: int
    bottom: int
    rooms_on_left: bool

    def landings(self) -> list[int]:
        # An even number of rises puts both room mouths on the same bank.
        steps = max(2, math.ceil((self.bottom - self.top) / 5))
        steps += steps % 2
        return [round(self.top + (self.bottom - self.top) * i / steps) for i in range(steps)]


class CaveCanvas:
    def __init__(self) -> None:
        self.image = Image.new("L", (GRID, GRID), 255)
        self.draw = ImageDraw.Draw(self.image)

    def chamber(self, room: Chamber, rng: random.Random) -> None:
        width = room.right - room.left
        roof = smooth_profile(width, rng, 3, 7)
        for offset in range(width):
            edge = min(offset, width - 1 - offset)
            cap = max(0, room.height - 4 - edge * 2)
            top = min(room.floor - 4, room.floor - room.height + cap + roof[offset])
            self.draw.line((room.left + offset, top, room.left + offset, room.floor - 1), fill=0)

    def hall(self, left: int, right: int, floor: int) -> None:
        # Low mouths meet the base of a climb without cutting away its banks.
        self.draw.rectangle((min(left, right), floor - 4, max(left, right), floor - 1), fill=0)

    def passage(self, climb: Climb) -> None:
        self.draw.rectangle((climb.x - 7, max(0, climb.top - 4),
                             climb.x + 6, climb.bottom - 1), fill=0)

    def banks(self, climb: Climb) -> None:
        for index, y in enumerate(climb.landings()):
            left = climb.rooms_on_left != bool(index % 2)
            self.bank(climb.x, y, left)

    def bank(self, x: int, y: int, left: bool) -> None:
        # Taper the underside into the wall, leaving clearance over the next
        # landing. These are short rock lips, not suspended platforms.
        for row in range(3):
            a = x - 8 if left else x + 2 + row * 2
            b = x - 2 - row * 2 if left else x + 7
            if a <= b:
                self.draw.line((a, y + row, b, y + row), fill=255)

    def ports(self, connectors: str) -> None:
        self.draw.rectangle((0, 0, GRID - 1, GRID - 1), outline=255, width=2)
        lo, hi = PORT_START // 2, PORT_END // 2 - 1
        for direction in connectors:
            rect = {"U": (lo, 0, hi, 3), "D": (lo, GRID - 4, hi, GRID - 1),
                    "L": (0, lo, 3, hi), "R": (GRID - 4, lo, GRID - 1, hi)}[direction]
            self.draw.rectangle(rect, fill=0)

    def tidy(self) -> None:
        pixels = self.image.load()
        # Erode the least-supported corner of a diagonal contact. Repeat since
        # removing one corner can expose another at the tip of the same spur.
        while contacts := diagonal_contacts(self.image):
            for a, b in contacts:
                def support(point):
                    x, y = point
                    return sum(pixels[x + dx, y + dy] == 255
                               for dx, dy in chunks.CARDINAL_DIRECTIONS
                               if 0 <= x + dx < GRID and 0 <= y + dy < GRID)
                pixels[min((a, b), key=support)] = 0
        for group in components(self.image, 255):
            if len(group) < MIN_ROCK_AREA:
                for point in group:
                    pixels[point] = 0
        air_groups = components(self.image, 0)
        for group in sorted(air_groups, key=len, reverse=True)[1:]:
            for point in group:
                pixels[point] = 255

    def rgba(self) -> Image.Image:
        result = Image.new("RGBA", (SIZE, SIZE), (0, 0, 0, 0))
        result.putalpha(self.image.resize((SIZE, SIZE), Image.Resampling.NEAREST))
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


def smooth_profile(width: int, rng: random.Random, amplitude: int, spacing: int) -> list[int]:
    anchors = [rng.randint(-amplitude, amplitude) for _ in range(width // spacing + 2)]
    return [round(anchors[x // spacing] + (anchors[x // spacing + 1] - anchors[x // spacing])
                  * (x % spacing) / spacing) for x in range(width)]


def chamber_plan(rng: random.Random, family: str, first_climb_left: bool) -> list[Chamber]:
    floors = [rng.randint(18, 23), rng.randint(44, 49), 75,
              rng.randint(101, 108), rng.randint(132, 137)]
    if family == "caverns":
        floors = [rng.randint(21, 25), 75, rng.randint(128, 135)]
    elif family == "braided":
        floors = [rng.randint(22, 27), 75, rng.randint(103, 110), rng.randint(132, 137)]
    rooms = []
    for row, floor in enumerate(floors):
        width = rng.randint(46, 64)
        height = rng.randint(12, 18)
        if family == "caverns" and row == 1:
            width, height = rng.randint(82, 94), rng.randint(24, 30)
        left = max(23, rng.randint(62, 82) - width // 2)
        right = min(GRID - 23, left + width)
        if row in (0, len(floors) - 1):
            # Keep the vertical port beside the chamber, outside its clear floor.
            climb_left = first_climb_left != bool(max(0, row - 1) % 2)
            left, right = (rng.randint(23, 29), 65) if climb_left else (79, rng.randint(115, 121))
            height = min(height, floor - 5)
        rooms.append(Chamber(left, right, floor, height))
    return rooms


def build(connectors: str, seed: int, family: str) -> Image.Image:
    if family not in FAMILIES:
        raise ValueError(f"unknown large-room family: {family}")
    rng = random.Random(seed)
    canvas = CaveCanvas()
    first_left = bool(rng.randrange(2))
    rooms = chamber_plan(rng, family, first_left)
    for room in rooms:
        canvas.chamber(room, rng)
    climbs = []
    for index, (upper, lower) in enumerate(zip(rooms, rooms[1:])):
        sides = [first_left != bool(index % 2)]
        if family == "braided" and index == 1:
            sides.append(not sides[0])
        for left in sides:
            bounds = rooms if family == "braided" else (upper, lower)
            x = min(r.left for r in bounds) - 7 if left else max(r.right for r in bounds) + 7
            for room in (upper, lower):
                canvas.hall(x, room.left + 3 if left else room.right - 3, room.floor)
            climbs.append(Climb(x, upper.floor, lower.floor, not left))
    if "U" in connectors:
        canvas.hall(GRID // 2, rooms[0].right - 3 if first_left else rooms[0].left + 3, rooms[0].floor)
        climbs.append(Climb(GRID // 2, 5, rooms[0].floor, first_left))
    bottom_climb = None
    if "D" in connectors:
        left = rooms[-1].right < GRID // 2
        canvas.hall(GRID // 2, rooms[-1].right - 3 if left else rooms[-1].left + 3, rooms[-1].floor)
        bottom_climb = Climb(GRID // 2, rooms[-1].floor, GRID - 2, left)
        climbs.append(bottom_climb)
    middle = next(room for room in rooms if room.floor == PORT_END // 2)
    if "L" in connectors:
        canvas.hall(0, middle.left + 3, middle.floor)
    if "R" in connectors:
        canvas.hall(middle.right - 3, GRID - 1, middle.floor)
    # Carve the complete route before adding banks, so neighboring passages
    # cannot erase each other's landing surfaces.
    for climb in climbs:
        canvas.passage(climb)
    for climb in climbs:
        canvas.banks(climb)
        if climb.top <= middle.floor <= climb.bottom:
            # A horizontal route crossing a shaft has a landing on each bank.
            canvas.bank(climb.x, middle.floor, True)
            canvas.bank(climb.x, middle.floor, False)
    if bottom_climb is not None:
        # Join the last bank to the boundary, closing the unusable recess under
        # its lip while leaving the centered downward opening unobstructed.
        last_left = not bottom_climb.rooms_on_left
        a, b = (GRID // 2 - 7, PORT_START // 2 - 1) if last_left else (PORT_END // 2, GRID // 2 + 6)
        canvas.draw.rectangle((a, bottom_climb.landings()[-1], b, GRID - 1), fill=255)
    canvas.ports(connectors)
    canvas.tidy()
    return canvas.rgba()


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
            if len({solid[yy][xx] for yy in (y, y + 1) for xx in (x, x + 1)}) != 1:
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
    coarse = image.convert("RGBA").getchannel("A").resize((GRID, GRID), Image.Resampling.NEAREST)
    if diagonal_contacts(coarse):
        errors.append("rock masses touch only at a corner")
    if any(len(group) < MIN_ROCK_AREA for group in components(coarse, 255)):
        errors.append("isolated rock fragments are smaller than the minimum rock mass")
    labels = [bytearray(SIZE) for _ in range(SIZE)]
    profile = chunks.MovementProfile()
    platforms = chunks.extract_platforms(solid, labels, profile)
    if encounter_floor_count(solid, platforms) < 3:
        errors.append("fewer than three broad, clear floors for ground encounters")
    graph = chunks.platform_graph(solid, platforms, profile)
    return_graph = [set() for _ in platforms]
    for source, targets in enumerate(graph):
        for target in targets:
            return_graph[target].add(source)
    exits = {}
    for direction in connectors:
        exits[direction] = {index for index, p in enumerate(platforms) if (
            direction == "L" and p.x0 == 0 and PORT_START <= p.y < PORT_END
            or direction == "R" and p.x1 == SIZE - 1 and PORT_START <= p.y < PORT_END
            or direction == "U" and p.y <= 15 and p.x1 >= PORT_START and p.x0 < PORT_END
            or direction == "D" and p.y >= SIZE - profile.max_rise - 1
            and p.x1 >= PORT_START - profile.max_horizontal and p.x0 < PORT_END + profile.max_horizontal)}
    for direction, starts in exits.items():
        visited = chunks.reachable_nodes(graph, starts)
        if not starts or any(not visited & targets for targets in exits.values()):
            errors.append(f"base movement cannot connect every port from {direction}")
        if len(visited) != len(platforms):
            errors.append(f"{len(platforms) - len(visited)} platforms are unreachable from {direction}")
        can_return = chunks.reachable_nodes(return_graph, starts)
        if len(can_return) != len(platforms):
            errors.append(f"{len(platforms) - len(can_return)} platforms have no return route to {direction}")
    return chunks.ValidationReport(tuple(dict.fromkeys(errors)), len(platforms), len(air) / (SIZE * SIZE))


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
