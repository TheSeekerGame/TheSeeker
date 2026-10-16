use bevy::asset::RenderAssetUsages;
use crate::graphics::fullscreen::{self, FullscreenPass, FullscreenView};
use bevy::image::ImageSampler;
use bevy::render::extract_resource::{ExtractResource, ExtractResourcePlugin};
use bevy::render::extract_component::{
    ComponentUniforms, DynamicUniformIndex, ExtractComponent,
    ExtractComponentPlugin, UniformComponentPlugin,
};
use bevy::render::render_asset::RenderAssets;
use bevy::render::render_resource::binding_types::uniform_buffer;
use bevy::render::render_resource::{
    BindGroupEntries, FilterMode, MipmapFilterMode, Sampler, SamplerDescriptor, ShaderType,
    TextureDimension, TextureFormat, TextureUsages,
};
use bevy::render::renderer::{ViewQuery, RenderContext, RenderDevice};
use bevy::render::texture::GpuImage;
use bevy::render::RenderApp;

use crate::camera::{update_camera, MainCamera};
use crate::game::player::Player;
use crate::graphics::post_processing::foreground_fog::{fog_texture_views, ForegroundFogSettings};
use crate::prelude::*;

pub(crate) struct VignettePlugin;

const PLAYER_VIGNETTE_FOLLOW_STRENGTH: f32 = 0.85;
const MAX_PLAYER_VIGNETTE_OFFSET_UV: f32 = 0.32;
const OUTER_FOG_STRENGTH: f32 = 0.28;
const VIGNETTE_NOISE_TEXTURE_SIZE: u32 = 1024;
const VIGNETTE_RADIUS: f32 = 0.18;
const VIGNETTE_SMOOTHNESS: f32 = 0.62;

impl Plugin for VignettePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<VignetteNoiseImage>();
        app.add_plugins((
            ExtractComponentPlugin::<VignetteSettings>::default(),
            ExtractResourcePlugin::<VignetteNoiseImage>::default(),
            UniformComponentPlugin::<VignetteSettings>::default(),
        ));

        app.register_type::<VignetteSettings>();
        app.register_required_components::<MainCamera, VignetteSettings>();
        app.add_systems(
            Update,
            sync_vignette_settings
                .after(update_camera)
                .in_set(GameTickSet::Post)
                .run_if(in_state(AppState::InGame)),
        );

    }

    fn finish(&self, app: &mut App) {
        let Some(render_app) = app.get_sub_app_mut(RenderApp) else {
            return;
        };

        render_app.add_systems(bevy::render::RenderStartup, |world: &mut World| { world.init_resource::<VignettePipeline>(); });
    }
}

/// Component that controls the vignette post processing effect
#[derive(Component, Clone, Copy, ExtractComponent, ShaderType, Reflect)]
#[extract_component_sync_target((Self, bevy::render::extract_component::DynamicUniformIndex<Self>))]
pub struct VignetteSettings {
    /// The color of the vignette
    pub color: Vec3,
    /// brightness clamp for the top value of vignette gradient
    pub base_brightness: f32,
    /// The radius of the vignette
    pub radius: f32,
    /// How smooth the vignette gradient is
    pub smoothness: f32,
    /// Offset for the vignette center
    pub offset: Vec2,
    /// Extra noisy fog darkening concentrated in the outer rim.
    pub fog_strength: f32,
}

// Default configuration for a mild black vignette
impl Default for VignetteSettings {
    fn default() -> Self {
        Self {
            color: Vec3::new(0.0, 0.0, 0.0),
            base_brightness: 0.9,
            radius: VIGNETTE_RADIUS,
            smoothness: VIGNETTE_SMOOTHNESS,
            offset: Vec2::ZERO,
            fog_strength: OUTER_FOG_STRENGTH,
        }
    }
}

#[derive(Resource, Clone)]
struct VignetteNoiseImage {
    handle: Handle<Image>,
}

impl ExtractResource for VignetteNoiseImage {
    type Source = Self;

    fn extract_resource(source: &Self::Source) -> Self {
        source.clone()
    }
}

impl FromWorld for VignetteNoiseImage {
    fn from_world(world: &mut World) -> Self {
        let handle = world
            .resource_mut::<Assets<Image>>()
            .add(build_vignette_noise_image());
        Self { handle }
    }
}

/// Leans the vignette's center toward the player, no further than its noise reaches.
fn sync_vignette_settings(
    cameras: Query<(&Transform, &Projection), With<MainCamera>>,
    player: Query<&GlobalTransform, With<Player>>,
    mut vignettes: Query<&mut VignetteSettings>,
) {
    let offset = match (cameras.single(), player.single()) {
        (Ok((camera, Projection::Orthographic(ortho))), Ok(player)) => {
            // The player's place in the view as a screen uv, whose y points down.
            let from_center = (player.translation().truncate() - camera.translation.truncate()) / ortho.area.size().max(Vec2::ONE);
            let uv = (Vec2::new(from_center.x, -from_center.y) + 0.5).clamp(Vec2::ZERO, Vec2::ONE);
            ((uv - 0.5) * PLAYER_VIGNETTE_FOLLOW_STRENGTH).clamp(Vec2::splat(-MAX_PLAYER_VIGNETTE_OFFSET_UV), Vec2::splat(MAX_PLAYER_VIGNETTE_OFFSET_UV))
        },
        _ => Vec2::ZERO,
    };
    for mut settings in &mut vignettes {
        settings.offset = offset;
    }
}

pub(crate) fn vignette_pass(
    world: &World,
    view: ViewQuery<(FullscreenView, &DynamicUniformIndex<VignetteSettings>, &DynamicUniformIndex<ForegroundFogSettings>)>,
    mut render_context: RenderContext,
) {
    let (view, settings_index, fog_index) = view.into_inner();
    let settings = world.resource::<ComponentUniforms<VignetteSettings>>().uniforms().binding();
    let fog = world.resource::<ComponentUniforms<ForegroundFogSettings>>().uniforms().binding();
    let noise = world.resource::<RenderAssets<GpuImage>>().get(&world.resource::<VignetteNoiseImage>().handle);
    let (Some(settings), Some(fog), Some(noise), Some(fog_textures)) = (settings, fog, noise, fog_texture_views(world)) else {
        return;
    };
    let pipeline = world.resource::<VignettePipeline>();
    pipeline.pass.draw(world, &mut render_context, view, |image| BindGroupEntries::sequential((
        image,
        &noise.texture_view,
        &pipeline.noise_sampler,
        settings,
        fog,
        fog_textures.rear_atlas,
        fog_textures.detail_atlas,
        fog_textures.protection_mask,
    )), &[settings_index.index(), fog_index.index()]);
}

#[derive(Resource)]
struct VignettePipeline {
    pass: FullscreenPass,
    noise_sampler: Sampler,
}

impl FromWorld for VignettePipeline {
    fn from_world(world: &mut World) -> Self {
        // The view's image, the noise with the sampler for noise and fog, the vignette's and
        // the fog's settings, then the fog's rear and detail atlases and protection mask.
        let pass = FullscreenPass::new(world, "vignette", "shaders/post_processing/vignette.wgsl", (
            fullscreen::texture(),
            fullscreen::texture(),
            fullscreen::sampler(),
            uniform_buffer::<VignetteSettings>(true),
            uniform_buffer::<ForegroundFogSettings>(true),
            fullscreen::texture(),
            fullscreen::texture(),
            fullscreen::texture(),
        ), None);
        let noise_sampler = world.resource::<RenderDevice>().create_sampler(&SamplerDescriptor {
            mag_filter: FilterMode::Linear,
            min_filter: FilterMode::Linear,
            mipmap_filter: MipmapFilterMode::Linear,
            ..default()
        });
        Self { pass, noise_sampler }
    }
}

fn build_vignette_noise_image() -> Image {
    #[inline]
    fn lerp(a: f32, b: f32, t: f32) -> f32 {
        a + (b - a) * t
    }

    #[inline]
    fn fract_f32(x: f32) -> f32 {
        x - x.floor()
    }

    #[inline]
    fn fract_vec2(v: Vec2) -> Vec2 {
        v - v.floor()
    }

    fn hash12(p: Vec2) -> f32 {
        let h = p.dot(Vec2::new(127.1, 311.7));
        fract_f32(h.sin() * 43_758.547)
    }

    fn value_noise(p: Vec2) -> f32 {
        let i = p.floor();
        let f = fract_vec2(p);
        let u = f * f * (Vec2::splat(3.0) - 2.0 * f);

        let a = hash12(i);
        let b = hash12(i + Vec2::new(1.0, 0.0));
        let c = hash12(i + Vec2::new(0.0, 1.0));
        let d = hash12(i + Vec2::new(1.0, 1.0));

        lerp(lerp(a, b, u.x), lerp(c, d, u.x), u.y)
    }

    let size = VIGNETTE_NOISE_TEXTURE_SIZE.max(1);
    let domain_min = Vec2::splat(-MAX_PLAYER_VIGNETTE_OFFSET_UV);
    let domain_size = Vec2::splat(1.0 + 2.0 * MAX_PLAYER_VIGNETTE_OFFSET_UV);
    let mut data = Vec::with_capacity((size * size) as usize);

    for y in 0..size {
        for x in 0..size {
            let uv = Vec2::new(
                (x as f32 + 0.5) / size as f32,
                (y as f32 + 0.5) / size as f32,
            );
            // Bake the whole UV range the player-follow offset can ever sample.
            let expanded_uv = domain_min + uv * domain_size;
            let centered_uv = expanded_uv - Vec2::splat(0.5);
            let fog_uv = centered_uv * 5.5 + Vec2::new(17.3, -9.1);
            let fog_noise = value_noise(fog_uv) * 0.72
                + value_noise(fog_uv * 2.07 + Vec2::new(13.1, 7.9)) * 0.28;

            data.push((fog_noise.clamp(0.0, 1.0) * 255.0).round() as u8);
        }
    }

    let mut image = Image::new(
        bevy::render::render_resource::Extent3d {
            width: size,
            height: size,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        data,
        TextureFormat::R8Unorm,
        RenderAssetUsages::default(),
    );
    image.texture_descriptor.usage = TextureUsages::TEXTURE_BINDING
        | TextureUsages::COPY_DST;
    image.sampler = ImageSampler::linear();
    image
}
