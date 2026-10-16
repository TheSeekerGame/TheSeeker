//! Foreground fog: two animated layers of noise over the view, a rear layer and a detailed
//! one nearer the camera, cleared around tethers and under open sky. The vignette pass draws
//! them; this module owns their settings, their atlases, and the mask tethers protect into.
use bevy::asset::RenderAssetUsages;
use bevy::camera::visibility::{NoFrustumCulling, RenderLayers};
use bevy::camera::{OrthographicProjection, Projection, RenderTarget, ScalingMode};
use bevy::image::ImageSampler;
use bevy::mesh::{Mesh, Mesh2d, MeshVertexBufferLayoutRef};
use bevy::render::extract_component::{ExtractComponent, ExtractComponentPlugin, UniformComponentPlugin};
use bevy::render::extract_resource::{ExtractResource, ExtractResourcePlugin};
use bevy::render::render_asset::RenderAssets;
use bevy::render::render_resource::{
    AsBindGroup, BlendComponent, BlendFactor, BlendOperation, BlendState, ColorWrites, Extent3d,
    RenderPipelineDescriptor, ShaderType, SpecializedMeshPipelineError, TextureDimension, TextureFormat,
    TextureUsages, TextureView,
};
use bevy::render::texture::GpuImage;
use bevy::render::view::Msaa;
use bevy::shader::ShaderRef;
use bevy::sprite_render::{AlphaMode2d, Material2d, Material2dKey, Material2dPlugin, MeshMaterial2d};
use theseeker_engine::time::GameTime;

use crate::camera::{update_camera, GameplayPixelScale, MainCamera};
use crate::game::player::spawns::tether_node::{TetherInfluenceVisuals, VisualInfluenceCircle};
use crate::graphics::quad_mesh::{add_mesh, publish_mesh, QuadMesh};
use crate::prelude::*;

const DETAIL_ATLAS_PATH: &str = "fx/foreground_fog_atlas.png";
const REAR_ATLAS_PATH: &str = "fx/foreground_fog_rear_atlas.png";
/// Both atlases hold a looping flipbook in a grid of frames, each framed by a gutter.
const ATLAS_GRID: UVec2 = UVec2::new(12, 8);
const GUTTER_PX: f32 = 2.0;
const FRAMES_PER_SECOND: f32 = 12.0;
const FOG_COLOR: Color = Color::srgb(0.58, 0.62, 0.64);
/// A tether's protection as shares of its radius: how far inside its outline the detailed
/// layer's protection reaches, and how far past the outline protection fades out.
const PROTECTION_BAND: Vec2 = Vec2::new(1.0 / 12.0, 1.0 / 6.0);
/// How much of the fog full protection clears.
const PROTECTION_STRENGTH: f32 = 1.0;
/// World units the mask draws past each tether's protection.
const PROTECTION_MASK_MARGIN_WORLD: f32 = 1.0;
/// The render layer that only the protection mask's camera sees.
const PROTECTION_MASK_LAYER: usize = 7;

pub(crate) struct ForegroundFogPlugin;

impl Plugin for ForegroundFogPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ForegroundFogImages>()
            .add_plugins((
                Material2dPlugin::<FogProtectionMaskMaterial>::default(),
                ExtractComponentPlugin::<ForegroundFogSettings>::default(),
                UniformComponentPlugin::<ForegroundFogSettings>::default(),
                ExtractResourcePlugin::<ForegroundFogImages>::default(),
            ))
            .register_type::<ForegroundFogSettings>()
            .register_required_components::<MainCamera, ForegroundFogSettings>()
            .add_systems(OnEnter(AppState::InGame), spawn_fog_protection_mask)
            .add_systems(
                Update,
                (sync_foreground_fog_settings, maintain_fog_protection_mask, sync_fog_protection_mesh)
                    .chain()
                    .after(update_camera)
                    .in_set(GameTickSet::Post)
                    .run_if(in_state(AppState::InGame)),
            );
    }
}

/// One fog layer: a looping flipbook of noise tiled across the world, laid out as `FogLayer`
/// in `vignette.wgsl`.
#[derive(Clone, Copy, Debug, Reflect, ShaderType)]
pub(crate) struct FogLayer {
    /// The noise values that map to the least and the most fog.
    density_range: Vec2,
    /// A flipbook frame's size in the atlas, without its gutter.
    frame_size_px: f32,
    /// The world span one frame tiles.
    tile_world_size: f32,
    /// How fast the layer passes against the camera; above one, it lies nearer than the world.
    parallax_factor: f32,
    optical_thickness: f32,
    /// The density of the thinnest noise.
    minimum_density: f32,
    /// How far inside a tether's outline its protection reaches, as a share of its radius.
    protection_reach: f32,
}

impl FogLayer {
    /// Broad fog behind the detailed layer, protected only outside tethers' outlines.
    const REAR: Self = Self {
        density_range: Vec2::new(0.08, 0.92),
        frame_size_px: 128.0,
        tile_world_size: 512.0,
        parallax_factor: 1.08,
        optical_thickness: 1.15,
        minimum_density: 0.08,
        protection_reach: 0.0,
    };
    /// Detailed fog nearer the camera, protected into tethers' outlines.
    const DETAIL: Self = Self {
        density_range: Vec2::new(0.0, 0.9),
        frame_size_px: 256.0,
        tile_world_size: 384.0,
        parallax_factor: 1.2,
        optical_thickness: 1.15,
        minimum_density: 0.0,
        protection_reach: PROTECTION_BAND.x,
    };
}

/// What the vignette pass reads to draw the fog, laid out as `ForegroundFogSettings` in
/// `vignette.wgsl`; a test checks the layout.
#[derive(Component, Clone, Copy, Debug, ExtractComponent, Reflect, ShaderType)]
#[extract_component_sync_target((Self, bevy::render::extract_component::DynamicUniformIndex<Self>))]
pub(crate) struct ForegroundFogSettings {
    rear: FogLayer,
    detail: FogLayer,
    /// Linear.
    fog_color: Vec3,
    protection_strength: f32,
    protection_band: Vec2,
    camera_center: Vec2,
    view_world_size: Vec2,
    atlas_grid: UVec2,
    frame_index: u32,
    gutter_px: f32,
    /// See [`OpenSkyRegion`]; the default range is empty.
    open_sky: Vec4,
}

impl Default for ForegroundFogSettings {
    fn default() -> Self {
        Self {
            rear: FogLayer::REAR,
            detail: FogLayer::DETAIL,
            fog_color: FOG_COLOR.to_linear().to_vec3(),
            protection_strength: PROTECTION_STRENGTH,
            protection_band: PROTECTION_BAND,
            camera_center: Vec2::ZERO,
            view_world_size: Vec2::ONE,
            atlas_grid: ATLAS_GRID,
            frame_index: 0,
            gutter_px: GUTTER_PX,
            open_sky: OpenSkyRegion::uniform(None),
        }
    }
}

/// World region open to the sky, where foreground fog fades out upward.
#[derive(Resource, Clone, Copy, Debug)]
pub(crate) struct OpenSkyRegion {
    pub(crate) x_range: Vec2,
    /// Fog is unchanged below `x` and fully cleared above `y`.
    pub(crate) fade_y: Vec2,
}

impl OpenSkyRegion {
    fn uniform(region: Option<&Self>) -> Vec4 {
        region.map_or(Vec4::new(1.0, -1.0, 0.0, 1.0), |region| {
            Vec4::new(region.x_range.x, region.x_range.y, region.fade_y.x, region.fade_y.y)
        })
    }
}

#[derive(Resource, Clone)]
pub(crate) struct ForegroundFogImages {
    rear_atlas: Handle<Image>,
    detail_atlas: Handle<Image>,
    protection_mask: Handle<Image>,
    /// A blank stand-in for whichever atlas has not loaded yet.
    fallback: Handle<Image>,
    /// The protection mask's size, one texel per game pixel of the view.
    mask_size: UVec2,
}

impl ExtractResource for ForegroundFogImages {
    type Source = Self;

    fn extract_resource(source: &Self::Source) -> Self {
        source.clone()
    }
}

impl FromWorld for ForegroundFogImages {
    fn from_world(world: &mut World) -> Self {
        let server = world.resource::<AssetServer>();
        let (rear_atlas, detail_atlas) = (server.load(REAR_ATLAS_PATH), server.load(DETAIL_ATLAS_PATH));
        let mut images = world.resource_mut::<Assets<Image>>();
        Self {
            rear_atlas,
            detail_atlas,
            protection_mask: images.add(create_protection_mask_image(UVec2::ONE)),
            fallback: images.add(create_fallback_image()),
            mask_size: UVec2::ONE,
        }
    }
}

/// The fog's textures on the GPU.
pub(crate) struct FogTextureViews<'a> {
    pub(crate) rear_atlas: &'a TextureView,
    pub(crate) detail_atlas: &'a TextureView,
    pub(crate) protection_mask: &'a TextureView,
}

/// The fog's textures, each blank until it has loaded, once the blank one has.
pub(crate) fn fog_texture_views(world: &World) -> Option<FogTextureViews<'_>> {
    let images = world.resource::<ForegroundFogImages>();
    let gpu_images = world.resource::<RenderAssets<GpuImage>>();
    let fallback = gpu_images.get(&images.fallback)?;
    let view = |handle: &Handle<Image>| &gpu_images.get(handle).unwrap_or(fallback).texture_view;
    Some(FogTextureViews {
        rear_atlas: view(&images.rear_atlas),
        detail_atlas: view(&images.detail_atlas),
        protection_mask: view(&images.protection_mask),
    })
}

#[derive(Component)]
struct FogProtectionMaskCamera;

#[derive(Component)]
struct FogProtectionMaskMesh;

/// Draws each tether's protection into the mask: full inside its outline, fading across the
/// protection band past it.
#[derive(Asset, TypePath, AsBindGroup, Clone)]
struct FogProtectionMaskMaterial {
    #[uniform(0)]
    protection_band: Vec2,
}

impl Material2d for FogProtectionMaskMaterial {
    fn fragment_shader() -> ShaderRef {
        "shaders/post_processing/foreground_fog_mask.wgsl".into()
    }

    fn alpha_mode(&self) -> AlphaMode2d {
        AlphaMode2d::Blend
    }

    /// Overlapping tethers keep the strongest protection, written to the red channel.
    fn specialize(
        descriptor: &mut RenderPipelineDescriptor,
        _layout: &MeshVertexBufferLayoutRef,
        _key: Material2dKey<Self>,
    ) -> Result<(), SpecializedMeshPipelineError> {
        let Some(target) = descriptor.fragment.as_mut()
            .and_then(|fragment| fragment.targets.first_mut())
            .and_then(Option::as_mut)
        else {
            return Ok(());
        };
        let max = BlendComponent { src_factor: BlendFactor::One, dst_factor: BlendFactor::One, operation: BlendOperation::Max };
        target.blend = Some(BlendState { color: max, alpha: max });
        target.write_mask = ColorWrites::RED;
        Ok(())
    }
}

/// The protection mask camera's projection for a mask of `size` texels, one per world unit.
fn protection_mask_projection(size: UVec2) -> Projection {
    Projection::Orthographic(OrthographicProjection {
        scaling_mode: ScalingMode::Fixed { width: size.x as f32, height: size.y as f32 },
        ..OrthographicProjection::default_2d()
    })
}

fn spawn_fog_protection_mask(
    mut commands: Commands,
    images: Res<ForegroundFogImages>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<FogProtectionMaskMaterial>>,
) {
    commands.spawn((
        Camera2d,
        RenderTarget::Image(images.protection_mask.clone().into()),
        Camera { order: -2, clear_color: ClearColorConfig::Custom(Color::NONE), ..default() },
        protection_mask_projection(images.mask_size),
        Msaa::Off,
        RenderLayers::layer(PROTECTION_MASK_LAYER),
        FogProtectionMaskCamera,
        StateDespawnMarker,
        Name::new("FogProtectionMaskCamera"),
    ));
    commands.spawn((
        FogProtectionMaskMesh,
        NoFrustumCulling,
        Mesh2d(add_mesh(&mut meshes, protection_mask_mesh(&[]))),
        MeshMaterial2d(materials.add(FogProtectionMaskMaterial { protection_band: PROTECTION_BAND })),
        Transform::default(),
        Visibility::Visible,
        RenderLayers::layer(PROTECTION_MASK_LAYER),
        StateDespawnMarker,
        Name::new("FogProtectionMask"),
    ));
}

fn sync_foreground_fog_settings(
    game_time: Res<GameTime>,
    scale: Res<GameplayPixelScale>,
    cameras: Query<&Transform, With<MainCamera>>,
    open_sky: Option<Res<OpenSkyRegion>>,
    mut settings: Query<&mut ForegroundFogSettings>,
) {
    let Ok(camera) = cameras.single() else {
        return;
    };
    for mut settings in &mut settings {
        settings.camera_center = camera.translation.truncate();
        settings.view_world_size = scale.visible_game_size().max(Vec2::ONE);
        settings.frame_index = fog_frame_index(game_time.time_in_seconds());
        settings.open_sky = OpenSkyRegion::uniform(open_sky.as_deref());
    }
}

/// Keeps the protection mask's camera on the main camera, and the mask one texel per game
/// pixel of the view.
fn maintain_fog_protection_mask(
    mut fog_images: ResMut<ForegroundFogImages>,
    mut images: ResMut<Assets<Image>>,
    scale: Res<GameplayPixelScale>,
    main_cameras: Query<(&Camera, &Transform), With<MainCamera>>,
    mut mask_cameras: Query<(&mut Camera, &mut Transform, &mut Projection), (With<FogProtectionMaskCamera>, Without<MainCamera>)>,
) {
    let (Ok((main_camera, main_transform)), Ok((mut camera, mut transform, mut projection))) =
        (main_cameras.single(), mask_cameras.single_mut())
    else {
        return;
    };
    camera.is_active = main_camera.is_active;
    *transform = *main_transform;
    let size = scale.visible_game_size().as_uvec2().max(UVec2::ONE);
    if size != fog_images.mask_size {
        images.insert(&fog_images.protection_mask, create_protection_mask_image(size))
            .expect("fog protection handle remains reserved");
        fog_images.mask_size = size;
        *projection = protection_mask_projection(size);
    }
}

fn sync_fog_protection_mesh(
    influences: Res<TetherInfluenceVisuals>,
    mut mesh_entities: Query<&mut Mesh2d, With<FogProtectionMaskMesh>>,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    if !influences.is_changed() {
        return;
    }
    let Ok(mut mesh_handle) = mesh_entities.single_mut() else {
        return;
    };
    publish_mesh(&mut meshes, &mut mesh_handle.0, protection_mask_mesh(&influences.circles));
}

/// A quad over each tether's protection, carrying its outline for the mask shader.
fn protection_mask_mesh(influences: &[VisualInfluenceCircle]) -> Mesh {
    let mut mesh = QuadMesh::with_capacity(influences.len());
    for influence in influences.iter().filter(|influence| influence.radius > 0.0) {
        let outer_radius = influence.maximum_outline_radius()
            + influence.radius * PROTECTION_BAND.y
            + PROTECTION_MASK_MARGIN_WORLD;
        let outline = [
            influence.radius / outer_radius,
            influence.deformation / outer_radius,
            influence.outline_jaggedness() / outer_radius,
            influence.outline_seed(),
        ];
        mesh.rect(influence.center - Vec2::splat(outer_radius), influence.center + Vec2::splat(outer_radius), outline);
    }
    mesh.finish_colored()
}

fn create_protection_mask_image(size: UVec2) -> Image {
    let size = size.max(UVec2::ONE);
    let mut image = Image::new_fill(
        Extent3d { width: size.x, height: size.y, depth_or_array_layers: 1 },
        TextureDimension::D2,
        &[0, 0, 0, 0],
        TextureFormat::Bgra8UnormSrgb,
        RenderAssetUsages::default(),
    );
    image.texture_descriptor.usage = TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST | TextureUsages::RENDER_ATTACHMENT;
    image.sampler = ImageSampler::linear();
    image
}

fn create_fallback_image() -> Image {
    let mut image = Image::new_fill(Extent3d::default(), TextureDimension::D2, &[0], TextureFormat::R8Unorm, RenderAssetUsages::default());
    image.texture_descriptor.usage = TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST;
    image.sampler = ImageSampler::linear();
    image
}

/// The flipbook frame the fog shows `elapsed_seconds` in, looping through the atlas.
fn fog_frame_index(elapsed_seconds: f64) -> u32 {
    let frames_per_second = f64::from(FRAMES_PER_SECOND);
    let loop_seconds = f64::from(ATLAS_GRID.element_product()) / frames_per_second;
    (elapsed_seconds.rem_euclid(loop_seconds) * frames_per_second).floor() as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fog_animation_wraps_after_eight_seconds() {
        assert_eq!(fog_frame_index(0.0), 0);
        assert_eq!(fog_frame_index(1.0 / 12.0), 1);
        assert_eq!(fog_frame_index(7.999), 95);
        assert_eq!(fog_frame_index(8.0), 0);
    }

    #[test]
    fn each_layers_atlas_holds_its_grid_of_framed_frames() {
        for (path, layer) in [(REAR_ATLAS_PATH, FogLayer::REAR), (DETAIL_ATLAS_PATH, FogLayer::DETAIL)] {
            let png = std::fs::read(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("assets").join(path)).unwrap();
            // A PNG's header chunk opens with its width and height.
            let dimension = |at: usize| u32::from_be_bytes(png[at..at + 4].try_into().unwrap());
            let cell = (layer.frame_size_px + 2.0 * GUTTER_PX) as u32;
            assert_eq!(UVec2::new(dimension(16), dimension(20)), ATLAS_GRID * cell, "{path}");
        }
    }
}
