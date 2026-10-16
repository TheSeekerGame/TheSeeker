//! Terrain-edge shading at artwork resolution, independent of render chunk boundaries.

use super::{distance::DistanceField, CompiledLevel, GROUND_DARKEN_CURVE,
    GROUND_DARKEN_RANGE_PX, GROUND_DARKEN_START_PX};
use bevy::prelude::*;
use std::sync::OnceLock;

pub(super) const SUPPORT_PX: u32 = 64;
thread_local! { static DISTANCES: std::cell::RefCell<DistanceField> = std::cell::RefCell::default(); }
static ATTENUATION: OnceLock<[[u8; 256]; 65]> = OnceLock::new();

fn attenuation() -> &'static [[u8; 256]; 65] {
    ATTENUATION.get_or_init(|| std::array::from_fn(|distance| {
        let t = (distance as f32 / GROUND_DARKEN_RANGE_PX).clamp(0.0, 1.0);
        let gain = 1.0 - t.powf(GROUND_DARKEN_CURVE);
        std::array::from_fn(|channel| super::linear_to_srgb8_channel(
            super::srgb8_to_linear_channel(channel as u8) * gain))
    }))
}

fn terrain_at(level: &CompiledLevel, x: i32, y: i32) -> bool {
    x >= 0 && y >= 0 && x < level.width as i32 && y < level.height as i32
        && level.terrain[(y as u32 * level.width + x as u32) as usize]
}

fn boundary_at(level: &CompiledLevel, pixel: UVec2, tile_px: u32) -> bool {
    let tile = pixel / tile_px;
    let x = tile.x as i32;
    let y = tile.y as i32;
    if !terrain_at(level, x, y) { return false; }
    let local = pixel % tile_px;
    (local.x == 0 && !terrain_at(level, x - 1, y))
        || (local.x + 1 == tile_px && !terrain_at(level, x + 1, y))
        || (local.y == 0 && !terrain_at(level, x, y - 1))
        || (local.y + 1 == tile_px && !terrain_at(level, x, y + 1))
}

pub(super) fn shade(data: &mut [u8], stride: usize, origin: UVec2, level: &CompiledLevel, tile_px: u32) {
    DISTANCES.with_borrow_mut(|field| shade_with_buffer(data, stride, origin, level, tile_px, field));
}

fn shade_with_buffer(data: &mut [u8], stride: usize, origin: UVec2, level: &CompiledLevel, tile_px: u32, field: &mut DistanceField) {
    if stride == 0 || tile_px == 0 { return; }
    let width = stride / 4;
    let height = data.len() / stride;
    field.compute(width, height, |x, y| boundary_at(level,
        origin + UVec2::new(x as u32, y as u32), tile_px));
    let palette = attenuation();
    for (index, pixel) in data.as_chunks_mut::<4>().0.iter_mut().enumerate() {
        if pixel[3] == 0 { continue; }
        let x = origin.x + (index % width) as u32;
        let y = origin.y + (index / width) as u32;
        let noise = (52.982_918 * (0.067_110_56 * x as f32 + 0.005_837_15 * y as f32).fract()).fract();
        let distance = field.squared[index].sqrt().min(SUPPORT_PX as f32);
        let step = ((distance + noise).floor() as i32 - GROUND_DARKEN_START_PX).max(0) as usize;
        let table = &palette[step.min(64)];
        pixel[0] = table[pixel[0] as usize];
        pixel[1] = table[pixel[1] as usize];
        pixel[2] = table[pixel[2] as usize];
    }
}
