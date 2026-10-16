mod atlas;
mod bake;
pub use bake::DarknessBakeMaskAtlas;
#[cfg(test)]
mod tests;
use crate::game::player::foreground::ActionTag;
use crate::graphics::fullscreen::{self, FullscreenPass, FullscreenView};
use bevy::camera::Projection;
use bevy::render::extract_component::{
    ComponentUniforms, ExtractComponent, ExtractComponentPlugin,
    UniformComponentPlugin,
};
use bevy::render::extract_resource::{ExtractResource, ExtractResourcePlugin};
use bevy::render::render_asset::RenderAssets;
use bevy::render::render_resource::binding_types::{texture_2d, texture_2d_array, uniform_buffer};
use bevy::render::render_resource::{
    BindGroupEntries, FilterMode, MipmapFilterMode, Sampler, SamplerDescriptor, ShaderType,
    TextureDimension, TextureFormat, TextureSampleType, TextureViewDimension,
};
use bevy::render::renderer::{ViewQuery, RenderContext, RenderDevice};
use bevy::render::texture::GpuImage;
use bevy::render::RenderApp;
use std::sync::Arc;

use crate::camera::{update_camera, MainCamera};
use crate::game::player::{Elevator, Player};
use crate::levelgen::{
    mapgen, CompiledLevel, pipeline::GenerationBuild, LevelChunkOccupancyMask,
    SpawnKind,
};
use crate::prelude::*;

pub(crate) struct DarknessPlugin;

/// Height of the open surface's ground above the capsule anchor.
const DARKNESS_SURFACE_OFFSET_WORLD: f32 = 50.0;
/// Chunks below the surface that stay free of depth darkness.
const DARKNESS_FREE_DEPTH_CHUNKS: f32 = 1.0;
/// Chunks over which depth darkness eases in from zero to its base strength.
const DARKNESS_RAMP_DEPTH_CHUNKS: f32 = 1.0;
/// Fullscreen darkness opacity at the end of the ramp.
const DARKNESS_DEPTH_BASE_STRENGTH: f32 = 0.4;
/// Fullscreen darkness opacity gained per downward world unit below the ramp.
const DARKNESS_DEPTH_STRENGTH_PER_WORLD_PX: f32 = 0.2 / 2000.0;
/// Player-following local backlight radius in world units.
const PLAYER_BACKLIGHT_RADIUS_WORLD: f32 = 48.0;
/// Player-following local backlight strength (0..1).
const PLAYER_BACKLIGHT_STRENGTH: f32 = 0.75;
/// Player-following local backlight falloff exponent (>1 = faster early falloff).
const PLAYER_BACKLIGHT_FALLOFF_EXPONENT: f32 = 2.0;

impl Plugin for DarknessPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<DarknessLightMapTarget>();
        app.init_resource::<DarknessBakeMaskAtlas>();

        app.add_plugins((
            ExtractComponentPlugin::<DarknessSettings>::default(),
            UniformComponentPlugin::<DarknessSettings>::default(),
            ExtractResourcePlugin::<DarknessLightMapTarget>::default(),
        ));

        app.register_type::<DarknessSettings>();
        app.register_required_components::<MainCamera, DarknessSettings>();

        app.add_systems(OnEnter(AppState::InGame), bake::reset_bake);

        app.add_systems(
            Update,
            (bake::bake_lightmap, sync_darkness_settings)
                .chain()
                .after(update_camera)
                .in_set(GameTickSet::Post)
                .run_if(in_state(AppState::InGame)),
        );

    }

    fn finish(&self, app: &mut App) {
        let Some(render_app) = app.get_sub_app_mut(RenderApp) else {
            return;
        };

        render_app.add_systems(bevy::render::RenderStartup, |world: &mut World| { world.init_resource::<DarknessPipeline>(); });
    }
}

/// The baked lightmap's atlas pages and page table, and whether they hold this level's bake.
#[derive(Resource, Clone)]
pub struct DarknessLightMapTarget {
    pub handle: Handle<Image>,
    pub table: Handle<Image>,
    baked: bool,
}

impl ExtractResource for DarknessLightMapTarget {
    type Source = DarknessLightMapTarget;

    fn extract_resource(source: &Self::Source) -> Self {
        source.clone()
    }
}

impl FromWorld for DarknessLightMapTarget {
    fn from_world(world: &mut World) -> Self {
        let mut images = world.resource_mut::<Assets<Image>>();
        let (pages, table) = atlas::pack(UVec2::ONE, &[0], 1).expect("empty darkness atlas");
        let handle = images.add(pages);
        let table = images.add(table);
        Self { handle, table, baked: false }
    }
}

/// What the darkness pass reads, laid out as `DarknessSettings` in `darkness.wgsl`. Positions
/// and sizes are in world units. The default has no depth ramp, so it darkens nothing.
#[derive(Component, Clone, Copy, Default, ExtractComponent, ShaderType, Reflect)]
#[extract_component_sync_target((Self, bevy::render::extract_component::DynamicUniformIndex<Self>))]
pub struct DarknessSettings {
    /// The bottom-left corner of the camera's view.
    view_min: Vec2,
    view_size: Vec2,
    /// The bottom-left corner of the level, which the lightmap spans.
    level_min: Vec2,
    level_size: Vec2,
    /// The lightmap's size in pixels.
    lightmap_size: Vec2,
    /// The edge of the square atlas pages that hold the lightmap, in pixels.
    lightmap_page_edge: f32,
    /// The world y where depth darkness begins.
    depth_start: f32,
    /// How far below its start depth darkness eases in, to `depth_ramp_strength`; without a
    /// ramp there is none.
    depth_ramp: f32,
    depth_ramp_strength: f32,
    /// The strength depth darkness gains per world unit below its ramp.
    depth_strength_per_unit: f32,
    player_light_center: Vec2,
    player_light_radius: f32,
    /// The share of darkness the player's backlight lifts at its center; zero turns it off.
    player_light_strength: f32,
    /// How quickly the backlight fades toward its radius (1 is linear).
    player_light_falloff: f32,
}

impl DarknessSettings {
    /// Settings for a camera at `center` whose projection spans `view` around it, in `level`,
    /// with the player's backlight at `player`, if it is lit.
    fn new(center: Vec2, view: Rect, level: Option<&CompiledLevel>, player: Option<Vec2>) -> Self {
        let bounds = level.map_or_else(Rect::default, |level| level.bounds);
        let (depth_start, depth_ramp) = level.and_then(depth_darkness).unwrap_or_default();
        Self {
            view_min: center + view.min,
            view_size: view.size(),
            level_min: bounds.min,
            level_size: bounds.size(),
            lightmap_size: level.map_or(UVec2::ONE, bake::lightmap_size).as_vec2(),
            lightmap_page_edge: atlas::PAGE_EDGE as f32,
            depth_start,
            depth_ramp,
            depth_ramp_strength: DARKNESS_DEPTH_BASE_STRENGTH,
            depth_strength_per_unit: DARKNESS_DEPTH_STRENGTH_PER_WORLD_PX,
            player_light_center: player.unwrap_or_default(),
            player_light_radius: PLAYER_BACKLIGHT_RADIUS_WORLD,
            player_light_strength: player.map_or(0.0, |_| PLAYER_BACKLIGHT_STRENGTH),
            player_light_falloff: PLAYER_BACKLIGHT_FALLOFF_EXPONENT,
        }
    }
}

/// Where depth darkness starts and the ramp over which it eases in, both measured in chunks
/// from the surface above the capsule. A level without a capsule has none.
fn depth_darkness(level: &CompiledLevel) -> Option<(f32, f32)> {
    let capsule = level.spawns.iter().find(|spawn| spawn.kind == SpawnKind::Capsule)?;
    let chunk = level.tile_size() * mapgen::CHUNK_IMAGE_SIZE as f32;
    let surface = capsule.position.y + DARKNESS_SURFACE_OFFSET_WORLD;
    Some((surface - DARKNESS_FREE_DEPTH_CHUNKS * chunk, DARKNESS_RAMP_DEPTH_CHUNKS * chunk))
}

fn sync_darkness_settings(
    level: Option<Res<CompiledLevel>>,
    player: Query<(&GlobalTransform, Has<ActionTag<Elevator>>), With<Player>>,
    mut cameras: Query<(&mut DarknessSettings, &Transform, &Projection), With<MainCamera>>,
) {
    // The backlight goes out while the player rides an elevator.
    let player = player.iter().next().filter(|&(_, riding)| !riding).map(|(transform, _)| transform.translation().truncate());
    for (mut settings, transform, projection) in &mut cameras {
        if let Projection::Orthographic(ortho) = projection {
            *settings = DarknessSettings::new(transform.translation.truncate(), ortho.area, level.as_deref(), player);
        }
    }
}

pub(crate) fn darkness_pass(world: &World, view: ViewQuery<FullscreenView, With<DarknessSettings>>, mut render_context: RenderContext) {
    let target = world.resource::<DarknessLightMapTarget>();
    let images = world.resource::<RenderAssets<GpuImage>>();
    let settings = world.resource::<ComponentUniforms<DarknessSettings>>().uniforms().binding();
    let (Some(settings), Some(lightmap), Some(pages)) = (settings, images.get(&target.handle), images.get(&target.table)) else {
        return;
    };
    let pipeline = world.resource::<DarknessPipeline>();
    pipeline.pass.draw(world, &mut render_context, view.into_inner(), |image| {
        BindGroupEntries::sequential((image, &lightmap.texture_view, &pipeline.lightmap_sampler, settings, &pages.texture_view))
    }, &[]);
}

#[derive(Resource)]
struct DarknessPipeline {
    pass: FullscreenPass,
    lightmap_sampler: Sampler,
}

impl FromWorld for DarknessPipeline {
    fn from_world(world: &mut World) -> Self {
        let pass = FullscreenPass::new(world, "darkness", "shaders/post_processing/darkness.wgsl", (
            fullscreen::texture(),
            texture_2d_array(TextureSampleType::Float { filterable: true }),
            fullscreen::sampler(),
            uniform_buffer::<DarknessSettings>(false),
            texture_2d(TextureSampleType::Uint),
        ), None);
        let lightmap_sampler = world.resource::<RenderDevice>().create_sampler(&SamplerDescriptor {
            label: Some("darkness_lightmap"),
            mag_filter: FilterMode::Linear,
            min_filter: FilterMode::Linear,
            mipmap_filter: MipmapFilterMode::Linear,
            ..default()
        });
        Self { pass, lightmap_sampler }
    }
}

pub(crate) fn bake_ready(world: &World) -> bool {
    world.get_resource::<DarknessLightMapTarget>().is_some_and(|target| target.baked)
}
