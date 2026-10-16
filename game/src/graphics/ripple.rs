use crate::graphics::fullscreen::{self, FullscreenPass, FullscreenView};
use bevy::render::extract_component::{
    ComponentUniforms, ExtractComponent, ExtractComponentPlugin,
    UniformComponentPlugin,
};
use bevy::render::render_resource::binding_types::uniform_buffer;
use bevy::render::render_resource::{BindGroupEntries, ShaderType};
use bevy::render::renderer::{ViewQuery, RenderContext};
use bevy::render::RenderApp;
use bevy::window::PrimaryWindow;

use crate::camera::{update_camera, MainCamera};
use crate::prelude::*;

const MAX_RIPPLES: usize = 8;

/// Event to request a ripple centered at a world-space point.
#[derive(Message, Clone, Copy)]
pub struct RippleEvent {
    pub origin: Vec3,
    pub descriptor: RippleDescriptor,
}

/// Parameters that control the shape and feel of a ripple.
#[derive(Clone, Copy, Debug)]
pub struct RippleDescriptor {
    /// Starting radius in world units.
    pub start_radius: f32,
    /// Maximum radius in world units.
    pub max_radius: f32,
    /// Radial growth per game tick (world units).
    pub speed_per_tick: f32,
    /// Thickness of the ring in world units.
    pub width: f32,
    /// Peak UV offset in screen pixels at progress = 0.
    pub amplitude_px: f32,
    /// Controls how quickly amplitude decays as the ring expands.
    pub fade_power: f32,
}

impl RippleDescriptor {
    /// Fast, high-impact shockwave used for the amplified bell ring.
    pub fn shockwave() -> Self {
        Self {
            start_radius: 6.0,
            max_radius: 180.0,
            speed_per_tick: 12.0,
            width: 6.0,
            amplitude_px: 10.0,
            fade_power: 3.0,
        }
    }
}

#[derive(Component, Default)]
struct RippleState {
    active: Vec<ActiveRipple>,
}

struct ActiveRipple {
    origin: Vec3,
    descriptor: RippleDescriptor,
    age_ticks: u32,
}

/// GPU-facing uniform for the ripple post-process.
#[derive(Component, Clone, Copy, ShaderType, ExtractComponent, Default, Reflect)]
#[extract_component_sync_target((Self, bevy::render::extract_component::DynamicUniformIndex<Self>))]
pub struct RippleUniform {
    viewport_size: Vec2,
    ripple_count: u32,
    _pad0: u32,
    ripples: [GpuRipple; MAX_RIPPLES],
}

#[derive(Clone, Copy, ShaderType, Default, Reflect)]
struct GpuRipple {
    pub center: Vec2,
    pub radius_px: f32,
    pub width_px: f32,
    pub amplitude_px: f32,
    pub _pad: Vec2,
}

pub struct RippleFxPlugin;

impl Plugin for RippleFxPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<RippleEvent>();
        app.add_plugins((
            ExtractComponentPlugin::<RippleUniform>::default(),
            UniformComponentPlugin::<RippleUniform>::default(),
        ));

        app.register_required_components::<MainCamera, RippleUniform>();
        app.register_required_components::<MainCamera, RippleState>();
        app.add_systems(GameTickUpdate, update_ripples.after(update_camera).run_if(in_state(AppState::InGame)));
    }

    fn finish(&self, app: &mut App) {
        let Some(render_app) = app.get_sub_app_mut(RenderApp) else {
            return;
        };
        render_app.add_systems(bevy::render::RenderStartup, |world: &mut World| { world.init_resource::<RipplePipeline>(); });
    }
}

/// Advance active ripples, build the GPU uniform, and cull finished ripples.
fn update_ripples(
    mut ripple_events: MessageReader<RippleEvent>,
    mut q_cam: Query<
        (
            &Camera,
            &GlobalTransform,
            &Projection,
            &mut RippleState,
            &mut RippleUniform,
        ),
        With<MainCamera>,
    >,
    windows: Query<&Window, With<PrimaryWindow>>,
) {
    let Some((camera, cam_tf, projection, mut state, mut uniform)) =
        q_cam.iter_mut().next()
    else {
        return;
    };

    let Some(window) = windows.iter().next() else {
        *uniform = RippleUniform::default();
        return;
    };
    let viewport = camera
        .logical_viewport_size()
        .unwrap_or_else(|| Vec2::new(window.width(), window.height()));
    if viewport.x <= 0.0 || viewport.y <= 0.0 {
        *uniform = RippleUniform::default();
        return;
    }

    let pixels_per_world = match projection {
        Projection::Orthographic(ortho) => ortho.scale.recip(),
        _ => return,
    };

    // Append new requests (newest at the end).
    for event in ripple_events.read() {
        state.active.push(ActiveRipple {
            origin: event.origin,
            descriptor: event.descriptor,
            age_ticks: 0,
        });
    }

    // Keep the most recent ripples so the GPU array always holds fresh data.
    if state.active.len() > MAX_RIPPLES {
        let overflow = state.active.len() - MAX_RIPPLES;
        state.active.drain(0..overflow);
    }

    let mut gpu = RippleUniform {
        viewport_size: viewport,
        ..Default::default()
    };
    let mut gpu_count = 0u32;

    state.active.retain_mut(|ripple| {
        ripple.age_ticks = ripple.age_ticks.saturating_add(1);

        let desc = ripple.descriptor;
        let max_radius =
            (desc.max_radius).max(desc.start_radius + f32::EPSILON);
        let radius_world = (desc.start_radius
            + desc.speed_per_tick * ripple.age_ticks as f32)
            .min(max_radius);
        let width_world = desc.width.max(0.001);
        let denom = (max_radius - desc.start_radius).max(0.001);
        let progress =
            ((radius_world - desc.start_radius) / denom).clamp(0.0, 1.0);
        let amplitude_px =
            desc.amplitude_px * (1.0 - progress).powf(desc.fade_power);

        // Cull once the ring has dissipated.
        if amplitude_px < 0.001 && progress >= 0.999 {
            return false;
        }

        let Ok(screen_pos) = camera.world_to_viewport(cam_tf, ripple.origin) else {
                // Keep the ripple alive; it may come back into view as the camera moves.
                return true;
            };

        if (gpu_count as usize) < MAX_RIPPLES {
            gpu.ripples[gpu_count as usize] = GpuRipple {
                center: screen_pos / viewport,
                radius_px: radius_world * pixels_per_world,
                width_px: width_world * pixels_per_world,
                amplitude_px,
                _pad: Vec2::ZERO,
            };
            gpu_count += 1;
        }

        true
    });

    gpu.ripple_count = gpu_count;
    *uniform = gpu;
}

#[derive(Resource)]
struct RipplePipeline(FullscreenPass);

impl FromWorld for RipplePipeline {
    fn from_world(world: &mut World) -> Self {
        Self(FullscreenPass::new(world, "ripple", "shaders/post_processing/ripple.wgsl",
            (fullscreen::texture(), uniform_buffer::<RippleUniform>(false)), None))
    }
}

pub(crate) fn ripple_pass(world: &World, view: ViewQuery<(FullscreenView, &RippleUniform)>, mut render_context: RenderContext) {
    let (view, uniform) = view.into_inner();
    // Without ripples, the pass would only copy the view.
    if uniform.ripple_count == 0 {
        return;
    }
    let Some(settings) = world.resource::<ComponentUniforms<RippleUniform>>().uniforms().binding() else {
        return;
    };
    world.resource::<RipplePipeline>().0.draw(world, &mut render_context, view,
        |image| BindGroupEntries::sequential((image, settings)), &[]);
}
