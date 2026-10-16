use crate::graphics::fullscreen::{self, FullscreenPass, FullscreenView};
use bevy::camera::Projection;
use bevy::render::extract_component::{
    ComponentUniforms, DynamicUniformIndex, ExtractComponent,
    ExtractComponentPlugin, UniformComponentPlugin,
};
use bevy::render::render_resource::binding_types::uniform_buffer;
use bevy::render::render_resource::{BindGroupEntries, ShaderType};
use bevy::render::renderer::{ViewQuery, RenderContext};
use bevy::render::RenderApp;

use crate::camera::{update_camera, MainCamera};
use crate::game::player::Player;
use crate::game::world_objects::{Capsule, CapsuleLightMask, CapsuleOccupant};
use crate::graphics::pixel_view::{PixelView, update_pixel_view};
use crate::prelude::*;

pub(crate) struct CapsuleDimPlugin;

const CAPSULE_DIM_FADE_SPEED: f32 = 1.0;
const CAPSULE_DIM_MAX_ALPHA: f32 = 0.2;
pub(crate) const CAPSULE_DIM_RECT_LIMIT: usize = 64;

/// What the capsule dim pass reads, laid out as `CapsuleDimSettings` in `capsule_dim.wgsl`.
/// Positions and sizes are in world units. The default dims nothing.
#[derive(Component, Clone, Copy, ExtractComponent, ShaderType, Reflect)]
#[extract_component_sync_target((Self, bevy::render::extract_component::DynamicUniformIndex<Self>))]
pub struct CapsuleDimSettings {
    /// The bottom-left corner of the camera's view.
    view_min: Vec2,
    view_size: Vec2,
    /// The share of light the view loses outside the lit rectangles.
    dim: f32,
    /// How many of `lit_rects` the capsule lights.
    lit_count: u32,
    /// The rooms the capsule lights, as minimum and maximum corners.
    lit_rects: [Vec4; CAPSULE_DIM_RECT_LIMIT],
}

impl Default for CapsuleDimSettings {
    fn default() -> Self {
        Self { view_min: Vec2::ZERO, view_size: Vec2::ZERO, dim: 0.0, lit_count: 0, lit_rects: [Vec4::ZERO; CAPSULE_DIM_RECT_LIMIT] }
    }
}

impl Plugin for CapsuleDimPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            ExtractComponentPlugin::<CapsuleDimSettings>::default(),
            UniformComponentPlugin::<CapsuleDimSettings>::default(),
        ));
        app.register_type::<CapsuleDimSettings>();
        app.register_required_components::<MainCamera, CapsuleDimSettings>();
        app.add_systems(
            Update,
            sync_capsule_dim_settings
                .after(update_camera)
                .after(update_pixel_view)
                .in_set(GameTickSet::Post)
                .run_if(in_state(AppState::InGame)),
        );
    }

    fn finish(&self, app: &mut App) {
        let Some(render_app) = app.get_sub_app_mut(RenderApp) else {
            return;
        };
        render_app.add_systems(bevy::render::RenderStartup, |world: &mut World| { world.init_resource::<CapsuleDimPipeline>(); });
    }
}

/// Fades the view's dimming in while the player shelters in a capsule, sparing the rooms the
/// capsule lights, and drops it when the player leaves.
fn sync_capsule_dim_settings(
    time: Res<PresentationTime>,
    view: Res<PixelView>,
    player: Query<&CapsuleOccupant, With<Player>>,
    capsules: Query<(&Transform, &CapsuleLightMask), With<Capsule>>,
    mut progress: Local<f32>,
    mut cameras: Query<(&mut CapsuleDimSettings, &Projection), With<MainCamera>>,
) {
    let occupant = player.single().ok();
    *progress = occupant.map_or(0.0, |_| (*progress + time.delta_secs() * CAPSULE_DIM_FADE_SPEED).min(1.0));
    let Some(view) = view.0 else { return; };
    let lit = occupant.and_then(|occupant| capsules.get(occupant.shelter).ok())
        .map(|(transform, mask)| mask.world_rects(transform.translation.truncate()))
        .unwrap_or_default();
    let mut lit_rects = [Vec4::ZERO; CAPSULE_DIM_RECT_LIMIT];
    for (slot, rect) in lit_rects.iter_mut().zip(&lit) {
        *slot = Vec4::from((rect.min, rect.max));
    }
    for (mut settings, projection) in &mut cameras {
        let Projection::Orthographic(ortho) = projection else { continue; };
        let view_size = ortho.area.size();
        *settings = CapsuleDimSettings {
            view_min: view.displayed_center() - 0.5 * view_size,
            view_size,
            dim: *progress * CAPSULE_DIM_MAX_ALPHA,
            lit_count: lit.len().min(CAPSULE_DIM_RECT_LIMIT) as u32,
            lit_rects,
        };
    }
}

pub(crate) fn capsule_dim_pass(
    world: &World,
    view: ViewQuery<(FullscreenView, &CapsuleDimSettings, &DynamicUniformIndex<CapsuleDimSettings>)>,
    mut render_context: RenderContext,
) {
    let (view, settings, settings_index) = view.into_inner();
    // Undimmed, the pass would only copy the view.
    if settings.dim <= 0.0 {
        return;
    }
    let Some(settings) = world.resource::<ComponentUniforms<CapsuleDimSettings>>().uniforms().binding() else {
        return;
    };
    world.resource::<CapsuleDimPipeline>().0.draw(world, &mut render_context, view,
        |image| BindGroupEntries::sequential((image, settings)), &[settings_index.index()]);
}

#[derive(Resource)]
struct CapsuleDimPipeline(FullscreenPass);

impl FromWorld for CapsuleDimPipeline {
    fn from_world(world: &mut World) -> Self {
        Self(FullscreenPass::new(world, "capsule_dim", "shaders/post_processing/capsule_dim.wgsl",
            (fullscreen::texture(), uniform_buffer::<CapsuleDimSettings>(true)), None))
    }
}
