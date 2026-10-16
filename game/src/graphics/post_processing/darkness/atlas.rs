//! R8 virtual light map pages. Gutters preserve bilinear sampling across page edges.
use bevy::asset::RenderAssetUsages;
use super::*;
use bevy::render::render_resource::{Extent3d, TextureViewDescriptor};

pub(super) const PAGE_EDGE: u32 = 2048;
const TEXTURE_EDGE: u32 = PAGE_EDGE + 2;

pub(super) fn pack(size: UVec2, pixels: &[u8], max_layers: u32) -> Result<(Image, Image), String> {
    let columns = size.x.div_ceil(PAGE_EDGE).max(1);
    let rows = size.y.div_ceil(PAGE_EDGE).max(1);
    let mut table = Vec::with_capacity((columns * rows) as usize);
    let page_bytes = (TEXTURE_EDGE * TEXTURE_EDGE) as usize;
    let mut data = vec![0; page_bytes];
    for index in 0..columns * rows {
        let origin = UVec2::new(index % columns, index / columns) * PAGE_EDGE;
        if !page_has_light(size, pixels, origin) { table.push(0); continue; }
        let layer = data.len() / page_bytes;
        if layer >= max_layers as usize { return Err(format!("baked darkness requires more than {max_layers} resident atlas pages")); }
        table.push(layer as u32);
        append_page(&mut data, size, pixels, origin);
    }
    let mut pages = Image::new(Extent3d { width: TEXTURE_EDGE, height: TEXTURE_EDGE,
        depth_or_array_layers: (data.len() / page_bytes) as u32 }, TextureDimension::D2,
        data, TextureFormat::R8Unorm, RenderAssetUsages::RENDER_WORLD);
    pages.texture_view_descriptor = Some(TextureViewDescriptor { dimension: Some(TextureViewDimension::D2Array), ..default() });
    let table = Image::new(Extent3d { width: columns, height: rows, depth_or_array_layers: 1 },
        TextureDimension::D2, table.into_iter().flat_map(u32::to_le_bytes).collect(),
        TextureFormat::R32Uint, RenderAssetUsages::RENDER_WORLD);
    Ok((pages, table))
}

fn page_has_light(size: UVec2, pixels: &[u8], origin: UVec2) -> bool {
    let min = origin.as_ivec2() - IVec2::ONE;
    let max = (origin + UVec2::splat(PAGE_EDGE + 1)).min(size);
    (min.y.max(0) as u32..max.y).any(|y| {
        let start = (y * size.x + min.x.max(0) as u32) as usize;
        pixels[start..(y * size.x + max.x) as usize].iter().any(|&v| v != 0)
    })
}

fn append_page(data: &mut Vec<u8>, size: UVec2, pixels: &[u8], origin: UVec2) {
    for y in 0..TEXTURE_EDGE {
        let source_y = (origin.y + y).saturating_sub(1).min(size.y - 1);
        append_row(data, size, pixels, origin.x, source_y);
    }
}

fn append_row(data: &mut Vec<u8>, size: UVec2, pixels: &[u8], origin_x: u32, source_y: u32) {
    for x in 0..TEXTURE_EDGE {
        let source_x = (origin_x + x).saturating_sub(1).min(size.x - 1);
        data.push(pixels[(source_y * size.x + source_x) as usize]);
    }
}
