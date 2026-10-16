#!/usr/bin/env python3
"""Downsample the authored damage-spark strips into game-ready pixel art."""

from __future__ import annotations

import argparse
from pathlib import Path

from PIL import Image


FRAME_COUNT = 3
SOURCE_FRAME_SIZE = 48
ATLAS_FRAME_SIZE = 64
DIRECTION_COUNT = 16
ROOT = Path(__file__).resolve().parents[1]
SOURCE_DIR = ROOT / "tools" / "source_art" / "damage_sparks"
OUTPUT_DIR = ROOT / "assets" / "animations" / "player" / "sparks"

SPARKS = {
    "SwordHitSpark": {
        "source": "SwordHitSparkSource.png",
        "key": "anim.player.SwordHitSpark",
        "audio": [("SwordHit", "audio.game.PlayerGetsDamaged")],
    },
    "HammerHitSpark": {
        "source": "HammerHitSparkSource.png",
        "key": "anim.player.HammerHitSpark",
        "audio": [("HammerHit", "audio.game.HammerImpact")],
    },
    "ProjectileHitSpark": {
        "source": "ProjectileHitSparkSource.png",
        "key": "anim.player.ProjectileHitSpark",
        "audio": [
            ("BowHit", "audio.game.PlayerGetsDamaged"),
            ("BurningDashHit", "audio.game.PlayerGetsDamaged"),
        ],
    },
}


def quantize_pixel(r: int, g: int, b: int, a: int) -> tuple[int, int, int, int]:
    if a < 48:
        return (0, 0, 0, 0)

    if a < 104:
        alpha = 85
    elif a < 184:
        alpha = 170
    else:
        alpha = 255

    luminance = (54 * r + 183 * g + 19 * b) // 256
    if luminance < 176:
        color = (178, 193, 207)
    elif luminance < 232:
        color = (224, 234, 243)
    else:
        color = (255, 255, 255)
    return (*color, alpha)


def pixelate_frame(frame: Image.Image) -> Image.Image:
    reduced = frame.resize(
        (SOURCE_FRAME_SIZE, SOURCE_FRAME_SIZE), Image.Resampling.LANCZOS
    ).convert("RGBA")
    reduced.putdata([quantize_pixel(*pixel) for pixel in reduced.getdata()])
    padded = Image.new("RGBA", (ATLAS_FRAME_SIZE, ATLAS_FRAME_SIZE))
    inset = (ATLAS_FRAME_SIZE - SOURCE_FRAME_SIZE) // 2
    padded.alpha_composite(reduced, (inset, inset))
    return padded


def generate_sheet(source_path: Path, output_path: Path) -> None:
    source = Image.open(source_path).convert("RGBA")
    if source.width % FRAME_COUNT != 0:
        raise ValueError(
            f"{source_path} width {source.width} is not divisible by {FRAME_COUNT}"
        )

    source_frame_size = source.width // FRAME_COUNT
    if source.height != source_frame_size:
        raise ValueError(
            f"{source_path} must contain {FRAME_COUNT} square cells; "
            f"got {source.size}"
        )

    sheet = Image.new(
        "RGBA",
        (
            FRAME_COUNT * ATLAS_FRAME_SIZE,
            DIRECTION_COUNT * ATLAS_FRAME_SIZE,
        ),
    )
    for frame_index in range(FRAME_COUNT):
        left = frame_index * source_frame_size
        frame = source.crop(
            (left, 0, left + source_frame_size, source_frame_size)
        )
        pixel_frame = pixelate_frame(frame)
        if pixel_frame.getbbox() is None:
            raise ValueError(
                f"{source_path} frame {frame_index + 1} became empty"
            )
        for direction_index in range(DIRECTION_COUNT):
            angle = direction_index * 360.0 / DIRECTION_COUNT
            directional_frame = pixel_frame.rotate(
                angle,
                resample=Image.Resampling.NEAREST,
                expand=False,
            )
            sheet.alpha_composite(
                directional_frame,
                (
                    frame_index * ATLAS_FRAME_SIZE,
                    direction_index * ATLAS_FRAME_SIZE,
                ),
            )

    output_path.parent.mkdir(parents=True, exist_ok=True)
    sheet.save(output_path, optimize=True)


def generate_manifest(
    spark_name: str,
    animation_key: str,
    audio_slots: list[tuple[str, str]],
    output_path: Path,
) -> None:
    sheet_name = (
        f"{spark_name}Sheet__r{DIRECTION_COUNT}c{FRAME_COUNT}"
        f"__{ATLAS_FRAME_SIZE}x{ATLAS_FRAME_SIZE}.png"
    )
    lines = [
        "[asset]",
        f'key = "{animation_key}"',
        f'image = "animations/player/sparks/{sheet_name}"',
        "",
        "[settings]",
        'time_base = "Relative"',
        "ticks_per_frame = 4",
        "frame_min = 1",
        f"frame_max = {FRAME_COUNT * DIRECTION_COUNT}",
        "frame_start = 1",
    ]

    for slot_name, asset_key in audio_slots:
        lines.extend(
            [
                "",
                "[[script]]",
                f'run_on_slot_enable = "{slot_name}"',
                'action = "PlayAudio"',
                "volume = 1.0",
                f'asset_key = "{asset_key}"',
                'label = "DamageSparks"',
            ]
        )

    for direction_index in range(DIRECTION_COUNT):
        first_frame = direction_index * FRAME_COUNT + 1
        final_frame = first_frame + FRAME_COUNT - 1
        lines.extend(
            [
                "",
                "[[script]]",
                f'run_on_slot_enable = "SparkDirection{direction_index}"',
                'action = "SetFrameNow"',
                f"frame_index = {first_frame}",
                "",
                "[[script]]",
                f"run_at_frame = {final_frame}",
                'action = "SetFrameNext"',
                f"frame_index = {final_frame}",
            ]
        )

    output_path.write_text("\n".join(lines) + "\n", encoding="ascii")


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--output-dir",
        type=Path,
        default=OUTPUT_DIR,
        help="Directory for generated sprite sheets",
    )
    args = parser.parse_args()

    for spark_name, config in SPARKS.items():
        sheet_output = args.output_dir / (
            f"{spark_name}Sheet__r{DIRECTION_COUNT}c{FRAME_COUNT}"
            f"__{ATLAS_FRAME_SIZE}x{ATLAS_FRAME_SIZE}.png"
        )
        manifest_output = args.output_dir / f"{spark_name}.anim.toml"
        generate_sheet(SOURCE_DIR / config["source"], sheet_output)
        generate_manifest(
            spark_name,
            config["key"],
            config["audio"],
            manifest_output,
        )
        print(sheet_output.relative_to(ROOT))
        print(manifest_output.relative_to(ROOT))


if __name__ == "__main__":
    main()
