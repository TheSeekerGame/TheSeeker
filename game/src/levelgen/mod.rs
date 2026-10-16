//! PNG-driven level generation (scaffolding)
//!
//! Minimal plugin that loads a 96x96 level PNG, compiles spawns and bounds,
//! and stores them in a resource for later systems to consume.

use theseeker_engine::physics::{
    Collider, CollisionGroups, CoverCollisionTiles, Group, StaticCollider, TileCollisionWorld, ENEMY,
};
use crate::game::GameOver;
use crate::game::combat::blood::BloodForegroundOcclusion;
use crate::game::player::CraftMaterial;
use crate::graphics::background_blur::{
    apply_background_blur, BackgroundBlurSettings,
};
use crate::graphics::pixel_perfect::pixel_perfect_layers;
use crate::graphics::post_processing::darkness::{
    DarknessBakeMaskAtlas,
};
use crate::prelude::*;
use bevy::ecs::{
    schedule::{ApplyDeferred, SystemSet},
    system::SystemParam,
};
use bevy::image::ImageSampler;
use bevy::camera::visibility::RenderLayers;
use bevy::sprite::Anchor;
use bevy::sprite_render::ColorMaterial;
use rand::Rng;

use std::collections::{HashMap, HashSet};
use std::fs;
use std::marker::PhantomData;
use std::path::Path;

pub mod breakable_wall;
pub mod compiler;
pub(crate) mod distance;
mod ground_shading;
mod surface;
mod background_surface;
pub(crate) mod surface_backdrop;
pub(crate) mod impact_crater;
mod background_bank;
pub(crate) mod pipeline;
mod adaptation;
mod publication;
mod spatial;
mod background;
use background::*;
mod background_openings;
mod background_synthesis;
mod world_spawn;
use world_spawn::*;
mod image_ops;
use image_ops::*;
mod terrain;
use terrain::*;
mod catalog;
use catalog::*;
mod mosaic;
use mosaic::*;
mod placement;
use placement::*;
pub(crate) use placement::placement_area;
mod hash;
use hash::*;
#[cfg(test)]
mod surface_tests;
pub(crate) mod diagnostics;
pub(crate) mod foliage;
pub mod harvestable_materials;
pub mod ldtk_rules;
pub mod mapgen;
mod optional_spikes;
mod secret_covers;
mod secret_rooms;

pub(crate) const PLAYER_ACTIVE_CHUNK_RADIUS: i32 = 2;

pub(crate) const GROUND_TILE_ART_SIZE_PX: u32 = 6;
const GROUND_DARKEN_START_PX: i32 = 1;
const GROUND_DARKEN_RANGE_PX: f32 = 50.0;
const GROUND_DARKEN_CURVE: f32 = 1.35;

#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
struct LevelgenBuildSet;

#[derive(SystemParam)]
struct GroundParams<'w, 's> {
    compiled: Option<Res<'w, CompiledLevel>>,
    catalog: ResMut<'w, PreparedCatalog>,
    root: Option<Res<'w, LevelRootEntity>>,
    biomes: Option<Res<'w, LevelBiomeMask>>,
    chunk_colors: Option<Res<'w, LevelChunkColorMask>>,
    occupancy: Option<Res<'w, LevelChunkOccupancyMask>>,
    libraries: Res<'w, GenerationLibraries>,
    cfg: Res<'w, LevelPngConfig>,
    images: ResMut<'w, Assets<Image>>,
    meshes: ResMut<'w, Assets<Mesh>>,
    materials: ResMut<'w, Assets<ColorMaterial>>,
    foliage_materials: ResMut<'w, Assets<foliage::FoliageMaterial>>,
    foliage_settings: Res<'w, foliage::FoliageSettings>,
    darkness_masks: Option<ResMut<'w, DarknessBakeMaskAtlas>>,
    ldtk_data: Res<'w, ldtk_rules::LdtkRuleData>,
    secret_rooms_lib: Res<'w, secret_rooms::SecretRoomLibrary>,
    pre_impact: Option<Res<'w, impact_crater::PreImpactTerrain>>,
    built_flag: ResMut<'w, pipeline::GenerationBuild>,
    _marker: PhantomData<&'s ()>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum BiomeKind {
    #[default]
    Ice,
    Tundra,
}

impl BiomeKind {
    pub fn is_tundra(self) -> bool {
        matches!(self, BiomeKind::Tundra)
    }
}

#[derive(Resource, Clone)]
pub struct LevelBiomeMask {
    pub width: u32,
    pub height: u32,
    pub cells: Vec<BiomeKind>,
    /// Optional per-tile mask marking the “inner” tundra ring that also swaps entities/decorations
    pub tundra_inner: Vec<bool>,
}

impl LevelBiomeMask {
    pub fn matches_dims(&self, width: u32, height: u32) -> bool {
        self.width == width
            && self.height == height
            && self.cells.len() == (width * height) as usize
            && (self.tundra_inner.is_empty()
                || self.tundra_inner.len() == (width * height) as usize)
    }
}

#[derive(Resource, Clone)]
pub struct LevelChunkColorMask {
    pub chunk_size: u32,
    pub chunks_w: u32,
    pub chunks_h: u32,
    /// Row-major chunk colors (RGB) in map pixel space; y increases downward
    pub colors: Vec<[u8; 3]>,
}

impl LevelChunkColorMask {
    pub fn matches_chunks(&self, chunks_w: u32, chunks_h: u32) -> bool {
        self.chunks_w == chunks_w
            && self.chunks_h == chunks_h
            && self.colors.len() == (chunks_w * chunks_h) as usize
    }

    pub fn color_for_tile(&self, tile_x: u32, tile_y: u32) -> Option<[u8; 3]> {
        if self.chunk_size == 0 {
            return None;
        }
        let chunk_x = tile_x / self.chunk_size;
        let chunk_y = tile_y / self.chunk_size;
        self.color_for_chunk(chunk_x, chunk_y)
    }

    pub fn color_for_chunk(
        &self,
        chunk_x: u32,
        chunk_y: u32,
    ) -> Option<[u8; 3]> {
        if chunk_x >= self.chunks_w || chunk_y >= self.chunks_h {
            return None;
        }
        let idx = (chunk_y * self.chunks_w + chunk_x) as usize;
        self.colors.get(idx).copied()
    }
}

#[derive(Resource, Clone)]
pub struct LevelChunkOccupancyMask {
    pub chunks_w: u32,
    pub chunks_h: u32,
    /// Row-major chunk occupancy in map pixel space; y increases downward
    pub occupied: Vec<bool>,
}

impl LevelChunkOccupancyMask {
    pub fn matches_chunks(&self, chunks_w: u32, chunks_h: u32) -> bool {
        self.chunks_w == chunks_w
            && self.chunks_h == chunks_h
            && self.occupied.len() == (chunks_w * chunks_h) as usize
    }

    pub fn is_occupied(&self, chunk_x: u32, chunk_y: u32) -> bool {
        if chunk_x >= self.chunks_w || chunk_y >= self.chunks_h {
            return false;
        }
        let idx = (chunk_y * self.chunks_w + chunk_x) as usize;
        self.occupied.get(idx).copied().unwrap_or(false)
    }
}

#[derive(Resource, Clone, PartialEq)]
pub struct LevelPngConfig {
    pub path: String,
    pub tile_size: f32,
    /// Directory for decoration rules and sprites
    pub decorations_dir: String,
    /// Directory for tundra decoration rules and sprites
    pub tundra_decorations_dir: String,
    /// Directory for entity rules and sprites
    pub entities_dir: String,
    /// Directory for tundra entity rules and sprites
    pub tundra_entities_dir: String,
    /// Directories for mosaic layers (mosaic1..mosaic3)
    pub mosaic_dirs: [String; 3],
    /// Directories holding each biome's near-background exemplar (`exemplar.png`).
    pub near_bg_dir: String,
    pub tundra_near_bg_dir: String,
    /// Path to LDtk project file (relative to assets directory)
    pub ldtk_rules_path: String,
}

impl Default for LevelPngConfig {
    fn default() -> Self {
        Self {
            path: "levels/level_test.png".into(),
            tile_size: 6.0,
            decorations_dir: "levels/tiles/ice_biome/decorations".into(),
            tundra_decorations_dir: "levels/tiles/tundra_biome/decorations"
                .into(),
            entities_dir: "levels/tiles/ice_biome/entities".into(),
            tundra_entities_dir: "levels/tiles/tundra_biome/entities".into(),
            mosaic_dirs: [
                "levels/tiles/ice_biome/main_platforms/mosaic1".into(),
                "levels/tiles/ice_biome/main_platforms/mosaic2".into(),
                "levels/tiles/ice_biome/main_platforms/mosaic3".into(),
            ],
            near_bg_dir: "levels/tiles/ice_biome/near_bg".into(),
            tundra_near_bg_dir: "levels/tiles/tundra_biome/near_bg".into(),
            ldtk_rules_path: "levels/tiles/ldtk_rules/level.01_test.ldtk"
                .into(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SpawnKind {
    Player,
    VagrantYakPair,
    Collector,
    Monk,
    Exiled,
    Capsule,
}

#[derive(Clone, Debug)]
pub struct SpawnSpec {
    pub kind: SpawnKind,
    pub position: Vec2,
    pub chunk: IVec2,
}

#[derive(Resource, Default, Clone)]
pub struct CompiledLevel {
    pub spawns: Vec<SpawnSpec>,
    pub bounds: Rect,
    pub width: u32,
    pub height: u32,
    /// Row-major terrain occupancy (true = terrain present)
    pub terrain: Vec<bool>,
    /// Row-major spike occupancy (true = spike present)
    pub spike_tiles: Vec<bool>,
    /// Row-major optional spike markers, represented as terrain until activated.
    pub optional_spike_tiles: Vec<bool>,
    /// Row-major manual near-background mask (#5b666f in PNG)
    pub manual_near_bg_mask: Vec<bool>,
    /// Tile positions marked with #4dfbff for harvestable ore deposits.
    pub ore_spawn_tiles: Vec<IVec2>,
}

impl CompiledLevel {
    /// The world size of one of the level's tiles.
    pub fn tile_size(&self) -> f32 {
        self.bounds.width() / self.width as f32
    }
}

#[derive(Resource)]
pub struct LevelPngHandle(pub Handle<Image>);

/// Clear level runtime caches so subsequent loads rebuild
pub fn reset_level_runtime(commands: &mut Commands) {
    crate::game::enemy::reset_enemy_spawn_runtime(commands);
    commands.remove_resource::<CompiledLevel>();
    commands.remove_resource::<LevelPngHandle>();
    commands.remove_resource::<LevelRootEntity>();
    commands.remove_resource::<LevelBiomeMask>();
    commands.remove_resource::<LevelChunkColorMask>();
    commands.remove_resource::<LevelChunkOccupancyMask>();
    commands.remove_resource::<optional_spikes::OptionalSpikeRuntime>();
    commands.remove_resource::<impact_crater::PreImpactTerrain>();
    commands.insert_resource(TileCollisionWorld::default());
    commands.insert_resource(pipeline::GenerationBuild::default());
    commands.insert_resource(publication::Publication::default());
}

pub(crate) fn sync_tile_collision_world(
    compiled: Option<Res<CompiledLevel>>,
    mut collision_world: ResMut<TileCollisionWorld>,
    cover_tiles: Query<&CoverCollisionTiles>,
    added_cover_tiles: Query<&CoverCollisionTiles, Added<CoverCollisionTiles>>,
) {
    let Some(compiled) = compiled else {
        if collision_world.is_ready() {
            collision_world.clear();
        }
        return;
    };

    if compiled.width == 0 || compiled.height == 0 {
        collision_world.clear();
        return;
    }
    let tile_size = compiled.tile_size();
    if tile_size <= 0.0 {
        collision_world.clear();
        return;
    }

    let needs_rebuild = !collision_world.is_ready()
        || collision_world.width != compiled.width
        || collision_world.height != compiled.height
        || (collision_world.tile_size - tile_size).abs() > f32::EPSILON
        || compiled.is_changed();

    let mut rebuilt = false;
    if needs_rebuild {
        collision_world.rebuild(
            compiled.width,
            compiled.height,
            tile_size,
            &compiled.terrain,
        );
        rebuilt = true;
    }
    if rebuilt {
        for cover in cover_tiles.iter() {
            collision_world.add_dynamic_tiles(&cover.tiles);
        }
    } else if collision_world.is_ready() {
        for cover in added_cover_tiles.iter() {
            collision_world.add_dynamic_tiles(&cover.tiles);
        }
    }
}

pub struct PngLevelPlugin;

impl Plugin for PngLevelPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(diagnostics::GenerationDiagnosticsPlugin);
        app.init_resource::<LevelPngConfig>();
        app.add_plugins(publication::PublicationPlugin);
        app.init_resource::<pipeline::GenerationBuild>();
        app.init_resource::<PreparedCatalog>();
        app.add_systems(Update, invalidate_catalog.before(build_ground_layer));
        app.add_systems(PostUpdate, pipeline::publish_ready);
        app.add_systems(Update, pipeline::report_asset_failures);
        app.init_resource::<GenerationLibraries>();
        app.init_resource::<mapgen::ChunkLibrary>();
        app.init_resource::<mapgen::MapLibrary>();
        app.init_resource::<ldtk_rules::LdtkRuleData>();
        app.init_resource::<secret_rooms::SecretRoomLibrary>();
        app.init_resource::<TileCollisionWorld>();
        app.add_plugins((
            breakable_wall::BreakableWallPlugin,
            foliage::FoliagePlugin,
            harvestable_materials::HarvestableMaterialsPlugin,
        ));
        app.configure_sets(Update, LevelgenBuildSet);
        app.add_systems(
            OnEnter(AppState::InGame),
            (
                begin_load_level_png,
                load_generation_libraries,
                load_ldtk_rules,
                secret_rooms::load_secret_rooms,
            ),
        );
        app.add_systems(Update, compile_level_png);
        app.add_systems(
            Update,
            spawn_compiled_level.run_if(in_state(AppState::InGame)),
        );
        app.add_systems(
            Update,
            optional_spikes::activate_optional_spikes
                .before(sync_tile_collision_world)
                .in_set(GameTickSet::Pre)
                .run_if(in_state(AppState::InGame)),
        );
        app.add_systems(
            Update,
            sync_tile_collision_world
                .in_set(GameTickSet::Pre)
                .run_if(in_state(AppState::InGame)),
        );
        app.add_systems(
            Update,
            build_ground_layer.in_set(LevelgenBuildSet),
        );
        app.add_systems(
            Update,
            build_background_layers.after(LevelgenBuildSet),
        );
        app.add_systems(
            Update,
            surface_backdrop::spawn_surface_backdrop
                .after(build_background_layers)
                .run_if(in_state(AppState::InGame)),
        );
        app.add_systems(
            PostUpdate,
            surface_backdrop::place_surface_layers
                .before(bevy::transform::TransformSystems::Propagate)
                .run_if(in_state(AppState::InGame)),
        );
        app.add_systems(
            Update,
            ApplyDeferred
                .after(LevelgenBuildSet)
                .before(GameTickSet::Pre),
        );

    }
}

fn begin_load_level_png(
    mut commands: Commands,
    ass: Res<AssetServer>,
    cfg: Res<LevelPngConfig>,
    existing_handle: Option<Res<LevelPngHandle>>,
) {
    if existing_handle.is_some() {
        return;
    }
    if cfg.path.starts_with(mapgen::GENERATED_LEVEL_PREFIX) { return; }
    let handle: Handle<Image> = ass.load(cfg.path.clone());
    commands.insert_resource(LevelPngHandle(handle));
}

fn compile_level_png(
    mut commands: Commands,
    handle: Option<Res<LevelPngHandle>>,
    cfg: Option<Res<LevelPngConfig>>,
    images: Res<Assets<Image>>,
    existing: Option<Res<CompiledLevel>>,
) {
    if existing.is_some() {
        return;
    }
    let (Some(handle), Some(cfg)) = (handle, cfg) else {
        return;
    };
    let Some(image) = images.get(&handle.0) else {
        return;
    };
    let compiled = compiler::compile_level_image(image, &cfg);
    info!(
        "Compiled PNG level: spawns={}, bounds={:?}",
        compiled.spawns.len(),
        compiled.bounds
    );
    commands.insert_resource(compiled);
}

#[derive(Resource)]
struct LevelRootEntity(Entity);

/// Whether the LDtk project and its tileset images have loaded.
fn ldtk_ready(ldtk: &ldtk_rules::LdtkRuleData, images: &Assets<Image>) -> bool {
    ldtk.loaded && ldtk.tileset_images.values().all(|handle| images.contains(handle))
}

fn build_ground_layer(mut commands: Commands, params: GroundParams) {
    let GroundParams {
        compiled,
        mut catalog,
        root,
        biomes,
        chunk_colors,
        occupancy,
        libraries,
        cfg,
        mut images,
        mut meshes,
        mut materials,
        mut foliage_materials,
        foliage_settings,
        mut darkness_masks,
        ldtk_data,
        secret_rooms_lib,
        pre_impact,
        mut built_flag,
        _marker: _,
    } = params;
    if built_flag.phase != pipeline::BuildPhase::Assets {
        return;
    }
    let Some(compiled) = compiled else { return; };
    let root_e = match root {
        Some(r) => r.0,
        None => return,
    };
    if !ldtk_ready(&ldtk_data, &images) || !libraries.loaded(&images) {
        return;
    }

    let biome_mask = biomes.as_ref().and_then(|b| {
        if b.matches_dims(compiled.width, compiled.height) {
            Some(b.as_ref())
        } else {
            warn!(
                "Biome mask dimensions {}x{} do not match compiled level {}x{}",
                b.width, b.height, compiled.width, compiled.height
            );
            None
        }
    });
    let chunk_colors = chunk_colors.as_ref().and_then(|c| {
        if c.chunk_size == 0 {
            warn!("Chunk color mask chunk_size is zero; ignoring chunk color constraints");
            return None;
        }
        if compiled.width % c.chunk_size != 0
            || compiled.height % c.chunk_size != 0
        {
            warn!(
                "Chunk color mask chunk_size {} does not divide level {}x{}",
                c.chunk_size, compiled.width, compiled.height
            );
            return None;
        }
        let expected_w = compiled.width / c.chunk_size;
        let expected_h = compiled.height / c.chunk_size;
        if c.matches_chunks(expected_w, expected_h) {
            Some(c.as_ref())
        } else {
            warn!(
                "Chunk color mask dims {}x{} do not match expected {}x{}",
                c.chunks_w, c.chunks_h, expected_w, expected_h
            );
            None
        }
    });

    if catalog.configuration.as_ref() != Some(&cfg) {
        catalog.data = None;
        catalog.configuration = Some(cfg.clone());
    }
    let catalog = catalog.data.get_or_insert_with(|| std::sync::Arc::new(GenerationCatalog {
        libraries: libraries.clone(), ldtk: ldtk_data.clone(),
    }));
    ensure_ground(
        &mut commands,
        root_e,
        &compiled,
        &cfg,
        &catalog.libraries,
        &catalog.ldtk,
        &secret_rooms_lib,
        &mut images,
        &mut meshes,
        &mut materials,
        &mut foliage_materials,
        *foliage_settings,
        darkness_masks.as_deref_mut(),
        biome_mask,
        chunk_colors,
        occupancy.as_deref(),
        pre_impact.as_deref(),
    );
    built_flag.phase = pipeline::BuildPhase::Ground;
}

fn spike_tile_coord(compiled: &CompiledLevel, idx: usize) -> IVec2 {
    IVec2::new(
        (idx as u32 % compiled.width) as i32,
        (idx as u32 / compiled.width) as i32,
    )
}

fn spike_tile_world_center(compiled: &CompiledLevel, tile_size: f32, tile: IVec2) -> Vec2 {
    let x = (tile.x as f32 + 0.5) * tile_size;
    let y_img = (tile.y as f32 + 0.5) * tile_size;
    let y = (compiled.height as f32 * tile_size) - y_img;
    Vec2::new(x, y)
}

fn prepare_spike_hazard(commands: &mut Commands, parent: Entity, center: Vec2, half: f32, tile: IVec2, active: bool) -> Entity {
    let entity = commands.spawn((
        Transform::from_translation(Vec3::new(center.x, center.y, 6.0)), GlobalTransform::default(),
        StateDespawnMarker, ChildOf(parent), Name::new(format!("SpikeHazard({}, {})", tile.x, tile.y)),
        theseeker_engine::physics::Collider::cuboid(half, half), StaticCollider,
        theseeker_engine::physics::Sensor, adaptation::hazard_groups(active), crate::game::spikes::SpikeHazard,
    )).id();
    commands.entity(entity).insert(crate::game::combat::DamageSource::new(
        u32::MAX, entity, crate::game::spikes::SPIKE_DAMAGE).with_max_targets(1));
    entity
}

fn spawn_spike_hazard_at_tile(commands: &mut Commands, parent: Entity, compiled: &CompiledLevel,
    tile_size: f32, half: f32, tile: IVec2,
) {
    prepare_spike_hazard(commands, parent, spike_tile_world_center(compiled, tile_size, tile), half, tile, true);
}

fn spawn_spike_hazards(
    commands: &mut Commands,
    parent: Entity,
    compiled: &CompiledLevel,
) {
    if compiled.spike_tiles.is_empty()
        || !compiled.spike_tiles.iter().any(|&b| b)
    {
        return;
    }

    let tile = compiled.tile_size();
    let half = tile * 0.3;

    for idx in 0..(compiled.width * compiled.height) as usize {
        if !compiled.spike_tiles.get(idx).copied().unwrap_or(false) {
            continue;
        }

        let tile_coord = spike_tile_coord(compiled, idx);
        spawn_spike_hazard_at_tile(commands, parent, compiled, tile, half, tile_coord);
    }
}
