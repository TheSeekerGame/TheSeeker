use bevy::camera::visibility::RenderLayers;
use bevy::asset::RenderAssetUsages;
use crate::graphics::fullscreen::{self, FullscreenPass, FullscreenView};
use bevy::image::ImageSampler;
use bevy::camera::{Camera, OrthographicProjection, RenderTarget};
use bevy::render::extract_component::{
    ComponentUniforms, ExtractComponent, ExtractComponentPlugin,
    UniformComponentPlugin,
};
use bevy::render::extract_resource::{ExtractResource, ExtractResourcePlugin};
use bevy::render::render_asset::RenderAssets;
use bevy::render::render_resource::binding_types::uniform_buffer;
use bevy::render::render_resource::{
    BindGroupEntries, BlendState, Extent3d, FilterMode, MipmapFilterMode, Sampler,
    SamplerDescriptor, ShaderType, TextureDimension, TextureFormat, TextureUsages,
};
use bevy::render::renderer::{ViewQuery, RenderContext, RenderDevice};
use bevy::render::texture::GpuImage;
use bevy::render::view::Msaa;
use bevy::render::RenderApp;

use crate::camera::MainCamera;
use super::pixel_view::{PixelView, RenderBand, update_pixel_view};
use crate::prelude::*;

/// Marker for the off-screen pixel-perfect camera.
#[derive(Component)]
pub struct PixelPerfectCamera;

/// Composite parameters for the main camera, whose view the composite pass draws.
#[derive(Component, Clone, Copy, ExtractComponent, ShaderType, Reflect)]
#[extract_component_sync_target((Self, bevy::render::extract_component::DynamicUniformIndex<Self>))]
pub struct PixelPerfectCompositeSettings {
    pub offset_uv: Vec2,
    pub sample_uv_min: Vec2,
    pub sample_uv_size: Vec2,
}

impl Default for PixelPerfectCompositeSettings {
    fn default() -> Self {
        Self {
            offset_uv: Vec2::ZERO,
            sample_uv_min: Vec2::ZERO,
            sample_uv_size: Vec2::ONE,
        }
    }
}

/// GPU render target that holds the pixel-perfect layer.
#[derive(Resource, Clone)]
pub struct PixelPerfectTarget {
    pub handle: Handle<Image>,
    pub size: UVec2,
}

impl ExtractResource for PixelPerfectTarget {
    type Source = PixelPerfectTarget;

    fn extract_resource(source: &Self) -> Self {
        source.clone()
    }
}

impl FromWorld for PixelPerfectTarget {
    fn from_world(world: &mut World) -> Self {
        let size = Extent3d {
            width: 1,
            height: 1,
            depth_or_array_layers: 1,
        };

        let mut image = Image::new_fill(
            size,
            TextureDimension::D2,
            &[0, 0, 0, 0],
            TextureFormat::Bgra8UnormSrgb,
            RenderAssetUsages::default(), // MAIN_WORLD | RENDER_WORLD required for render targets
        );
        image.texture_descriptor.usage = TextureUsages::TEXTURE_BINDING
            | TextureUsages::COPY_DST
            | TextureUsages::RENDER_ATTACHMENT;
        image.sampler = ImageSampler::nearest();

        let handle = world.resource_mut::<Assets<Image>>().add(image);
        Self {
            handle,
            size: UVec2::ONE,
        }
    }
}

/// The composite pass, which blends the pixel-perfect layer over the main view.
#[derive(Resource)]
struct PixelPerfectCompositePipeline {
    pass: FullscreenPass,
    layer_sampler: Sampler,
}

impl FromWorld for PixelPerfectCompositePipeline {
    fn from_world(world: &mut World) -> Self {
        // The main view, then the pixel-perfect layer with its nearest sampler.
        let pass = FullscreenPass::new(world, "pixel_perfect_composite", "shaders/pixel_perfect_composite.wgsl", (
            fullscreen::texture(),
            fullscreen::texture(),
            fullscreen::sampler(),
            uniform_buffer::<PixelPerfectCompositeSettings>(false),
        ), Some(BlendState::ALPHA_BLENDING));
        let layer_sampler = world.resource::<RenderDevice>().create_sampler(&SamplerDescriptor {
            label: Some("pixel_perfect_layer_sampler"),
            mag_filter: FilterMode::Nearest,
            min_filter: FilterMode::Nearest,
            mipmap_filter: MipmapFilterMode::Nearest,
            ..default()
        });
        Self { pass, layer_sampler }
    }
}

pub(crate) fn pixel_composite_pass(
    world: &World,
    view: ViewQuery<FullscreenView, With<PixelPerfectCompositeSettings>>,
    mut render_context: RenderContext,
) {
    let target = world.resource::<PixelPerfectTarget>();
    let layer = world.resource::<RenderAssets<GpuImage>>().get(&target.handle);
    let settings = world.resource::<ComponentUniforms<PixelPerfectCompositeSettings>>().uniforms().binding();
    let (Some(layer), Some(settings)) = (layer, settings) else {
        return;
    };
    let pipeline = world.resource::<PixelPerfectCompositePipeline>();
    pipeline.pass.draw(world, &mut render_context, view.into_inner(), |image| BindGroupEntries::sequential((
        image,
        &layer.texture_view,
        &pipeline.layer_sampler,
        settings,
    )), &[]);
}

pub struct PixelPerfectPlugin;

impl Plugin for PixelPerfectPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PixelPerfectTarget>();

        app.add_plugins((
            ExtractResourcePlugin::<PixelPerfectTarget>::default(),
            ExtractComponentPlugin::<PixelPerfectCompositeSettings>::default(),
            UniformComponentPlugin::<PixelPerfectCompositeSettings>::default(),
        ));
        app.register_required_components::<MainCamera, PixelPerfectCompositeSettings>();

        // Spawn PP camera at the same time as main camera to ensure full initialization
        app.add_systems(
            OnEnter(AppState::InGame),
            spawn_pixel_perfect_camera,
        );

        // Run PP camera sync after the main camera update so both cameras share
        // the interpolated transform each frame.
        app.add_systems(
            Update,
            maintain_pixel_perfect_camera
                .after(update_pixel_view)
                .in_set(GameTickSet::Post)
                .run_if(in_state(AppState::InGame)),
        );
    }

    fn finish(&self, app: &mut App) {
        let Some(render_app) = app.get_sub_app_mut(RenderApp) else {
            return;
        };
        render_app.add_systems(bevy::render::RenderStartup, |world: &mut World| { world.init_resource::<PixelPerfectCompositePipeline>(); });
    }
}

/// Spawn the pixel-perfect camera at game start.
/// This runs in OnEnter(AppState::InGame) alongside the main camera setup
/// to ensure the PP camera is fully initialized before any visibility checks.
fn spawn_pixel_perfect_camera(
    mut commands: Commands,
    target: Res<PixelPerfectTarget>,
) {
    // PP camera needs scale=1.0 so that 1 texel = 1 world unit
    // (The PP render target is sized such that this gives pixel-perfect rendering)
    let projection = Projection::Orthographic(OrthographicProjection {
        scale: 1.0,
        ..OrthographicProjection::default_2d()
    });

    commands.spawn((
        Camera2d,
        RenderTarget::Image(target.handle.clone().into()),
        Camera {
            order: -1,
            clear_color: ClearColorConfig::Custom(Color::NONE),
            ..Default::default()
        },
        projection,
        Msaa::Off,
        RenderBand::PixelatedWorld.layers(),
        PixelPerfectCamera,
        StateDespawnMarker,
        Name::new("PixelPerfectCamera"),
    ));
}

/// Update the off-screen camera transform and composite settings each frame.
fn maintain_pixel_perfect_camera(
    mut target: ResMut<PixelPerfectTarget>,
    mut images: ResMut<Assets<Image>>,
    view: Res<PixelView>,
    mut q_main: Query<(&Camera, &mut PixelPerfectCompositeSettings), (With<MainCamera>, Without<PixelPerfectCamera>)>,
    mut q_pp: Query<(&mut Camera, &mut Transform), (With<PixelPerfectCamera>, Without<MainCamera>)>,
) {
    let Ok((main_camera, mut settings)) = q_main.single_mut() else {
        return;
    };

    let Some(view) = view.0 else { return; };
    let viewport_physical = view.viewport;
    if viewport_physical.x == 0 || viewport_physical.y == 0 {
        return;
    }

    let divisor = view.divisor;
    let desired_size = UVec2::new(
        viewport_physical.x.saturating_add(divisor - 1) / divisor,
        viewport_physical.y.saturating_add(divisor - 1) / divisor,
    );

    // Resize render target if needed
    if desired_size != target.size {
        let mut image = Image::new_fill(
            Extent3d {
                width: desired_size.x,
                height: desired_size.y,
                depth_or_array_layers: 1,
            },
            TextureDimension::D2,
            &[0, 0, 0, 0],
            TextureFormat::Bgra8UnormSrgb,
            RenderAssetUsages::default(),
        );
        image.texture_descriptor.usage = TextureUsages::TEXTURE_BINDING
            | TextureUsages::COPY_DST
            | TextureUsages::RENDER_ATTACHMENT;
        image.sampler = ImageSampler::nearest();
        images.insert(&target.handle, image).expect("pixel target handle remains reserved");
        target.size = desired_size;
    }

    let Ok((mut pp_camera, mut pp_transform)) = q_pp.single_mut() else {
        return;
    };
    pp_camera.is_active = main_camera.is_active;
    pp_transform.translation = view.raster_center.extend(view.camera.z);
    let snapped_residual = view.residual;

    // Sub-texel offset for camera movement on the composite.
    // The offset must account for the Y-flip between world coords (Y+ up) and UV coords (V+ down).
    // - For X: world X and UV U are the same direction, so offset.x = -residual.x * ts
    // - For Y: world Y and UV V are opposite, so offset.y = +residual.y * ts
    let texel_size = Vec2::new(
        1.0 / target.size.x.max(1) as f32,
        1.0 / target.size.y.max(1) as f32,
    );
    let target_screen_size = target.size.as_vec2() * divisor as f32;
    let sample_uv_size = Vec2::new(
        (viewport_physical.x as f32 / target_screen_size.x).clamp(0.0, 1.0),
        (viewport_physical.y as f32 / target_screen_size.y).clamp(0.0, 1.0),
    );
    let sample_uv_min = (Vec2::ONE - sample_uv_size) * 0.5;

    *settings = PixelPerfectCompositeSettings {
        offset_uv: Vec2::new(-snapped_residual.x * texel_size.x, snapped_residual.y * texel_size.y),
        sample_uv_min,
        sample_uv_size,
    };
}

/// Convenience helper for attaching the render layer to pixel-perfect visuals.
pub fn pixel_perfect_layers() -> RenderLayers {
    RenderBand::PixelatedWorld.layers()
}
