#!/usr/bin/env python3
"""Generate the surface backdrop and landing-cinematic art.

Requires Pillow and NumPy. Output is deterministic. Colors come from the ice-biome
tilesets (assets/levels/tiles/ldtk_rules/MainTileset.png and BackgroundTileset.png) so
the open surface continues into the caves. Shading is smooth; distant layers get the same
kind of soft blur as the cave backgrounds.

Backdrop layers are 576 pixels wide, one spawn chunk, and use spawn-chunk pixel rows
(y downward from the chunk top). game/src/levelgen/surface_backdrop.rs places them; keep
LAYER_TOPS in sync with it.

    python3 tools/surface_art/generate.py
    python3 tools/surface_art/generate.py --preview /absolute/preview/dir
"""
from __future__ import annotations

import argparse
import math
from pathlib import Path

import numpy as np
from PIL import Image

ROOT = Path(__file__).resolve().parents[2]
BACKDROP_DIR = ROOT / "assets/levels/surface"
FX_DIR = ROOT / "assets/animations/intro"
SPAWN_CHUNK = ROOT / "assets/levels/chunks/player_spawn_D.png"
CHUNK_TILE_PX = 6
WIDTH = 576
# Horizon row behind the landscape; sky altitude is measured upward from it.
HORIZON_Y = 500
# The crater lies centered under the capsule; its rim sits on this row.
CRATER_CENTER_X = 291
CRATER_LIP_Y = 498
CRATER_HALF_WIDTH = 96

# Chunk rows covered by each layer image; the engine places image row 0 at this row.
LAYER_TOPS = {
    "Sky": -80,
    "Clouds": -440,
    "FarRange": 400,
    "MidHills": 440,
    "CraterRim": 470,
}
LAYER_BOTTOM = 576


def rgb(value: str) -> np.ndarray:
    value = value.lstrip("#")
    return np.array([int(value[i:i + 2], 16) for i in (0, 2, 4)], dtype=float) / 255


# Ice-biome ramps sampled from the tilesets.
TERRAIN = {name: rgb(value) for name, value in {
    "deep": "#181c20", "dark": "#20242a", "rock": "#353e50", "rock_lit": "#455369",
    "ledge": "#4f5d73", "edge": "#92a4af", "snow": "#edf0f1",
}.items()}
BACKGROUND = [rgb(value) for value in (
    "#5d666d", "#6c7983", "#83909c", "#8d9faa", "#a1b2bb", "#b0c3cd", "#cfdde4", "#d4e6ec", "#f5fafd",
)]

# Cold sky from a pale icy horizon to night, keyed by altitude above HORIZON_Y.
SKY_STOPS = [
    (-60, "#e4edf1"), (0, "#dae6ec"), (30, "#c9d8e0"), (70, "#afc1cc"), (120, "#91a3b1"),
    (185, "#73869a"), (265, "#58697f"), (355, "#415067"), (455, "#2e3a4f"), (565, "#20293a"),
    (690, "#161c29"), (820, "#10141e"),
]


def sky_color(altitude) -> np.ndarray:
    altitude = np.asarray(altitude, dtype=float)
    stops = np.array([stop for stop, _ in SKY_STOPS], dtype=float)
    colors = np.array([rgb(color) for _, color in SKY_STOPS])
    return np.stack([np.interp(altitude, stops, colors[:, channel]) for channel in range(3)], axis=-1)


def smoothstep(edge0, edge1, value):
    t = np.clip((np.asarray(value, dtype=float) - edge0) / (edge1 - edge0), 0.0, 1.0)
    return t * t * (3 - 2 * t)


def mix(a, b, t):
    """Blend colors `a` and `b`; a scalar-per-pixel `t` applies to every channel."""
    t = np.asarray(t, dtype=float)
    if t.ndim and t.shape[-1:] != (3,):
        t = t[..., None]
    return a + (b - a) * t


def value_noise_1d(length: int, wavelength: float, rng: np.random.Generator) -> np.ndarray:
    knots = rng.uniform(-1.0, 1.0, int(length / wavelength) + 3)
    t = np.arange(length) / wavelength
    i = np.floor(t).astype(int)
    f = t - i
    f = f * f * (3 - 2 * f)
    return knots[i] * (1 - f) + knots[i + 1] * f


def fbm_1d(length: int, wavelengths: list[float], rng: np.random.Generator) -> np.ndarray:
    total = np.zeros(length)
    amplitude = 1.0
    for wavelength in wavelengths:
        total += value_noise_1d(length, wavelength, rng) * amplitude
        amplitude *= 0.5
    return total


class Canvas:
    """Straight-alpha float RGBA image."""

    def __init__(self, width: int, height: int):
        self.rgb = np.zeros((height, width, 3))
        self.alpha = np.zeros((height, width))

    @property
    def shape(self):
        return self.alpha.shape

    def over(self, color, alpha) -> None:
        """Composite `color` with coverage `alpha` over the canvas."""
        alpha = np.clip(np.broadcast_to(np.asarray(alpha, dtype=float), self.alpha.shape), 0, 1)
        color = np.broadcast_to(color, self.rgb.shape)
        out_alpha = alpha + self.alpha * (1 - alpha)
        weight = np.divide(alpha, out_alpha, out=np.zeros_like(out_alpha), where=out_alpha > 0)
        self.rgb = self.rgb + (color - self.rgb) * weight[..., None]
        self.alpha = out_alpha

    def blurred(self, radius: float) -> "Canvas":
        """Gaussian blur in linear light with premultiplied alpha, as the runtime does."""
        if radius <= 0:
            return self
        linear = np.where(self.rgb <= 0.04045, self.rgb / 12.92, ((self.rgb + 0.055) / 1.055) ** 2.4)
        values = np.concatenate([linear * self.alpha[..., None], self.alpha[..., None]], axis=-1)
        support = math.ceil(radius * 1.5)
        weights = np.exp(-np.arange(-support, support + 1) ** 2 / (2 * (radius * 0.5) ** 2))
        weights /= weights.sum()
        for axis in (1, 0):
            padding = [(0, 0)] * 3
            padding[axis] = (support, support)
            padded = np.pad(values, padding, mode="edge")
            values = sum(weight * np.take(padded, np.arange(values.shape[axis]) + i, axis=axis)
                         for i, weight in enumerate(weights))
        result = Canvas(self.shape[1], self.shape[0])
        result.alpha = values[..., 3]
        linear = values[..., :3] / np.maximum(values[..., 3:4], 1e-4)
        result.rgb = np.where(linear <= 0.0031308, linear * 12.92, 1.055 * np.maximum(linear, 0) ** (1 / 2.4) - 0.055)
        return result

    def to_uint8(self) -> np.ndarray:
        out = np.concatenate([np.clip(self.rgb, 0, 1), np.clip(self.alpha, 0, 1)[..., None]], axis=-1)
        out = (out * 255 + 0.5).astype(np.uint8)
        out[out[..., 3] == 0] = 0
        return out


# ---------------------------------------------------------------------------
# Sky


def build_sky(rng: np.random.Generator) -> Canvas:
    top = LAYER_TOPS["Sky"]
    height = LAYER_BOTTOM - top
    rows = np.arange(height) + top
    canvas = Canvas(WIDTH, height)
    canvas.rgb[:] = sky_color(HORIZON_Y - rows)[:, None, :]
    canvas.alpha[:] = 1
    add_sky_veils(canvas, top, rng)
    add_stars(canvas, top, rng)
    return canvas


def add_sky_veils(canvas: Canvas, top: int, rng: np.random.Generator) -> None:
    """Thin, soft cirrus bands, brighter near the horizon."""
    height = canvas.shape[0]
    yy, xx = np.mgrid[0:height, 0:WIDTH]
    for _ in range(18):
        altitude = rng.uniform(20, 240)
        cy = HORIZON_Y - altitude - top
        cx = rng.uniform(0, WIDTH)
        length = rng.uniform(90, 260)
        thickness = rng.uniform(1.2, 3.2)
        dx = (xx - cx + WIDTH / 2) % WIDTH - WIDTH / 2
        band = np.exp(-(dx / (length / 2)) ** 4 - ((yy - cy) / thickness) ** 2)
        tint = sky_color(altitude - 40) if altitude < 110 else sky_color(altitude + 60)
        canvas.over(tint, band * rng.uniform(0.25, 0.45))


def add_stars(canvas: Canvas, top: int, rng: np.random.Generator) -> None:
    height = canvas.shape[0]
    for y in range(height):
        altitude = HORIZON_Y - (y + top)
        density = float(smoothstep(170, 620, altitude)) * 0.011
        for x in np.nonzero(rng.random(WIDTH) < density)[0]:
            brightness = rng.uniform(0.25, 0.8) * float(smoothstep(170, 500, altitude))
            canvas.rgb[y, x] = mix(canvas.rgb[y, x], rgb("#e8f0f6"), brightness)
    # A few bright stars with a soft halo.
    for _ in range(18):
        altitude = rng.uniform(330, HORIZON_Y - top - 4)
        y = int(HORIZON_Y - altitude - top)
        x = int(rng.uniform(2, WIDTH - 2))
        canvas.rgb[y, x] = rgb("#f5fafd")
        for dx, dy in ((1, 0), (-1, 0), (0, 1), (0, -1)):
            canvas.rgb[y + dy, x + dx] = mix(canvas.rgb[y + dy, x + dx], rgb("#cfdde4"), 0.35)


# ---------------------------------------------------------------------------
# High clouds seen during the descent


def build_clouds(rng: np.random.Generator) -> Canvas:
    top = LAYER_TOPS["Clouds"]
    height = HORIZON_Y - 150 - top
    canvas = Canvas(WIDTH, height)
    bands = [(170, 270, 4), (300, 400, 5), (430, 540, 5), (580, 700, 4), (740, 870, 4)]
    for low, high, count in bands:
        for _ in range(count):
            draw_cloud(canvas, top, rng.uniform(low, high), rng)
    return canvas.blurred(2.0)


def draw_cloud(canvas: Canvas, top: int, altitude: float, rng: np.random.Generator) -> None:
    """A long, soft stratus bank built from overlapping stretched blobs."""
    height = canvas.shape[0]
    yy, xx = np.mgrid[0:height, 0:WIDTH]
    cx = rng.uniform(0, WIDTH)
    cy = HORIZON_Y - altitude - top
    length = rng.uniform(140, 320)
    field = np.zeros((height, WIDTH))
    for _ in range(int(rng.integers(4, 8))):
        bx = cx + rng.uniform(-0.4, 0.4) * length
        by = cy + rng.uniform(-3, 3)
        sx = length * rng.uniform(0.2, 0.36)
        sy = rng.uniform(2.5, 5.5)
        dx = (xx - bx + WIDTH / 2) % WIDTH - WIDTH / 2
        field = np.maximum(field, np.exp(-(dx / sx) ** 2 - ((yy - by) / sy) ** 2))
    coverage = smoothstep(0.25, 0.75, field)
    # Seen against sky at half their altitude: a shade darker, lit a little from below.
    behind = sky_color(altitude * 0.5 + 40)
    body = mix(behind, rgb("#20293a"), 0.2)
    underside = mix(behind, rgb("#dae6ec"), 0.22)
    lower = smoothstep(cy - 2, cy + 6, yy)
    canvas.over(mix(body, underside, lower * 0.8), coverage * 0.8)


# ---------------------------------------------------------------------------
# Mountains


def midpoint_path(start, end, roughness: float, rng: np.random.Generator, depth: int = 6):
    points = [start, end]
    scale = roughness
    for _ in range(depth):
        refined = [points[0]]
        for a, b in zip(points, points[1:]):
            mx, my = (a[0] + b[0]) / 2, (a[1] + b[1]) / 2
            length = math.hypot(b[0] - a[0], b[1] - a[1])
            nx, ny = -(b[1] - a[1]) / max(length, 1e-6), (b[0] - a[0]) / max(length, 1e-6)
            offset = rng.uniform(-1, 1) * length * scale
            refined.extend([(mx + nx * offset, my + ny * offset), b])
        points = refined
        scale *= 0.62
    return points


def polyline_x_at(points, ys: np.ndarray) -> np.ndarray:
    pts = sorted(points, key=lambda p: p[1])
    return np.interp(ys, [p[1] for p in pts], [p[0] for p in pts])


class Ramp:
    def __init__(self, shade, lit, peak_lit, snow_shade, snow_lit, haze):
        self.shade, self.lit, self.peak_lit = shade, lit, peak_lit
        self.snow_shade, self.snow_lit, self.haze = snow_shade, snow_lit, haze


def draw_peak(canvas: Canvas, top: int, peak: tuple[float, float], base_y: float,
              spans: tuple[float, float], ramp: Ramp, snow_depth: float, rng: np.random.Generator) -> None:
    """A peak with a lit left face, a shaded right face, and a softly edged snow cap."""
    (peak_x, peak_y), (left_span, right_span) = peak, spans
    left = midpoint_path(peak, (peak_x - left_span, base_y), 0.10, rng)
    right = midpoint_path(peak, (peak_x + right_span, base_y), 0.10, rng)
    spine = midpoint_path(peak, (peak_x + rng.uniform(-0.25, 0.35) * right_span, base_y), 0.16, rng)
    height, _ = canvas.shape
    yy, xx = np.mgrid[0:height, 0:WIDTH]
    world_y = yy + top + 0.5
    ys = np.clip(world_y[:, 0], peak_y, base_y)
    lx, rx, sx = (polyline_x_at(path, ys)[:, None] for path in (left, right, spine))
    x = xx + 0.5
    inside = (world_y >= peak_y) & (world_y <= base_y) & (x >= lx) & (x <= rx)
    # Soft transition across the spine instead of a hard facet line.
    lit = 1 - smoothstep(-1.5, 1.5, x - sx)
    elevation = 1 - np.clip((world_y - peak_y) / max(base_y - peak_y, 1), 0, 1)
    rock = mix(mix(ramp.shade, ramp.lit, lit), ramp.peak_lit, elevation * lit * 0.6)
    drip = fbm_1d(WIDTH, [9.0, 4.0], rng)[None, :] * snow_depth * 0.45
    snow = 1 - smoothstep(-1.2, 1.2, world_y - (peak_y + snow_depth + drip))
    color = mix(rock, mix(ramp.snow_shade, ramp.snow_lit, lit), snow)
    canvas.over(color, inside.astype(float))


def add_haze(canvas: Canvas, top: int, haze_rows: tuple[float, float], color, strength: float) -> None:
    """Mist lying in the valleys, thickening toward the base."""
    rows = np.arange(canvas.shape[0])[:, None] + top
    amount = np.broadcast_to(smoothstep(haze_rows[0], haze_rows[1], rows) * strength, canvas.alpha.shape)
    canvas.rgb = mix(canvas.rgb, color, amount)


def fill_below(canvas: Canvas, top: int, from_y: int, color) -> None:
    canvas.rgb[from_y - top:] = color
    canvas.alpha[from_y - top:] = 1


FAR = Ramp(shade=rgb("#a1b2bb"), lit=rgb("#b0c3cd"), peak_lit=rgb("#c3d3db"),
           snow_shade=rgb("#cfdde4"), snow_lit=rgb("#eef4f7"), haze=rgb("#d4e6ec"))
MID = Ramp(shade=rgb("#6c7983"), lit=rgb("#83909c"), peak_lit=rgb("#94a4af"),
           snow_shade=rgb("#a1b2bb"), snow_lit=rgb("#d4e0e6"), haze=rgb("#a9bac4"))


def build_far_range(rng: np.random.Generator) -> Canvas:
    top = LAYER_TOPS["FarRange"]
    canvas = Canvas(WIDTH, LAYER_BOTTOM - top)
    base = 506
    peaks = [
        (-20, 470, 70, 80, 16), (62, 452, 70, 95, 22), (150, 468, 60, 60, 14),
        (212, 476, 40, 55, 10), (352, 474, 50, 50, 12), (438, 446, 95, 80, 24),
        (520, 462, 55, 70, 16), (600, 470, 70, 70, 14),
    ]
    for x, y, left, right, snow in peaks:
        draw_peak(canvas, top, (x, y), base, (left, right), FAR, snow, rng)
    fill_below(canvas, top, base, FAR.haze)
    add_haze(canvas, top, (470, 508), FAR.haze, 0.8)
    return canvas.blurred(2.5)


def build_mid_hills(rng: np.random.Generator) -> Canvas:
    top = LAYER_TOPS["MidHills"]
    canvas = Canvas(WIDTH, LAYER_BOTTOM - top)
    base = 516
    hills = [
        (18, 486, 60, 55, 5), (96, 492, 50, 60, 4), (168, 488, 45, 38, 6),
        (404, 490, 45, 50, 5), (470, 480, 40, 62, 7), (552, 488, 55, 60, 5),
    ]
    for x, y, left, right, snow in hills:
        draw_peak(canvas, top, (x, y), base, (left, right), MID, snow, rng)
    # Rounded snowdrifts between the hills.
    ridge = 500 + fbm_1d(WIDTH, [48.0, 16.0, 6.0], rng) * 5
    rows = np.arange(canvas.shape[0])[:, None] + top + 0.5
    drift = (rows >= ridge[None, :]) & (canvas.alpha < 0.5)
    drift_color = mix(MID.snow_lit, MID.shade, smoothstep(0, 10, rows - ridge[None, :]))
    canvas.over(drift_color, drift.astype(float))
    fill_below(canvas, top, base, MID.shade)
    add_haze(canvas, top, (496, 518), MID.haze, 0.55)
    return canvas.blurred(1.5)


def build_crater_rim(rng: np.random.Generator) -> Canvas:
    """The crater's far wall, seen above the bowl's front cross-section."""
    top = LAYER_TOPS["CraterRim"]
    height = LAYER_BOTTOM - top
    canvas = Canvas(WIDTH, height)
    xs = np.arange(WIDTH) + 0.5
    dx = (xs - CRATER_CENTER_X) / (CRATER_HALF_WIDTH + 12)
    inside = np.abs(dx) < 1
    # The far lip reads slightly higher than the near one, as the back of an ellipse.
    lip = np.where(inside, CRATER_LIP_Y - 5 * np.sqrt(np.clip(1 - dx * dx, 0, 1)), 0.0)
    # Ejecta slopes away outside the lips and tucks behind the surrounding plain.
    outer = np.clip((np.abs(xs - CRATER_CENTER_X) - CRATER_HALF_WIDTH - 12) / 34, 0, 1)
    lip = np.where(inside, lip, CRATER_LIP_Y + np.sqrt(outer) * 26)
    lip = lip + fbm_1d(WIDTH, [14.0, 5.0], rng) * 1.4
    rows = np.arange(height)[:, None] + top + 0.5
    depth = rows - lip[None, :]
    # Sky-lit near the lip, sinking into shadow toward the crater floor.
    wall = mix(BACKGROUND[2], BACKGROUND[0], smoothstep(3, 22, depth))
    wall = mix(wall, TERRAIN["rock"], smoothstep(18, 48, depth))
    wall = mix(wall, TERRAIN["dark"], smoothstep(44, 80, depth))
    # Snow sliding down the slope in soft streaks.
    streaks = np.clip(fbm_1d(WIDTH, [7.0, 3.0], rng), 0, None)[None, :]
    wall = mix(wall, BACKGROUND[4], streaks * (1 - smoothstep(4, 26, depth)) * 0.6)
    snow_depth = 2.6 + fbm_1d(WIDTH, [11.0, 4.0], rng)[None, :] * 1.3
    snow = 1 - smoothstep(snow_depth - 1.0, snow_depth + 1.0, depth)
    color = mix(wall, mix(BACKGROUND[6], TERRAIN["snow"], smoothstep(1.5, 0, depth)), snow)
    canvas.over(color, (depth >= 0).astype(float))
    return canvas.blurred(0.8)


# ---------------------------------------------------------------------------
# Landing cinematic effects

FIRE_STOPS = [(0.0, "#fffdf6"), (0.18, "#ffeec4"), (0.38, "#ffc978"), (0.58, "#f79552"),
              (0.78, "#d4604a"), (1.0, "#7e4a66")]
DUST_LIGHT, DUST_MID, DUST_SHADE = rgb("#e6eef2"), rgb("#b0c3cd"), rgb("#7e8e9b")


def fire_color(cooling) -> np.ndarray:
    """Hot white to cooling violet, for a cooling value in 0..1."""
    cooling = np.clip(np.asarray(cooling, dtype=float), 0, 1)
    stops = np.array([stop for stop, _ in FIRE_STOPS])
    colors = np.array([rgb(color) for _, color in FIRE_STOPS])
    return np.stack([np.interp(cooling, stops, colors[:, channel]) for channel in range(3)], axis=-1)


def grid(width: int, height: int, cx: float, cy: float):
    yy, xx = np.mgrid[0:height, 0:width]
    return xx + 0.5 - cx, yy + 0.5 - cy


def build_pod_ember(rng: np.random.Generator) -> list[Canvas]:
    """The distant falling pod: a glowing point trailing a fading streak (core at the bottom)."""
    width, height = 11, 48
    frames = []
    for index in range(4):
        canvas = Canvas(width, height)
        dx, dy = grid(width, height, width / 2, height - 6)
        flicker = 1 + 0.12 * math.sin(index * 1.9) + rng.uniform(-0.04, 0.04)
        trail_length = 36 + 3 * math.sin(index * 2.3)
        up = np.clip(-dy, 0, None)
        trail = np.exp(-(dx / (0.55 + up * 0.02)) ** 2) * np.clip(1 - up / trail_length, 0, 1) ** 1.6 * (up > 0)
        canvas.over(fire_color(0.45 + up / trail_length * 0.55), trail * 0.85)
        glow = np.exp(-(np.hypot(dx, dy) / (2.6 * flicker)) ** 2)
        canvas.over(fire_color(1 - glow), np.clip(glow * 1.25, 0, 1))
        frames.append(canvas)
    return frames


def build_pod_streak(rng: np.random.Generator) -> list[Canvas]:
    """The pod plunging into view: dark sphere under a white-hot cap, flames streaming up."""
    width, height = 76, 180
    radius = 21.0
    dx, dy = grid(width, height, 38.0, height - 30.0)
    distance = np.hypot(dx, dy)
    frames = []
    for _ in range(4):
        canvas = Canvas(width, height)
        # Flame body: tongues rising from the upper hemisphere, merged into one soft plume.
        field = np.zeros((height, width))
        for _ in range(26):
            offset = rng.uniform(-1.0, 1.0)
            base_x = offset * radius * 1.05
            base_y = -math.sqrt(max(0.0, 1 - offset * offset)) * radius * 0.75
            length = rng.uniform(40, 135) * (1 - 0.4 * abs(offset))
            base_width = rng.uniform(4.0, 8.0)
            lean = offset * rng.uniform(2, 8)
            t = np.clip((base_y - dy) / length, 0, 1)
            reach = (dy <= base_y + 6) & ((base_y - dy) <= length)
            tongue = np.exp(-((dx - base_x - lean * t) / (base_width * (1 - t) ** 1.2 + 0.6)) ** 2) * (1 - t) ** 0.8
            field = np.maximum(field, np.where(reach, tongue, 0))
        cooling = np.clip((-dy - radius * 0.2) / 125, 0, 1) * 0.8 + (1 - field) * 0.35
        canvas.over(fire_color(cooling), smoothstep(0.08, 0.55, field))
        # Plasma sheath hugging the leading hemisphere.
        sheath = np.exp(-((distance - radius - 1.5) / 3.2) ** 2) * smoothstep(-radius * 0.6, radius * 0.4, dy)
        canvas.over(fire_color(0.1), np.clip(sheath * 1.3, 0, 1))
        glow = np.exp(-((distance - radius) / 6.5) ** 2) * smoothstep(-radius, radius, dy)
        canvas.over(fire_color(0.3), glow * 0.6)
        nx, ny = dx / radius, dy / radius
        nz = np.sqrt(np.clip(1 - nx * nx - ny * ny, 0, 1))
        body = mix(TERRAIN["dark"], TERRAIN["rock"], np.clip(-nx * 0.4 + nz * 0.6, 0, 1))
        body = mix(body, fire_color(0.45), np.clip(ny * 0.85 + nz * 0.2, 0, 1) ** 2.2)
        canvas.over(body, (distance <= radius).astype(float))
        frames.append(canvas)
    return frames


def build_impact_burst(rng: np.random.Generator) -> list[Canvas]:
    """Soft flare, rays, and shock ring at impact; the anchor is the bottom center."""
    width, height = 256, 160
    dx, dy = grid(width, height, width / 2, height - 24.0)
    distance = np.hypot(dx, dy)
    angle = np.arctan2(-dy, dx)
    rays = [(rng.uniform(-0.2, math.pi + 0.2), rng.uniform(0.5, 1.0), rng.uniform(0.012, 0.03)) for _ in range(16)]
    torn = np.clip(0.55 + 0.6 * np.interp(angle, np.linspace(-math.pi, math.pi, 64), rng.uniform(-1, 1, 64)), 0, 1)
    frames = []
    count = 8
    for index in range(count):
        t = index / (count - 1)
        canvas = Canvas(width, height)
        fade = (1 - t) ** 1.5
        # Shockwave of thrown snow rolling out along the ground, torn up by noise.
        ring_radius = 14 + 110 * (1 - (1 - t) ** 2.2)
        flattened = np.hypot(dx, dy * 2.2)
        ring = np.exp(-((flattened - ring_radius) / (6 + 10 * t)) ** 2) * smoothstep(6, -10, dy)
        canvas.over(DUST_LIGHT, ring * torn * 0.6 * (1 - t) ** 2)
        # Rays of hot debris.
        reach = 30 + 110 * (1 - (1 - t) ** 2)
        for ray_angle, length, spread in rays:
            delta = np.abs((angle - ray_angle + math.pi) % (2 * math.pi) - math.pi)
            along = distance / (reach * length)
            ray = (np.exp(-(delta / spread) ** 2) * smoothstep(1.0, 0.55, along)
                   * smoothstep(0.15 + 0.5 * t, 0.35 + 0.5 * t, along))
            canvas.over(fire_color(0.2 + 0.7 * t + along * 0.2), ray * fade)
        # Central flare.
        core = np.exp(-(distance / (8 + 26 * fade)) ** 2)
        canvas.over(fire_color(0.05 + 0.6 * t), np.clip(core * 1.4 * fade, 0, 1))
        frames.append(canvas)
    return frames


def value_noise_2d(width: int, height: int, cell: float, offset: tuple[float, float], seed: int) -> np.ndarray:
    """Smooth lattice noise in -1..1, sampled at a moving offset so it can drift between frames."""
    yy, xx = np.mgrid[0:height, 0:width].astype(float)
    x = xx / cell + offset[0]
    y = yy / cell + offset[1]
    ix, iy = np.floor(x).astype(np.int64), np.floor(y).astype(np.int64)
    fx, fy = x - ix, y - iy
    fx, fy = fx * fx * (3 - 2 * fx), fy * fy * (3 - 2 * fy)

    def lattice(i, j):
        h = (i * 374761393 + j * 668265263 + seed * 2246822519) & 0xFFFFFFFF
        h = ((h ^ (h >> 13)) * 1274126177) & 0xFFFFFFFF
        return (h & 0xFFFF) / 32767.5 - 1.0

    top = lattice(ix, iy) * (1 - fx) + lattice(ix + 1, iy) * fx
    bottom = lattice(ix, iy + 1) * (1 - fx) + lattice(ix + 1, iy + 1) * fx
    return top * (1 - fy) + bottom * fy


def turbulence(width: int, height: int, time: float, seed: int) -> np.ndarray:
    """Billowing noise in 0..1 that rolls upward and outward as `time` advances."""
    total = np.zeros((height, width))
    amplitude, cell, weight = 1.0, 46.0, 0.0
    for octave in range(3):
        drift = (time * (0.2 + 0.08 * octave), -time * (0.45 + 0.2 * octave))
        total += (value_noise_2d(width, height, cell, drift, seed + octave) * 0.5 + 0.5) * amplitude
        weight += amplitude
        amplitude *= 0.45
        cell *= 0.5
    return blur_scalar(total / weight, 1.5)


def blur_scalar(values: np.ndarray, sigma: float) -> np.ndarray:
    support = math.ceil(sigma * 3)
    weights = np.exp(-np.arange(-support, support + 1) ** 2 / (2 * sigma * sigma))
    weights /= weights.sum()
    for axis in (1, 0):
        padding = [(0, 0), (0, 0)]
        padding[axis] = (support, support)
        padded = np.pad(values, padding, mode="edge")
        values = sum(weight * np.take(padded, np.arange(values.shape[axis]) + i, axis=axis)
                     for i, weight in enumerate(weights))
    return values


def build_dust_cloud(rng: np.random.Generator) -> list[Canvas]:
    """Snow dust thrown up by the impact; the anchor is the bottom center on the ground.

    Noise roughens the edge of an expanding dome, and a smoothed height field lights it
    from above in a few flat tones, so the cloud billows and erodes instead of reading as
    separate balls.
    """
    width, height = 300, 184
    ground_y, cx = height - 2.0, width / 2
    seed = int(rng.integers(1, 1 << 16))
    yy, xx = np.mgrid[0:height, 0:width].astype(float) + 0.5
    total = 40
    frames = []
    for index in range(total):
        t = index / (total - 1)
        grow = 1 - (1 - min(1.0, (index + 1) / 6)) ** 3
        # The dome keeps spreading slowly; its base stays on the ground.
        half_width = 34 + 104 * grow + 24 * t
        dome_height = 24 + 84 * grow + 22 * t
        dx = (xx - cx) / half_width
        dy = np.clip(ground_y - yy, 0, None) / dome_height
        radius = np.sqrt(dx * dx + dy * dy)
        billow = turbulence(width, height, index * 0.3, seed)
        erosion = smoothstep(0.3, 1.0, t) * 0.75
        # A finer octave bunches the rim into small billows.
        fine = value_noise_2d(width, height, 11.0, (index * 0.12, -index * 0.3), seed + 7) * 0.5 + 0.5
        field = 1 - radius + (billow - 0.5) * 0.7 + (fine - 0.5) * 0.22 - erosion + (billow - 0.5) * erosion * 1.2
        field = np.where(yy < ground_y, field, -1)
        # Crisp pixel silhouette with a few flat tones, like the game's sprites.
        solid = field > 0.0
        relief = blur_scalar(np.clip(field, 0, 0.5), 3.0) + billow * 0.18
        slope_x = np.gradient(relief, axis=1)
        slope_y = np.gradient(relief, axis=0)
        light = 0.55 + (slope_x * 0.6 + slope_y * 0.8) * 14.0 + dy * 0.12
        light -= smoothstep(ground_y - 40, ground_y, yy) * 0.25
        tones = np.digitize(light, [0.38, 0.56, 0.74])
        palette = np.array([DUST_SHADE, mix(DUST_SHADE, DUST_MID, 0.55), DUST_MID, DUST_LIGHT])
        canvas = Canvas(width, height)
        # The last wisps thin out to nothing, so the effect ends without a pop.
        canvas.over(palette[tones], solid * (1 - smoothstep(0.62, 1.0, t)))
        frames.append(canvas)
    return frames


def build_trail_puff() -> list[Canvas]:
    """A spark shed by the falling pod, cooling into a wisp of smoke."""
    frames = []
    dx, dy = grid(9, 9, 4.5, 4.5)
    for index in range(6):
        t = index / 5
        canvas = Canvas(9, 9)
        glow = np.exp(-(np.hypot(dx, dy) / (1.2 + 2.2 * t)) ** 2)
        color = mix(fire_color(0.3 + 0.7 * min(1, t * 1.6)), BACKGROUND[3], smoothstep(0.35, 0.9, t))
        canvas.over(color, glow * (1 - t) ** 1.2 * 0.95)
        frames.append(canvas)
    return frames


def build_debris() -> list[Canvas]:
    """Two rock chunks at four rotations each, 8×8, in the terrain tile colors."""
    shapes = [
        ["..###...", ".#####..", "#######.", "######..", ".####...", "..##...."],
        [".##.....", "####....", ".#####..", "..####..", "...##..."],
    ]
    frames = []
    for shape in shapes:
        base = np.zeros((8, 8), dtype=bool)
        for y, row in enumerate(shape):
            for x, char in enumerate(row):
                base[y + 1, x] = char == "#"
        for turn in range(4):
            mask = np.rot90(base, turn)
            canvas = Canvas(8, 8)
            canvas.over(TERRAIN["rock"], mask.astype(float))
            top_edge = mask & ~np.vstack([np.zeros((1, 8), bool), mask[:-1]])
            canvas.over(TERRAIN["edge"], top_edge.astype(float))
            frames.append(canvas)
    return frames


FX_ANIMATIONS = {
    # name: (builder, ticks per frame, playback mode, audio cues on the first frame)
    "PodEmber": (build_pod_ember, 5, "loop", []),
    "PodStreak": (build_pod_streak, 3, "loop", []),
    "ImpactBurst": (build_impact_burst, 4, "once",
                    [("audio.game.DashDownImpact", 0.9), ("audio.game.ImpactBass", 0.8)]),
    "DustCloud": (build_dust_cloud, 6, "once", []),
    "TrailPuff": (lambda rng: build_trail_puff(), 6, "once", []),
}


def save_rgba(array: np.ndarray, path: Path) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    Image.fromarray(array, "RGBA").save(path, optimize=True)


def save_sheet(frames: list[Canvas], name: str) -> str:
    height, width = frames[0].shape
    columns = min(len(frames), 8)
    rows = math.ceil(len(frames) / columns)
    sheet = np.zeros((rows * height, columns * width, 4), dtype=np.uint8)
    for index, frame in enumerate(frames):
        row, column = divmod(index, columns)
        sheet[row * height:(row + 1) * height, column * width:(column + 1) * width] = frame.to_uint8()
    filename = f"{name}__r{rows}c{columns}__{width}x{height}.png"
    for stale in FX_DIR.glob(f"{name}__*.png"):
        stale.unlink()
    save_rgba(sheet, FX_DIR / filename)
    return filename


def write_animation(name: str, filename: str, frame_count: int, playback: tuple[int, str], cues) -> None:
    ticks, mode = playback
    scripts = "".join(f"""
[[script]]
run_at_frame = 1
action = "PlayAudio"
asset_key = "{key}"
volume = {volume}
""" for key, volume in cues)
    (FX_DIR / f"{name}.anim.toml").write_text(f"""# Generated by tools/surface_art/generate.py.
[asset]
key = "anim.intro.{name}"
image = "animations/intro/{filename}"

[settings]
time_base = "Relative"
frame_clock = "Local"
ticks_per_frame = {ticks}
frame_min = 1
frame_max = {frame_count}
frame_start = 1

[playback]
start = 1
end = {frame_count}
ticks_per_frame = {ticks}
mode = "{mode}"
{scripts}""")


def build_effects() -> dict[str, list[Canvas]]:
    effects = {}
    for offset, (name, (builder, ticks, mode, cues)) in enumerate(FX_ANIMATIONS.items()):
        frames = builder(np.random.default_rng(0x1A4D_0000 + offset))
        write_animation(name, save_sheet(frames, name), len(frames), (ticks, mode), cues)
        effects[name] = frames
    save_sheet(build_debris(), "Debris")
    return effects


# ---------------------------------------------------------------------------
# Output and previews


def build_backdrop() -> dict[str, Canvas]:
    return {
        "Sky": build_sky(np.random.default_rng(0x5EED_0001)),
        "Clouds": build_clouds(np.random.default_rng(0x5EED_0002)),
        "FarRange": build_far_range(np.random.default_rng(0x5EED_0003)),
        "MidHills": build_mid_hills(np.random.default_rng(0x5EED_0004)),
        "CraterRim": build_crater_rim(np.random.default_rng(0x5EED_0005)),
    }


# Parallax depths mirror surface_backdrop.rs; used only for previews.
PREVIEW_DEPTHS = {"Sky": 4.0, "Clouds": 2.0, "FarRange": 1.6, "MidHills": 1.25, "CraterRim": 1.05}
REST_CAMERA = (291, 505)


def preview(layers: dict[str, np.ndarray], altitude: float, size=(330, 190)) -> Image.Image:
    """The backdrop, a terrain silhouette, and the capsule seen from a camera altitude."""
    width, height = size
    cam_x, cam_y = REST_CAMERA[0], REST_CAMERA[1] - altitude
    frame = Image.new("RGBA", size, (255, 255, 255, 255))
    for name, depth in PREVIEW_DEPTHS.items():
        offset_y = LAYER_TOPS[name] - REST_CAMERA[1] + altitude / depth
        frame.alpha_composite(Image.fromarray(layers[name], "RGBA"),
                              (int(round(width / 2 - cam_x)), int(round(height / 2 + offset_y))))
    frame.alpha_composite(terrain_preview(), (int(round(width / 2 - cam_x)), int(round(height / 2 - cam_y))))
    capsule = Image.open(ROOT / "assets/animations/objects/CapsuleInside__r1c1__54x154.png").convert("RGBA")
    frame.alpha_composite(capsule, (int(round(width / 2 - cam_x + CRATER_CENTER_X - 27)),
                                    int(round(height / 2 - cam_y + 567 - 98))))
    return frame


def terrain_preview() -> Image.Image:
    chunk = np.asarray(Image.open(SPAWN_CHUNK).convert("RGBA"))
    solid = (chunk[:, :, 3] > 0) & np.all(chunk[:, :, :3] == 0, axis=2)
    big = np.kron(solid, np.ones((CHUNK_TILE_PX, CHUNK_TILE_PX), dtype=bool))
    image = np.zeros((*big.shape, 4), dtype=np.uint8)
    image[big] = (32, 36, 42, 255)
    edge = big & ~np.vstack([np.zeros((1, big.shape[1]), bool), big[:-1]])
    image[edge] = (237, 240, 241, 255)
    return Image.fromarray(image, "RGBA")


def write_previews(layers: dict[str, np.ndarray], effects: dict[str, list[Canvas]], directory: Path) -> None:
    directory.mkdir(parents=True, exist_ok=True)
    for altitude in (0, 120, 400, 900, 1500):
        image = preview(layers, altitude)
        image.resize((image.width * 3, image.height * 3), Image.BILINEAR).save(directory / f"backdrop_alt{altitude:04d}.png")
    for name, frames in effects.items():
        height, width = frames[0].shape
        columns = min(len(frames), 10)
        strip = Image.new("RGBA", (width * columns, height * math.ceil(len(frames) / columns)), (52, 62, 80, 255))
        for index, frame in enumerate(frames):
            strip.alpha_composite(Image.fromarray(frame.to_uint8(), "RGBA"),
                                  ((index % columns) * width, (index // columns) * height))
        strip.resize((strip.width * 2, strip.height * 2), Image.NEAREST).save(directory / f"fx_{name}.png")


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--preview", type=Path, help="write composited previews to this directory")
    args = parser.parse_args()
    layers = {name: canvas.to_uint8() for name, canvas in build_backdrop().items()}
    for name, array in layers.items():
        save_rgba(array, BACKDROP_DIR / f"{name}.png")
    effects = build_effects()
    if args.preview:
        write_previews(layers, effects, args.preview)


if __name__ == "__main__":
    main()
