//! Sky and distant landscape seen above the player spawn chunk.
//!
//! `tools/surface_art/generate.py` generates one image per layer, each exactly one chunk wide.
//! A layer's parallax is measured from the player spawn, so the resting capsule view shows
//! the artwork as authored. Below that view the layers stay fixed to the world; they never
//! sink into the caves under the surface.
use super::*;
use bevy::image::{ImageLoaderSettings, ImageSampler};
use crate::camera::MainCamera;
use crate::graphics::post_processing::foreground_fog::OpenSkyRegion;

/// Spawn-chunk rows between which foreground fog fades out toward the open sky:
/// fully present below the ground line, gone just above the crater rim.
const FOG_CLEAR_ROWS: (f32, f32) = (560.0, 490.0);

struct LayerSpec {
    image: &'static str,
    /// Shows only once the impact has formed the crater.
    formed_crater: bool,
    /// Spawn-chunk pixel row of the image's top edge (y downward from the chunk top).
    top_row: f32,
    /// Distance factor: 1 moves with the world; larger values move less.
    depth: f32,
    z: f32,
}

/// Back to front. Top rows must match `LAYER_TOPS` in the generator. The cave backgrounds
/// parallax around the level center, which shifts their content under the spawn chunk; the
/// opaque sky covers the middle and far ones (z -2 and -3), and the crater's far wall covers
/// the near one (z -1) where it shows through the rail shaft.
const LAYERS: [LayerSpec; 5] = [
    LayerSpec { image: "levels/surface/Sky.png", formed_crater: false, top_row: -80.0, depth: 4.0, z: -1.9 },
    LayerSpec { image: "levels/surface/Clouds.png", formed_crater: false, top_row: -440.0, depth: 2.0, z: -1.8 },
    LayerSpec { image: "levels/surface/FarRange.png", formed_crater: false, top_row: 400.0, depth: 1.6, z: -1.7 },
    LayerSpec { image: "levels/surface/MidHills.png", formed_crater: false, top_row: 440.0, depth: 1.25, z: -1.6 },
    LayerSpec { image: "levels/surface/CraterRim.png", formed_crater: true, top_row: 470.0, depth: 1.05, z: -0.5 },
];

#[derive(Component)]
pub(crate) struct SurfaceLayer {
    depth: f32,
    /// Top-left corner when the camera rests at `pivot`.
    rest_position: Vec2,
    pivot: Vec2,
}

/// Handles for every backdrop image, so a caller can wait until the backdrop is drawable.
#[derive(Resource, Default)]
pub(crate) struct SurfaceBackdropImages(pub Vec<Handle<Image>>);

pub(super) fn spawn_surface_backdrop(
    mut commands: Commands,
    compiled: Option<Res<CompiledLevel>>,
    root: Option<Res<LevelRootEntity>>,
    asset_server: Res<AssetServer>,
    mut spawned_for_root: Local<Option<Entity>>,
) {
    let (Some(compiled), Some(root)) = (compiled, root) else {
        return;
    };
    if *spawned_for_root == Some(root.0) {
        return;
    }
    let Some(spawn) = compiled.spawns.iter().find(|spawn| spawn.kind == SpawnKind::Player) else {
        return;
    };
    *spawned_for_root = Some(root.0);

    let chunk_px = compiled.tile_size() * mapgen::CHUNK_IMAGE_SIZE as f32;
    let chunk_top_left = Vec2::new(
        spawn.chunk.x as f32 * chunk_px,
        compiled.bounds.max.y - spawn.chunk.y as f32 * chunk_px,
    );
    let mut handles = Vec::with_capacity(LAYERS.len());
    for layer in &LAYERS {
        // Smoothly shaded like the cave backgrounds, so filtered rather than pixel-sampled.
        let image = asset_server.load_builder().with_settings(|settings: &mut ImageLoaderSettings| {
            settings.sampler = ImageSampler::linear();
        }).load(layer.image);
        handles.push(image.clone());
        let rest_position = chunk_top_left - Vec2::new(0.0, layer.top_row);
        let mut entity = commands.spawn((
            Name::new("SurfaceBackdropLayer"),
            Sprite { image, ..default() },
            Anchor::TOP_LEFT,
            Transform::from_translation(rest_position.extend(layer.z)),
            RenderLayers::layer(0),
            SurfaceLayer { depth: layer.depth, rest_position, pivot: spawn.position },
            ChildOf(root.0),
            StateDespawnMarker,
        ));
        if layer.formed_crater {
            entity.insert(super::impact_crater::ImpactFormed);
        }
    }
    commands.insert_resource(SurfaceBackdropImages(handles));
    commands.insert_resource(OpenSkyRegion {
        x_range: Vec2::new(chunk_top_left.x, chunk_top_left.x + chunk_px),
        fade_y: Vec2::new(chunk_top_left.y - FOG_CLEAR_ROWS.0, chunk_top_left.y - FOG_CLEAR_ROWS.1),
    });
}

pub(super) fn place_surface_layers(
    camera: Query<&Transform, (With<MainCamera>, Without<SurfaceLayer>)>,
    mut layers: Query<(&SurfaceLayer, &mut Transform)>,
) {
    let Ok(camera) = camera.single() else {
        return;
    };
    for (layer, mut transform) in &mut layers {
        let offset = camera.translation.xy() - layer.pivot;
        let offset = Vec2::new(offset.x, offset.y.max(0.0));
        let position = layer.rest_position + offset * (1.0 - 1.0 / layer.depth);
        transform.translation.x = position.x;
        transform.translation.y = position.y;
    }
}
