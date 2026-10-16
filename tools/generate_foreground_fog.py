#!/usr/bin/env python3
"""Generate the deterministic, spatially tiled foreground fog atlas."""

from __future__ import annotations

import argparse
import math
from pathlib import Path

from PIL import Image, ImageChops, ImageFilter, ImageStat


FRAME_SIZE = 256
GUTTER = 2
CELL_SIZE = FRAME_SIZE + GUTTER * 2
REAR_FRAME_SIZE = 128
REAR_CELL_SIZE = REAR_FRAME_SIZE + GUTTER * 2
ATLAS_COLUMNS = 12
ATLAS_ROWS = 8
FRAME_COUNT = ATLAS_COLUMNS * ATLAS_ROWS
SEED = 0x5E_31_9A_77


def hash_byte(x: int, y: int, layer: int) -> int:
    value = SEED
    value ^= x * 0x9E3779B1
    value ^= y * 0x85EBCA77
    value ^= layer * 0x27D4EB2F
    value &= 0xFFFFFFFF
    value ^= value >> 16
    value = (value * 0x7FEB352D) & 0xFFFFFFFF
    value ^= value >> 15
    value = (value * 0x846CA68B) & 0xFFFFFFFF
    value ^= value >> 16
    return value & 0xFF


def periodic_noise_image(
    cells_x: int,
    cells_y: int,
    layer: int,
) -> Image.Image:
    if FRAME_SIZE % cells_x != 0 or FRAME_SIZE % cells_y != 0:
        raise ValueError("noise cell counts must divide the frame size")

    border = 2
    control_width = cells_x + border * 2
    control_height = cells_y + border * 2
    control = Image.new("L", (control_width, control_height))
    control.putdata(
        [
            hash_byte(
                (x - border) % cells_x,
                (y - border) % cells_y,
                layer,
            )
            for y in range(control_height)
            for x in range(control_width)
        ]
    )

    pixels_per_cell_x = FRAME_SIZE // cells_x
    pixels_per_cell_y = FRAME_SIZE // cells_y
    expanded = control.resize(
        (
            control_width * pixels_per_cell_x,
            control_height * pixels_per_cell_y,
        ),
        Image.Resampling.BICUBIC,
    )
    offset_x = border * pixels_per_cell_x
    offset_y = border * pixels_per_cell_y
    return expanded.crop(
        (
            offset_x,
            offset_y,
            offset_x + FRAME_SIZE,
            offset_y + FRAME_SIZE,
        )
    )


def tile_periodic_image(image: Image.Image, copies: int = 3) -> Image.Image:
    tiled = Image.new("L", (FRAME_SIZE * copies, FRAME_SIZE * copies))
    for y in range(copies):
        for x in range(copies):
            tiled.paste(image, (x * FRAME_SIZE, y * FRAME_SIZE))
    return tiled


def periodic_gaussian_blur(image: Image.Image, radius: float) -> Image.Image:
    tiled = tile_periodic_image(image)
    blurred = tiled.filter(ImageFilter.GaussianBlur(radius=radius))
    return blurred.crop(
        (FRAME_SIZE, FRAME_SIZE, FRAME_SIZE * 2, FRAME_SIZE * 2)
    )


def periodic_max_filter(image: Image.Image, size: int) -> Image.Image:
    tiled = tile_periodic_image(image)
    filtered = tiled.filter(ImageFilter.MaxFilter(size=size))
    return filtered.crop(
        (FRAME_SIZE, FRAME_SIZE, FRAME_SIZE * 2, FRAME_SIZE * 2)
    )


def periodic_wave_warp(
    image: Image.Image,
    amplitude_x: float,
    amplitude_y: float,
    phase_x: float,
    phase_y: float,
) -> Image.Image:
    grid_size = 16
    tiled = tile_periodic_image(image)
    mesh = []

    def source_point(x: int, y: int) -> tuple[float, float]:
        x_phase = math.tau * y / FRAME_SIZE
        y_phase = math.tau * x / FRAME_SIZE
        offset_x = amplitude_x * (
            math.sin(x_phase + phase_x)
            + 0.32 * math.sin(x_phase * 2.0 - phase_x * 2.0)
        )
        offset_y = amplitude_y * (
            math.sin(y_phase + phase_y)
            + 0.24 * math.sin(y_phase * 3.0 + phase_y * 2.0)
        )
        return FRAME_SIZE + x + offset_x, FRAME_SIZE + y + offset_y

    for y in range(0, FRAME_SIZE, grid_size):
        for x in range(0, FRAME_SIZE, grid_size):
            right = min(x + grid_size, FRAME_SIZE)
            bottom = min(y + grid_size, FRAME_SIZE)
            mesh.append(
                (
                    (x, y, right, bottom),
                    (
                        *source_point(x, y),
                        *source_point(x, bottom),
                        *source_point(right, bottom),
                        *source_point(right, y),
                    ),
                )
            )

    return tiled.transform(
        (FRAME_SIZE, FRAME_SIZE),
        Image.Transform.MESH,
        mesh,
        resample=Image.Resampling.BICUBIC,
    )


def periodic_morph(
    image: Image.Image,
    frame: int,
    amplitude_x: float,
    amplitude_y: float,
    phase_offset: float,
    frequency: float = 1.0,
) -> Image.Image:
    phase = (frame % FRAME_COUNT) / FRAME_COUNT
    angle = math.tau * phase * frequency
    return periodic_wave_warp(
        image,
        amplitude_x,
        amplitude_y,
        angle + phase_offset,
        -angle + phase_offset * 0.73,
    )


def smoothstep_image(
    image: Image.Image,
    low: float,
    high: float,
) -> Image.Image:
    def remap(value: int) -> int:
        t = max(0.0, min(1.0, (value / 255.0 - low) / (high - low)))
        return round((t * t * (3.0 - 2.0 * t)) * 255.0)

    return image.point([remap(value) for value in range(256)])


def ridged_image(
    image: Image.Image,
    low: float,
    high: float,
) -> Image.Image:
    ridge = image.point(
        [round((1.0 - abs(value / 127.5 - 1.0)) * 255.0) for value in range(256)]
    )
    return smoothstep_image(ridge, low, high)


def scaled_image(image: Image.Image, scale: float) -> Image.Image:
    return image.point([round(value * scale) for value in range(256)])


def normalize_macro_density(
    image: Image.Image,
    radius: float,
    strength: float,
) -> Image.Image:
    envelope = periodic_gaussian_blur(image, radius)
    target = ImageStat.Stat(envelope).mean[0]
    minimum_envelope = max(target * 0.35, 1.0)
    normalized = Image.new("L", image.size)
    normalized.putdata(
        [
            round(
                value
                * max(
                    0.82,
                    min(
                        1.65,
                        1.0
                        + strength
                        * (target / max(local_density, minimum_envelope) - 1.0),
                    ),
                )
            )
            for value, local_density in zip(image.getdata(), envelope.getdata())
        ]
    )
    return normalized


def veil_image() -> Image.Image:
    veil = periodic_noise_image(2, 2, 0)
    return periodic_gaussian_blur(veil, 20.0)


def bank_images() -> tuple[Image.Image, Image.Image]:
    first = periodic_gaussian_blur(periodic_noise_image(4, 4, 1), 13.0)
    second = periodic_noise_image(4, 8, 2)
    second = periodic_wave_warp(second, 5.0, 12.0, 0.4, 2.1)
    return first, periodic_gaussian_blur(second, 11.0)


def wisp_images() -> tuple[Image.Image, Image.Image]:
    first_field = periodic_noise_image(4, 16, 3)
    first_body = ridged_image(first_field, 0.24, 0.84)
    first_body = periodic_max_filter(first_body, 7)
    first_core = ridged_image(first_field, 0.58, 0.95)
    first = Image.blend(first_body, first_core, 0.18)
    first = periodic_wave_warp(first, 10.0, 24.0, 1.1, 0.2)
    first = periodic_gaussian_blur(first, 4.5)

    ribbon_variation = periodic_noise_image(8, 8, 5)
    ribbon_variation = periodic_wave_warp(
        ribbon_variation,
        7.0,
        9.0,
        2.6,
        4.4,
    )
    ribbon_variation = periodic_gaussian_blur(ribbon_variation, 9.0)
    ribbon_variation = smoothstep_image(ribbon_variation, 0.22, 0.80)
    ribbon_variation = ribbon_variation.point(
        [round(90.0 + value / 255.0 * 165.0) for value in range(256)]
    )
    first = ImageChops.multiply(first, ribbon_variation)

    second = ridged_image(periodic_noise_image(16, 16, 4), 0.68, 0.97)
    second = periodic_wave_warp(second, 12.0, 16.0, 3.4, 4.2)
    second = periodic_gaussian_blur(second, 2.2)
    return first, second


def rear_source_images() -> tuple[
    Image.Image,
    Image.Image,
    Image.Image,
    Image.Image,
    Image.Image,
]:
    veil = periodic_gaussian_blur(periodic_noise_image(2, 2, 10), 20.0)
    first_bank = periodic_gaussian_blur(
        periodic_noise_image(4, 4, 11),
        13.0,
    )
    second_bank = periodic_noise_image(4, 8, 12)
    second_bank = periodic_wave_warp(second_bank, 5.0, 12.0, 2.3, 0.7)
    second_bank = periodic_gaussian_blur(second_bank, 11.0)

    first_wisp = ridged_image(
        periodic_noise_image(4, 16, 13),
        0.38,
        0.88,
    )
    first_wisp = periodic_wave_warp(first_wisp, 4.0, 18.0, 3.2, 1.6)
    first_wisp = periodic_gaussian_blur(first_wisp, 5.5)

    second_wisp = ridged_image(
        periodic_noise_image(8, 16, 14),
        0.38,
        0.88,
    )
    second_wisp = periodic_wave_warp(second_wisp, 6.0, 12.0, 0.5, 3.7)
    second_wisp = periodic_gaussian_blur(second_wisp, 4.0)
    return veil, first_bank, second_bank, first_wisp, second_wisp


def build_source_images() -> tuple[
    Image.Image,
    Image.Image,
    Image.Image,
    Image.Image,
    Image.Image,
]:
    first_bank, second_bank = bank_images()
    first_wisp, second_wisp = wisp_images()
    return veil_image(), first_bank, second_bank, first_wisp, second_wisp


def build_frame(
    source_images: tuple[
        Image.Image,
        Image.Image,
        Image.Image,
        Image.Image,
        Image.Image,
    ],
    frame: int,
) -> Image.Image:
    veil, first_bank, second_bank, first_wisp, second_wisp = source_images

    moved_veil = periodic_morph(veil, frame, 0.75, 0.75, 0.0)
    moved_first_bank = periodic_morph(first_bank, frame, 1.5, 2.0, 0.8)
    moved_second_bank = periodic_morph(second_bank, frame, 2.0, 2.5, 2.7)
    moved_first_wisp = periodic_morph(first_wisp, frame, 2.5, 3.5, 1.4)
    moved_second_wisp = periodic_morph(
        second_wisp,
        frame,
        2.0,
        3.0,
        4.1,
        frequency=2.0,
    )

    veil_density = moved_veil.point(
        [round(7.0 + value / 255.0 * 17.0) for value in range(256)]
    )
    banks = Image.blend(moved_first_bank, moved_second_bank, 0.38)
    bank_density = scaled_image(smoothstep_image(banks, 0.28, 0.82), 0.22)

    bank_gate = smoothstep_image(banks, 0.24, 0.76).point(
        [round(102.0 + value / 255.0 * 153.0) for value in range(256)]
    )
    first_wisp_density = ImageChops.multiply(moved_first_wisp, bank_gate)
    second_wisp_density = ImageChops.multiply(moved_second_wisp, bank_gate)
    first_wisp_density = scaled_image(first_wisp_density, 0.68)
    second_wisp_density = scaled_image(second_wisp_density, 0.16)

    wisps = ImageChops.add(first_wisp_density, second_wisp_density)
    wisps = normalize_macro_density(wisps, 18.0, 0.55)
    combined = ImageChops.add(veil_density, bank_density)
    combined = ImageChops.add(combined, wisps)
    return periodic_gaussian_blur(combined, 0.8)


def resize_periodic_image(image: Image.Image, size: int) -> Image.Image:
    tiled = tile_periodic_image(image)
    resized = tiled.resize((size * 3, size * 3), Image.Resampling.LANCZOS)
    return resized.crop((size, size, size * 2, size * 2))


def build_rear_frame(
    source_images: tuple[
        Image.Image,
        Image.Image,
        Image.Image,
        Image.Image,
        Image.Image,
    ],
    frame: int,
) -> Image.Image:
    veil, first_bank, second_bank, first_wisp, second_wisp = source_images

    moved_veil = periodic_morph(veil, frame, 0.5, 0.5, 0.4)
    moved_first_bank = periodic_morph(first_bank, frame, 1.0, 1.25, 2.2)
    moved_second_bank = periodic_morph(second_bank, frame, 1.25, 1.5, 4.3)
    moved_first_wisp = periodic_morph(first_wisp, frame, 1.5, 2.0, 0.9)
    moved_second_wisp = periodic_morph(
        second_wisp,
        frame,
        1.25,
        1.75,
        3.1,
        frequency=2.0,
    )

    veil_density = moved_veil.point(
        [round(35.0 + value / 255.0 * 35.0) for value in range(256)]
    )
    banks = Image.blend(moved_first_bank, moved_second_bank, 0.38)
    bank_density = scaled_image(smoothstep_image(banks, 0.30, 0.78), 0.37)
    bank_gate = banks.point(
        [round(82.0 + value / 255.0 * 173.0) for value in range(256)]
    )
    first_wisp_density = scaled_image(
        ImageChops.multiply(moved_first_wisp, bank_gate),
        0.16,
    )
    second_wisp_density = scaled_image(
        ImageChops.multiply(moved_second_wisp, bank_gate),
        0.08,
    )

    combined = ImageChops.add(veil_density, bank_density)
    combined = ImageChops.add(combined, first_wisp_density)
    combined = ImageChops.add(combined, second_wisp_density)
    combined = periodic_gaussian_blur(combined, 1.25)
    return resize_periodic_image(combined, REAR_FRAME_SIZE)


def add_wrap_gutter(frame: Image.Image) -> Image.Image:
    if frame.width != frame.height:
        raise ValueError("fog frames must be square")
    frame_size = frame.width
    cell_size = frame_size + GUTTER * 2
    cell = Image.new("L", (cell_size, cell_size), 0)
    cell.paste(frame, (GUTTER, GUTTER))
    cell.paste(
        frame.crop((frame_size - GUTTER, 0, frame_size, frame_size)),
        (0, GUTTER),
    )
    cell.paste(
        frame.crop((0, 0, GUTTER, frame_size)),
        (GUTTER + frame_size, GUTTER),
    )
    cell.paste(
        cell.crop((0, frame_size, cell_size, frame_size + GUTTER)),
        (0, 0),
    )
    cell.paste(
        cell.crop((0, GUTTER, cell_size, GUTTER * 2)),
        (0, GUTTER + frame_size),
    )
    return cell


def verify_cell(cell: Image.Image) -> None:
    frame_size = cell.width - GUTTER * 2
    interior = cell.crop(
        (GUTTER, GUTTER, GUTTER + frame_size, GUTTER + frame_size)
    )
    expected = add_wrap_gutter(interior)
    if ImageChops.difference(cell, expected).getbbox() is not None:
        raise RuntimeError("fog frame gutter does not wrap exactly")


def generate_atlas() -> Image.Image:
    source_images = build_source_images()
    if ImageChops.difference(
        build_frame(source_images, 0),
        build_frame(source_images, FRAME_COUNT),
    ).getbbox() is not None:
        raise RuntimeError("fog animation does not loop exactly")

    atlas = Image.new(
        "L",
        (ATLAS_COLUMNS * CELL_SIZE, ATLAS_ROWS * CELL_SIZE),
        0,
    )
    for frame_index in range(FRAME_COUNT):
        cell = add_wrap_gutter(build_frame(source_images, frame_index))
        verify_cell(cell)
        atlas.paste(
            cell,
            (
                frame_index % ATLAS_COLUMNS * CELL_SIZE,
                frame_index // ATLAS_COLUMNS * CELL_SIZE,
            ),
        )
    return atlas


def generate_rear_atlas() -> Image.Image:
    source_images = rear_source_images()
    if ImageChops.difference(
        build_rear_frame(source_images, 0),
        build_rear_frame(source_images, FRAME_COUNT),
    ).getbbox() is not None:
        raise RuntimeError("rear fog animation does not loop exactly")

    atlas = Image.new(
        "L",
        (ATLAS_COLUMNS * REAR_CELL_SIZE, ATLAS_ROWS * REAR_CELL_SIZE),
        0,
    )
    for frame_index in range(FRAME_COUNT):
        cell = add_wrap_gutter(
            build_rear_frame(source_images, frame_index)
        )
        verify_cell(cell)
        atlas.paste(
            cell,
            (
                frame_index % ATLAS_COLUMNS * REAR_CELL_SIZE,
                frame_index // ATLAS_COLUMNS * REAR_CELL_SIZE,
            ),
        )
    return atlas


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--output",
        type=Path,
        default=Path("assets/fx/foreground_fog_atlas.png"),
    )
    parser.add_argument(
        "--rear-output",
        type=Path,
        default=Path("assets/fx/foreground_fog_rear_atlas.png"),
    )
    return parser.parse_args()


def main() -> None:
    args = parse_args()
    atlas = generate_atlas()
    rear_atlas = generate_rear_atlas()
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.rear_output.parent.mkdir(parents=True, exist_ok=True)
    atlas.save(args.output, format="PNG", optimize=True, compress_level=9)
    rear_atlas.save(
        args.rear_output,
        format="PNG",
        optimize=True,
        compress_level=9,
    )
    print(
        f"wrote {args.output} ({atlas.width}x{atlas.height}, "
        f"{FRAME_COUNT} frames)"
    )
    print(
        f"wrote {args.rear_output} ({rear_atlas.width}x{rear_atlas.height}, "
        f"{FRAME_COUNT} frames)"
    )


if __name__ == "__main__":
    main()
