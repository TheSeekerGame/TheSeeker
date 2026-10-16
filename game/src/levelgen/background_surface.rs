//! Regional background images with CPU coverage and filtered texture gutters.
use super::*;
use bevy::mesh::VertexAttributeValues;

pub(super) struct BackgroundBake {
    pub patches: Vec<BackgroundPatch>,
    pub size: UVec2,
    pub tile_px: u32,
    pub world_size: Vec2,
}

pub(super) struct BackgroundPatch {
    pub region: URect,
    pub texture_region: URect,
    pub color: Image,
    pub coverage: Vec<u8>,
}

/// The player spawn chunk, left empty so the surface backdrop shows through it.
pub(super) struct SpawnChunkCutout {
    chunk_origin: IVec2,
    chunk_size: u32,
}

impl SpawnChunkCutout {
    pub(super) fn new(level: &CompiledLevel, tile_px: u32) -> Option<Self> {
        let spawn = level.spawns.iter().find(|spawn| spawn.kind == SpawnKind::Player)?;
        let chunk_size = mapgen::CHUNK_IMAGE_SIZE * tile_px;
        Some(Self { chunk_origin: spawn.chunk * chunk_size as i32, chunk_size })
    }

    fn apply(&self, image: &mut Image, region_origin: UVec2) {
        clear_image_rect(image, self.chunk_origin - region_origin.as_ivec2(), UVec2::splat(self.chunk_size));
    }
}

pub(super) fn bake(surface: &surface::TileSurface<'_>, blur: f32, cutout: Option<SpawnChunkCutout>, world_size: Vec2) -> BackgroundBake {
    let regions = surface::chunk_regions(surface.size);
    let mut patches = surface::bake_jobs(&regions, |region| bake_patch(surface, region, blur, cutout.as_ref()));
    patches.sort_by_key(|patch| (patch.region.min.y, patch.region.min.x));
    BackgroundBake { patches, size: surface.size, tile_px: surface.tile_px, world_size }
}

fn bake_patch(surface: &surface::TileSurface<'_>, region: URect, blur: f32, cutout: Option<&SpawnChunkCutout>) -> BackgroundPatch {
    // The output owns one tile region. One extra texel supplies filtering across its edge.
    let support = crate::graphics::background_blur::support(blur) + 3;
    let padded = surface::expand(region, support.div_ceil(surface.tile_px), surface.size);
    let mut color = surface.bake(padded);
    let origin = padded.min * surface.tile_px;
    if let Some(cutout) = cutout { cutout.apply(&mut color, origin); }
    let pixels = URect { min: region.min * surface.tile_px, max: region.max * surface.tile_px };
    let local = URect { min: pixels.min - origin, max: pixels.max - origin };
    let coverage = surface::alpha(&surface::crop(&color, local));
    finalize_background_group_image(&mut color, blur);
    let texture_region = surface::expand(pixels, 1, surface.size * surface.tile_px);
    let mut color = surface::crop(&color, URect { min: texture_region.min - origin, max: texture_region.max - origin });
    color.sampler = if blur > 0.0001 { ImageSampler::linear() } else { ImageSampler::nearest() };
    BackgroundPatch { region, texture_region, color, coverage }
}

/// A background plane's parallax depth, the inverse of the share of camera motion it follows.
pub(super) fn parallax_depth(group: usize) -> f32 {
    1.0 / [0.95, 0.85, 0.75][group.min(2)]
}

#[expect(clippy::too_many_arguments, reason = "Publication owns Bevy asset stores and the shared coverage atlas.")]
pub(super) fn publish(commands: &mut Commands, root: Entity, group: usize, bake: BackgroundBake, center: Vec2,
    masks: Option<&mut DarknessBakeMaskAtlas>, images: &mut Assets<Image>, meshes: &mut Assets<Mesh>, materials: &mut Assets<ColorMaterial>,
) {
    let depth = parallax_depth(group);
    let mut coverage = Vec::with_capacity(bake.patches.len());
    for patch in bake.patches {
        let pixels = URect { min: patch.region.min * bake.tile_px, max: patch.region.max * bake.tile_px };
        coverage.push((pixels.min, pixels.width(), pixels.height(), patch.coverage));
        if !surface::visible(&patch.color) { continue; }
        let mut mesh = Mesh::from(Rectangle::default());
        let uv_min = (pixels.min - patch.texture_region.min).as_vec2() / patch.texture_region.size().as_vec2();
        let uv_scale = pixels.size().as_vec2() / patch.texture_region.size().as_vec2();
        inset_uvs(&mut mesh, uv_min, uv_scale);
        let size = patch.region.size().as_vec2() / bake.size.as_vec2() * bake.world_size;
        let local_center = (patch.region.min.as_vec2() + patch.region.size().as_vec2() * 0.5) / bake.size.as_vec2();
        let position = center + (local_center - Vec2::splat(0.5)) * bake.world_size * Vec2::new(1.0, -1.0);
        let texture = images.add(patch.color);
        commands.spawn((Mesh2d(meshes.add(mesh)), bevy::sprite_render::MeshMaterial2d(materials.add(ColorMaterial::from(texture))),
            Transform::from_xyz(position.x, position.y, -(group as f32 + 1.0)).with_scale(size.extend(1.0)),
            RenderLayers::layer(0), crate::parallax::Parallax { depth }, crate::parallax::ParallaxOffset(center - position), ChildOf(root), StateDespawnMarker,
            Name::new(format!("Background{}Chunk", group + 1))));
    }
    if let Some(masks) = masks {
        let layer = match group { 0 => &mut masks.near, 1 => &mut masks.middle, _ => &mut masks.far };
        layer.set_chunks(bake.size * bake.tile_px, coverage, center, bake.world_size, depth);
    }
}

fn inset_uvs(mesh: &mut Mesh, min: Vec2, scale: Vec2) {
    let Some(VertexAttributeValues::Float32x2(uvs)) = mesh.attribute_mut(Mesh::ATTRIBUTE_UV_0) else { return; };
    for uv in uvs { *uv = (min + Vec2::from_array(*uv) * scale).to_array(); }
}
