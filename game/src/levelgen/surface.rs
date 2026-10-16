//! Chunk-sized CPU surfaces shared by terrain, backgrounds, and prepared world changes.

use super::*;
use bevy::asset::RenderAssetUsages;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy::tasks::{ComputeTaskPool, TaskPool};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct TilePaint {
    pub tile: ldtk_rules::TilePlacement,
    pub source: usize,
}

pub(super) struct TileSurface<'a> {
    pub size: UVec2,
    pub tile_px: u32,
    pub sources: Vec<(&'a Image, u32)>,
    pub cells: Vec<Option<TilePaint>>,
}

pub(super) fn image(size: UVec2) -> Image {
    let mut image = Image::new_fill(
        Extent3d { width: size.x, height: size.y, depth_or_array_layers: 1 },
        TextureDimension::D2, &[0, 0, 0, 0], TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD,
    );
    image.sampler = ImageSampler::nearest();
    image
}

pub(super) fn chunk_regions(size: UVec2) -> Vec<URect> {
    let step = mapgen::CHUNK_IMAGE_SIZE;
    let cols = size.x.div_ceil(step);
    let rows = size.y.div_ceil(step);
    (0..cols * rows).map(|index| {
        let min = UVec2::new(index % cols, index / cols) * step;
        URect { min, max: (min + UVec2::splat(step)).min(size) }
    }).collect()
}

pub(super) fn expand(region: URect, margin: u32, size: UVec2) -> URect {
    URect {
        min: UVec2::new(region.min.x.saturating_sub(margin), region.min.y.saturating_sub(margin)),
        max: region.max.saturating_add(UVec2::splat(margin)).min(size),
    }
}

pub(super) fn bake_jobs<T: Send + 'static>(regions: &[URect], bake: impl Fn(URect) -> T + Sync) -> Vec<T> {
    if regions.is_empty() { return Vec::new(); }
    let pool = ComputeTaskPool::get_or_init(TaskPool::new);
    let workers = pool.thread_num().clamp(1, 4);
    let batch_size = regions.len().div_ceil(workers);
    pool.scope(|scope| {
        for batch in regions.chunks(batch_size) {
            let bake = &bake;
            scope.spawn(bake_batch(batch, bake));
        }
    }).into_iter().flatten().collect()
}

impl TileSurface<'_> {
    pub(super) fn bake(&self, region: URect) -> Image {
        let mut output = image(region.size() * self.tile_px);
        let width = output.width();
        let bytes = output.data.as_mut().expect("surface pixels");
        for y in region.min.y..region.max.y {
            blit_row(self, bytes, width, region, y);
        }
        output
    }

}

fn blit_row(surface: &TileSurface<'_>, bytes: &mut [u8], width: u32, region: URect, y: u32) {
    for x in region.min.x..region.max.x {
        let Some(paint) = &surface.cells[(y * surface.size.x + x) as usize] else { continue; };
        let (source, tiles_per_row) = surface.sources[paint.source];
        let tile = paint.tile.tile_id;
        let from = UVec2::new(tile % tiles_per_row, tile / tiles_per_row) * surface.tile_px;
        let to = (UVec2::new(x, y) - region.min) * surface.tile_px;
        let flip = BVec2::new(paint.tile.flip_x, paint.tile.flip_y);
        paint_tile(bytes, width, to, source, from, surface.tile_px, flip);
    }
}

/// Paints the `tile_px` tile at `from` in `source` onto `dest` at `to`, mirrored per
/// `flip`, over what is already there.
fn paint_tile(dest: &mut [u8], dest_width: u32, to: UVec2, source: &Image, from: UVec2, tile_px: u32, flip: BVec2) {
    let Some(src) = source.data.as_deref() else { return; };
    let src_width = source.width();
    for (tx, ty) in (0..tile_px).flat_map(|ty| (0..tile_px).map(move |tx| (tx, ty))) {
        let sx = from.x + if flip.x { tile_px - 1 - tx } else { tx };
        let sy = from.y + if flip.y { tile_px - 1 - ty } else { ty };
        let src_idx = ((sy * src_width + sx) * 4) as usize;
        let dst_idx = ((to.y + ty) * dest_width + (to.x + tx)) as usize * 4;
        if src_idx + 3 < src.len() && dst_idx + 3 < dest.len() {
            blend_over(&mut dest[dst_idx..dst_idx + 4], &src[src_idx..src_idx + 4]);
        }
    }
}

/// Source-over alpha blending of one RGBA pixel.
fn blend_over(dst: &mut [u8], src: &[u8]) {
    let alpha = src[3];
    if alpha == 0 {
        return;
    }
    if alpha == 255 {
        dst.copy_from_slice(&[src[0], src[1], src[2], 255]);
        return;
    }
    let sa = alpha as f32 / 255.0;
    let da = dst[3] as f32 / 255.0;
    let out_a = sa + da * (1.0 - sa);
    if out_a <= 0.0 {
        return;
    }
    for channel in 0..3 {
        dst[channel] = ((src[channel] as f32 * sa + dst[channel] as f32 * da * (1.0 - sa)) / out_a) as u8;
    }
    dst[3] = (out_a * 255.0) as u8;
}

pub(super) fn crop(source: &Image, rect: URect) -> Image {
    let mut output = image(rect.size());
    let Some(src) = &source.data else { return output; };
    let dst = output.data.as_mut().expect("crop pixels");
    let src_stride = source.width() as usize * 4;
    let dst_stride = rect.width() as usize * 4;
    for (y, row) in dst.chunks_exact_mut(dst_stride).enumerate() {
        let start = (rect.min.y as usize + y) * src_stride + rect.min.x as usize * 4;
        row.copy_from_slice(&src[start..start + dst_stride]);
    }
    output
}

pub(super) fn visible(image: &Image) -> bool {
    image.data.as_deref().unwrap_or_default().as_chunks::<4>().0.iter().any(|pixel| pixel[3] != 0)
}

pub(super) fn alpha(image: &Image) -> Vec<u8> {
    image.data.as_deref().unwrap_or_default().as_chunks::<4>().0.iter()
        .map(|pixel| u8::from(pixel[3] != 0)).collect()
}

pub(super) struct GroundPatch {
    pub region: URect,
    pub color: Image,
    pub overlay: Option<Image>,
}

pub(super) fn bake_ground(
    surface: &TileSurface<'_>, level: &CompiledLevel, region: URect,
    decorations: &[DecorationPlacement], images: &Assets<Image>,
) -> GroundPatch {
    let _timer = diagnostics::StageTimer::new("ground_region_bake");
    let padded = expand(region, ground_shading::SUPPORT_PX.div_ceil(surface.tile_px), surface.size);
    let mut color = surface.bake(padded);
    if !visible(&color) && decorations.is_empty() {
        return GroundPatch { region, color: image(region.size() * surface.tile_px), overlay: None };
    }
    let origin = padded.min * surface.tile_px;
    let width = color.width();
    let height = color.height();
    let bytes = color.data.as_mut().expect("ground pixels");
    let spike_mask = spike_pixels(bytes, width, origin, level, surface.tile_px);
    ground_shading::shade(bytes, width as usize * 4, origin, level, surface.tile_px);
    let overlay = build_spike_light_overlay_image(bytes, width, height, &spike_mask);
    decorate(&mut color, origin, decorations, images, surface.tile_px);
    let crop_rect = URect { min: (region.min - padded.min) * surface.tile_px,
        max: (region.max - padded.min) * surface.tile_px };
    GroundPatch { region, color: crop(&color, crop_rect),
        overlay: overlay.map(|image| crop(&image, crop_rect)) }
}

fn spike_pixels(bytes: &[u8], width: u32, origin: UVec2, level: &CompiledLevel, tile_px: u32) -> Vec<u8> {
    bytes.as_chunks::<4>().0.iter().enumerate().map(|(index, pixel)| {
        let tile = (origin + UVec2::new(index as u32 % width, index as u32 / width)) / tile_px;
        let spike = level.spike_tiles.get((tile.y * level.width + tile.x) as usize).copied().unwrap_or(false);
        u8::from(spike && pixel[3] != 0 && pixel[..3] == SPIKE_LIGHT_OVERLAY_COLOR)
    }).collect()
}

/// Paints active decorations over an image whose top-left corner sits at `origin` in level pixels.
pub(super) fn decorate(image: &mut Image, origin: UVec2, placements: &[DecorationPlacement], images: &Assets<Image>, tile_px: u32) {
    let size = IVec2::new(image.width() as i32, image.height() as i32);
    let origin = origin.as_ivec2();
    let overlaps = |art: &DecorationArt<'_>| (art.origin - origin).cmplt(size).all() && (art.origin + art.size).cmpgt(origin).all();
    let arts = placements.iter().filter(|placement| placement.active).filter_map(|placement| DecorationArt::new(placement, images, tile_px));
    let pixels = arts.filter(overlaps).flat_map(DecorationArt::pixels).map(|(at, pixel)| (at - origin, pixel));
    let data = image.data.as_mut().expect("decoration pixels");
    for (at, pixel) in pixels.filter(|(at, _)| at.cmpge(IVec2::ZERO).all() && at.cmplt(size).all()) {
        let index = (at.y * size.x + at.x) as usize * 4;
        data[index..index + 4].copy_from_slice(pixel);
    }
}

/// Level pixels, counted bottom-up, that snow decorations cover, so blood stays behind the snow.
pub(super) fn snow_occlusion(placements: &[DecorationPlacement], size: UVec2, tile_px: u32, images: &Assets<Image>) -> BloodForegroundOcclusion {
    let size = size.as_ivec2();
    let snow = placements.iter().filter(|placement| placement.active && placement.kind == "snow");
    let pixels = snow.filter_map(|placement| DecorationArt::new(placement, images, tile_px)).flat_map(DecorationArt::pixels);
    let inside = pixels.map(|(at, _)| at).filter(|at| at.cmpge(IVec2::ZERO).all() && at.cmplt(size).all());
    BloodForegroundOcclusion::from_pixels(inside.map(|at| IVec2::new(at.x, size.y - 1 - at.y)))
}

/// A decoration's sprite, hung 10px below its tile's bottom center and shifted by its offset (positive y up).
#[derive(Clone, Copy)]
struct DecorationArt<'a> {
    bytes: &'a [u8],
    size: IVec2,
    /// The top-left corner in level pixels.
    origin: IVec2,
    flip: BVec2,
}

impl<'a> DecorationArt<'a> {
    fn new(placement: &DecorationPlacement, images: &'a Assets<Image>, tile_px: u32) -> Option<Self> {
        let image = images.get(&placement.handle)?;
        let size = IVec2::new(image.width() as i32, image.height() as i32);
        if size.min_element() == 0 {
            return None;
        }
        let tile = tile_px as i32;
        let anchor = IVec2::new(placement.tile_x as i32, placement.tile_y as i32) * tile + IVec2::new(tile / 2, tile + 10);
        let origin = anchor - IVec2::new(size.x / 2, size.y) + placement.offset_px * IVec2::new(1, -1);
        Some(Self { bytes: image.data.as_deref()?, size, origin, flip: BVec2::new(placement.flip_x, placement.flip_y) })
    }

    /// Each opaque pixel's position in level pixels, with its color.
    fn pixels(self) -> impl Iterator<Item = (IVec2, &'a [u8])> {
        let positions = (0..self.size.y).flat_map(move |y| (0..self.size.x).map(move |x| IVec2::new(x, y)));
        positions.filter_map(move |at| {
            let source = IVec2::select(self.flip, self.size - 1 - at, at);
            let index = (source.y * self.size.x + source.x) as usize * 4;
            self.bytes.get(index..index + 4).filter(|pixel| pixel[3] != 0).map(|pixel| (self.origin + at, pixel))
        })
    }
}

async fn bake_batch<T>(regions: &[URect], bake: &(impl Fn(URect) -> T + Sync)) -> Vec<T> {
    regions.iter().map(|&region| bake(region)).collect()
}

/// Decoration ownership is indexed once, including artwork that overhangs a neighboring chunk.
pub(super) fn decoration_chunks(placements: &[DecorationPlacement], images: &Assets<Image>, tile_px: u32) -> HashMap<IVec2, Vec<DecorationPlacement>> {
    let edge = IVec2::splat((mapgen::CHUNK_IMAGE_SIZE * tile_px) as i32);
    let mut chunks: HashMap<IVec2, Vec<DecorationPlacement>> = HashMap::new();
    for placement in placements.iter().filter(|placement| placement.active) {
        let Some(art) = DecorationArt::new(placement, images, tile_px) else { continue };
        let (min, max) = (art.origin.div_euclid(edge), (art.origin + art.size - 1).div_euclid(edge));
        for chunk in (min.y..=max.y).flat_map(|y| (min.x..=max.x).map(move |x| IVec2::new(x, y))) {
            chunks.entry(chunk).or_default().push(placement.clone());
        }
    }
    chunks
}
