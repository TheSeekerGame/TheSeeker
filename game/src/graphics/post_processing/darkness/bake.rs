//! The startup lightmap bake. Light reaches the cave where daylight can: a lightmap pixel
//! is a source where a camera looking at it sees through the ground and every background
//! layer to the sky, and every pixel takes the light of its nearest source, full at the
//! source and fading to nothing at the light's radius.

use super::*;
use crate::levelgen::distance::DistanceField;

/// Lightmap pixels per level tile.
const LIGHTMAP_SCALE: u32 = 2;
/// How far a source lights, in world units.
const SOURCE_LIGHT_RADIUS_WORLD: f32 = 24.0;
/// How quickly a source's light fades toward its radius (1 is linear).
const SOURCE_LIGHT_FALLOFF_EXPONENT: f32 = 2.0;

const MASK_CHUNK_EDGE: u32 = mapgen::CHUNK_IMAGE_SIZE * crate::levelgen::GROUND_TILE_ART_SIZE_PX;

/// One layer's coverage, which parallax shifts against the camera by its depth.
#[derive(Clone, Default)]
pub struct DarknessBakeMaskLayer {
    chunks: Arc<HashMap<UVec2, MaskChunk>>,
    size_px: UVec2,
    world_center: Vec2,
    world_size: Vec2,
    parallax_depth: f32,
}

#[derive(Clone)]
struct MaskChunk { size: UVec2, alpha: Vec<u8> }

impl DarknessBakeMaskLayer {
    pub(crate) fn set_chunks(&mut self, size: UVec2,
        chunks: Vec<(UVec2, u32, u32, Vec<u8>)>, center: Vec2, world_size: Vec2, depth: f32) {
        let chunks = chunks.into_iter().filter(|(_, _, _, alpha)| alpha.contains(&1))
            .map(|(origin, width, height, alpha)| (origin / MASK_CHUNK_EDGE, MaskChunk { size: UVec2::new(width, height), alpha }));
        *self = Self { chunks: Arc::new(chunks.collect()), size_px: size, world_center: center, world_size, parallax_depth: depth };
    }

    fn is_empty(&self) -> bool {
        self.chunks.is_empty() || self.size_px.min_element() == 0 || self.world_size.min_element() <= 0.0
    }

    /// Whether the layer covers `world` for a camera centered on it. Parallax shows a layer
    /// its depth times nearer its own center there.
    fn covers(&self, world: Vec2) -> bool {
        let depth = if self.parallax_depth.is_finite() && self.parallax_depth > 0.0 { self.parallax_depth } else { 1.0 };
        let offset = (world - self.world_center) / depth / self.world_size;
        let uv = Vec2::new(0.5 + offset.x, 0.5 - offset.y);
        if uv.cmplt(Vec2::ZERO).any() || uv.cmpge(Vec2::ONE).any() {
            return false;
        }
        let pixel = (uv * self.size_px.as_vec2()).as_uvec2().min(self.size_px - 1);
        self.chunks.get(&(pixel / MASK_CHUNK_EDGE)).is_some_and(|chunk| {
            let local = pixel % MASK_CHUNK_EDGE;
            local.cmplt(chunk.size).all() && chunk.alpha[(local.y * chunk.size.x + local.x) as usize] != 0
        })
    }
}

/// The layers the sky is seen through.
#[derive(Resource, Clone, Default)]
pub struct DarknessBakeMaskAtlas {
    pub near: DarknessBakeMaskLayer,
    pub middle: DarknessBakeMaskLayer,
    pub far: DarknessBakeMaskLayer,
    pub ground: DarknessBakeMaskLayer,
}

impl DarknessBakeMaskAtlas {
    pub fn clear_background_layers(&mut self) {
        let ground = std::mem::take(&mut self.ground);
        *self = Self { ground, ..default() };
    }

    /// The layers holding coverage, the ground (most often covering) first.
    fn layers(&self) -> Vec<&DarknessBakeMaskLayer> {
        [&self.ground, &self.near, &self.middle, &self.far].into_iter().filter(|layer| !layer.is_empty()).collect()
    }
}

pub(super) fn reset_bake(mut target: ResMut<DarknessLightMapTarget>, mut masks: ResMut<DarknessBakeMaskAtlas>) {
    target.baked = false;
    *masks = default();
}

pub(super) fn lightmap_size(level: &CompiledLevel) -> UVec2 {
    UVec2::new(level.width, level.height).saturating_mul(UVec2::splat(LIGHTMAP_SCALE)).max(UVec2::ONE)
}

/// Bakes the lightmap once the level's surfaces, and with them all of its coverage, are ready.
pub(super) fn bake_lightmap(
    build: Option<ResMut<GenerationBuild>>,
    level: Option<Res<CompiledLevel>>,
    occupancy: Option<Res<LevelChunkOccupancyMask>>,
    masks: Res<DarknessBakeMaskAtlas>,
    mut target: ResMut<DarknessLightMapTarget>,
    mut images: ResMut<Assets<Image>>,
    device: Res<RenderDevice>,
) {
    let (Some(mut build), Some(level)) = (build, level) else { return; };
    if target.baked || !build.surfaces_ready() {
        return;
    }
    let pages = bake(&level, occupancy.as_deref(), &masks).ok_or_else(|| "Darkness coverage could not be baked".to_owned())
        .and_then(|light| atlas::pack(lightmap_size(&level), &light, device.limits().max_texture_array_layers));
    match pages {
        Ok((pages, table)) => {
            images.insert(&target.handle, pages).expect("lightmap handle remains reserved");
            images.insert(&target.table, table).expect("lightmap page-table handle remains reserved");
            target.baked = true;
        },
        Err(error) => build.fail(error),
    }
}

/// Bakes the lightmap, or `None` for a level without size or coverage. Chunks `occupancy`
/// leaves out of the map hold no sources.
fn bake(level: &CompiledLevel, occupancy: Option<&LevelChunkOccupancyMask>, masks: &DarknessBakeMaskAtlas) -> Option<Vec<u8>> {
    let _timer = crate::levelgen::diagnostics::StageTimer::new("darkness_lightmap_bake");
    let layers = masks.layers();
    let size = lightmap_size(level);
    let pixel_world = level.bounds.size() / size.as_vec2();
    if level.width == 0 || level.height == 0 || pixel_world.min_element() <= 0.0 || layers.is_empty() {
        return None;
    }
    let chunk = mapgen::CHUNK_IMAGE_SIZE * LIGHTMAP_SCALE;
    let occupancy = occupancy.filter(|mask| {
        let matches = mask.matches_chunks(size.x.div_ceil(chunk), size.y.div_ceil(chunk));
        if !matches {
            warn!("darkness: chunk occupancy mask does not match the level; every chunk may hold sources");
        }
        matches
    });
    let mut nearest = DistanceField::default();
    nearest.compute(size.x as usize, size.y as usize, |x, y| {
        let pixel = UVec2::new(x as u32, y as u32);
        let world = Vec2::new(level.bounds.min.x, level.bounds.max.y) + (pixel.as_vec2() + 0.5) * pixel_world * Vec2::new(1.0, -1.0);
        occupancy.is_none_or(|mask| mask.is_occupied(pixel.x / chunk, pixel.y / chunk))
            && !layers.iter().any(|layer| layer.covers(world))
    });
    let light = light_by_distance(pixel_world.min_element());
    Some(nearest.squared.iter().map(|&distance2| light.get(distance2 as usize).copied().unwrap_or(0)).collect())
}

/// Light by squared distance from the nearest source, in lightmap pixels `pixel_world` world
/// units wide, as 8-bit coverage.
fn light_by_distance(pixel_world: f32) -> Vec<u8> {
    let reach = (SOURCE_LIGHT_RADIUS_WORLD / pixel_world).ceil() as usize;
    (0..=reach * reach).map(|distance2| {
        let t = ((distance2 as f32).sqrt() * pixel_world / SOURCE_LIGHT_RADIUS_WORLD).min(1.0);
        ((1.0 - t).powf(SOURCE_LIGHT_FALLOFF_EXPONENT) * 255.0 + 0.5) as u8
    }).collect()
}

#[cfg(test)]
#[path = "bake_tests.rs"]
mod tests;
