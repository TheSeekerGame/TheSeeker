#!/usr/bin/env python3
"""Author the demo's connected sixteen-row cave map and validate large-room boundaries."""
from collections import deque
from pathlib import Path

from PIL import Image

ROOT = Path(__file__).resolve().parents[1]
ROWS = (
    (5,), (5,), (4, 5, 6), tuple(range(1, 9)), (1, 4, 5, 6),
    (0, 1, 2, 3, 5), (0, 3, 5, 6), (0, 2, 3, 4, 6), tuple(range(9)),
    (2, 3, 4, 8), (3, 8), (3, 7, 8, 9), tuple(range(3, 12)),
    (7, 8, 9), (8,), (8,),
)
ROOM_ORIGINS = ((4, 2), (2, 7), (7, 11))
GEYSERS = {(3, 6), (5, 8), (8, 10), (3, 12)}


def main():
    occupied = {(x, y) for y, columns in enumerate(ROWS) for x in columns}
    reached = {(5, 0)}
    queue = deque(reached)
    while queue:
        x, y = queue.popleft()
        for dx, dy in ((1, 0), (-1, 0), (0, 1), (0, -1)):
            cell = (x + dx, y + dy)
            if cell in occupied and cell not in reached:
                reached.add(cell)
                queue.append(cell)
    if reached != occupied:
        raise ValueError("map has unreachable occupied cells")
    for x, y in ROOM_ORIGINS:
        if any((x + dx, y + dy) not in occupied for dx in range(3) for dy in range(3)):
            raise ValueError(f"incomplete 3x3 block at {(x, y)}")
        corners = {(x + dx, y + dy) for dx, dy in ((0, -1), (2, -1), (0, 3), (2, 3),
                                                       (-1, 0), (-1, 2), (3, 0), (3, 2))}
        if corners & occupied:
            raise ValueError(f"corner transition at {(x, y)}")
    image = Image.new("RGBA", (12, len(ROWS)))
    for cell in occupied:
        color = (69, 255, 0, 255) if cell in GEYSERS else (0, 0, 0, 255)
        image.putpixel(cell, color)
    image.putpixel((5, 0), (255, 255, 255, 255))
    destination = ROOT / "assets/levels/maps/map1.png"
    image.save(destination)
    print(f"wrote {destination.name}: {len(occupied)} connected cells, {len(ROWS)} depth rows, {len(ROOM_ORIGINS)} large rooms")


if __name__ == "__main__":
    main()
