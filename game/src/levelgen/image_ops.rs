//! Pixel clearing and prepared spike overlays.
use bevy::asset::RenderAssetUsages;
use super::*;

pub(super) fn clear_image_rect(image: &mut Image, origin: IVec2, size: UVec2) {
    let Some(bytes) = image.data.as_mut() else {
        return;
    };
    let image_w = image.texture_descriptor.size.width as i32;
    let image_h = image.texture_descriptor.size.height as i32;
    if image_w <= 0 || image_h <= 0 || size.x == 0 || size.y == 0 {
        return;
    }

    let x0 = origin.x.max(0);
    let y0 = origin.y.max(0);
    let x1 = origin
        .x
        .saturating_add(size.x.min(i32::MAX as u32) as i32)
        .min(image_w);
    let y1 = origin
        .y
        .saturating_add(size.y.min(i32::MAX as u32) as i32)
        .min(image_h);
    if x0 >= x1 || y0 >= y1 {
        return;
    }

    for y in y0..y1 {
        let row_start = ((y * image_w + x0) * 4) as usize;
        let row_end = ((y * image_w + x1) * 4) as usize;
        bytes[row_start..row_end].fill(0);
    }
}

pub(super) const SPIKE_LIGHT_OVERLAY_COLOR: [u8; 3] = [0xC2, 0xC9, 0xC9];
pub(super) const SPIKE_LIGHT_OVERLAY_LAYER: usize = 2;


#[expect(
    clippy::needless_range_loop,
    reason = "Overlay image assembly maps flat pixel indices across matching byte buffers."
)]
pub(super) fn build_spike_light_overlay_image(
    source_bytes: &[u8],
    width: u32,
    height: u32,
    mask: &[u8],
) -> Option<Image> {
    use bevy::render::{
        render_resource::{Extent3d, TextureDimension, TextureFormat},
    };

    if width == 0 || height == 0 {
        return None;
    }

    let pixel_count = (width as usize).saturating_mul(height as usize);
    if mask.len() < pixel_count || source_bytes.len() < pixel_count * 4 {
        return None;
    }

    if !mask.contains(&1) { return None; }
    let mut out_bytes = vec![0u8; pixel_count * 4];
    let mut any_visible = false;
    for pixel_idx in 0..pixel_count {
        if mask[pixel_idx] == 0 {
            continue;
        }

        let src_idx = pixel_idx * 4;
        let dst_idx = src_idx;
        if source_bytes[src_idx + 3] == 0 {
            continue;
        }
        out_bytes[dst_idx..dst_idx + 4]
            .copy_from_slice(&source_bytes[src_idx..src_idx + 4]);
        any_visible = true;
    }

    if !any_visible {
        return None;
    }

    let mut image = Image::new(
        Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        out_bytes,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::default(),
    );
    image.sampler = ImageSampler::nearest();
    Some(image)
}

pub(super) fn srgb8_to_linear_channel(v: u8) -> f32 {
    srgb_to_linear(v as f32 / 255.0)
}

pub(super) fn linear_to_srgb8_channel(v: f32) -> u8 {
    (linear_to_srgb(v).clamp(0.0, 1.0) * 255.0 + 0.5).floor() as u8
}

pub(super) fn srgb_to_linear(c: f32) -> f32 {
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

pub(super) fn linear_to_srgb(c: f32) -> f32 {
    if c <= 0.003_130_8 {
        12.92 * c
    } else {
        1.055 * c.powf(1.0 / 2.4) - 0.055
    }
}
