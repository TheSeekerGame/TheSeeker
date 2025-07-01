//! Everything to do with the in-game camera(s)

use std::f32::consts::PI;
use rand::{thread_rng, Rng};

use crate::game::player::Player;

// use crate::graphics::post_processing::darkness::DarknessSettings;
use crate::graphics::post_processing::darkness::DarknessSettings;
use crate::graphics::post_processing::vignette::VignetteSettings;
use crate::level::MainBackround;
use crate::prelude::*;
use bevy::render::camera::ClearColorConfig;
use bevy::render::view::RenderLayers;
use bevy::ecs::schedule::IntoScheduleConfigs;
use bevy::render::camera::{Projection, OrthographicProjection, Camera};
use bevy::core_pipeline::core_2d::Camera2d;
use bevy::ecs::query::QuerySingleError;

// Scale factor for the camera's orthographic projection.
// 1/5 = 0.2 means 5 game pixels = 1 screen pixel at default zoom.
const PROJECTION_SCALE: f32 = 1.0 / 5.0;

pub struct CameraPlugin;

impl Plugin for CameraPlugin {
    fn build(&self, app: &mut App) {
        // app.register_clicommand_args("camera_at", cli_camera_at);
        // app.register_clicommand_noargs(
        //     "camera_limits",
        //     cli_camera_limits_noargs,
        // );
        // app.register_clicommand_args("camera_limits", cli_camera_limits_args);
        app.add_systems(
            OnEnter(AppState::InGame),
            setup_main_camera,
        );
        // app.add_systems(Update, (manage_camera_projection,));

        app.insert_resource(CameraRig {
            target: Default::default(),
            camera_position: Default::default(),
            move_speed: 1.9,
            lead_direction: LeadDirection::Forward,
            lead_amount: 20.0,
            lead_buffer: 10.0,
        });
        app.add_systems(
            GameTickUpdate,
            (
                camera_rig_follow_player,
                update_camera.after(camera_rig_follow_player),
                update_screen_shake.run_if(resource_exists::<CameraShake>),
            ),
        );
    }
}

/// Marker component for the main gameplay camera
#[derive(Component)]
pub struct MainCamera;

/// Marker component for the player camera (renders player on top of post-processing effects)
#[derive(Component)]
pub struct PlayerCamera;

#[derive(Resource)]
/// Tracks the target location of the camera, as well as internal state for interpolation.
pub struct CameraRig {
    /// The camera is moved towards this position smoothly.
    target: Vec2,
    /// the "base" position of the camera before screen shake is applied.
    camera_position: Vec2,
    /// The factor used in lerping to move the rig.
    move_speed: f32,
    /// Keeps track if the camera is leading ahead, or behind the player.
    lead_direction: LeadDirection,
    /// Defines how far ahead the camera will lead the player by.
    lead_amount: f32,
    /// Defines how far away the player can get going in the unanticipated direction
    /// before the camera switches to track that direction.
    lead_buffer: f32,
}

enum LeadDirection {
    Backward,
    Forward,
}

/// Limits to the viewable gameplay area.
///
/// The main camera should never display anything outside of these limits.
#[derive(Component)]
pub struct GameViewLimits(Rect);

pub(crate) fn setup_main_camera(mut commands: Commands) {
    #[cfg(feature = "iyes_perf_ui")]
    commands.spawn((
        iyes_perf_ui::PerfUiCompleteBundle::default(),
        StateDespawnMarker,
    ));

    // Custom orthographic projection with our desired scale.
    let projection = Projection::Orthographic(OrthographicProjection {
        scale: PROJECTION_SCALE,
        ..OrthographicProjection::default_2d()
    });

    // Spawn the main 2D camera.
    commands.spawn((
        // Minimal 2-D camera marker – Bevy populates the remaining required components automatically.
        Camera2d,
        // Override the automatically-added `Camera` component so we can enable HDR.
        Camera {
            hdr: true,
            ..Default::default()
        },
        // Our custom projection.
        projection,
        // Custom components specific to the game.
        MainCamera,
        GameViewLimits(Rect::new(0.0, 0.0, 640.0, 480.0)),
        StateDespawnMarker,
        VignetteSettings::default(),
        DarknessSettings::default(),
        // Render layers: world (0), light sources (1) and player (2)
        RenderLayers::from_layers(&[0, 1, 2]),
        Name::new("MainCamera"),
    ));
}

fn _manage_camera_projection(// mut q_cam: Query<&mut OrthographicProjection, With<MainCamera>>,
                            // mut q_window: Query<&Window, With<PrimaryWindow>>,
) {
    // TODO
}

/// Updates the Camera rig (ie, the camera target) based on where the player is going.
fn camera_rig_follow_player(
    mut rig: ResMut<CameraRig>,
    player_query: Query<&Transform, (With<Player>, Without<MainCamera>)>,
    time: Res<Time>,
) {
    let Ok(player_transform) = player_query.get_single() else {
        return;
    };
    // Default state is to predict the player goes forward, ie "right"
    let delta_x = player_transform.translation.x - rig.target.x;

    match rig.lead_direction {
        LeadDirection::Backward => {
            if delta_x < rig.lead_amount {
                rig.target.x = player_transform.translation.x - rig.lead_amount
            } else if delta_x > rig.lead_amount + rig.lead_buffer {
                rig.lead_direction = LeadDirection::Forward
            }
        },
        LeadDirection::Forward => {
            if delta_x > -rig.lead_amount {
                rig.target.x = player_transform.translation.x + rig.lead_amount
            } else if delta_x < -rig.lead_amount - rig.lead_buffer {
                rig.lead_direction = LeadDirection::Backward
            }
        },
    }

    rig.target.y = player_transform.translation.y;

    if (rig.camera_position - rig.target).length() < PROJECTION_SCALE {
        // Stop lerping if already at the target
        rig.camera_position = rig.target;
    } else {
        rig.camera_position = rig.camera_position.lerp(
            rig.target,
            time.delta_secs() * rig.move_speed,
        );
    }
}

/// Camera updates the camera position to smoothly interpolate to the
/// rig location. also applies camera shake, and limits camera within the level boundaries
pub(crate) fn update_camera(
    mut camera_query: Query<
        (&mut Transform, &Projection),
        With<MainCamera>,
    >,
    rig: Res<CameraRig>,
    backround_query: Query<
        (&LayerMetadata, &Transform),
        (With<MainBackround>, Without<MainCamera>),
    >,
    camera_shake: Option<Res<CameraShake>>,
) {
    let (mut camera_transform, projection) = match camera_query.get_single_mut() {
        Ok(tuple) => tuple,
        Err(QuerySingleError::NoEntities(_)) => return,
        Err(QuerySingleError::MultipleEntities(_)) => {
            // More than one MainCamera found; this should not happen in normal gameplay.
            // Log a warning and skip this update to avoid nondeterministic behavior.
            warn!("Multiple MainCamera entities detected – skipping camera update this frame.");
            return;
        },
    };

    // Update camera position
    camera_transform.translation.x = rig.camera_position.x;
    camera_transform.translation.y = rig.camera_position.y;

    // Only proceed if this is an orthographic camera.
    let camera_rect = if let Projection::Orthographic(ortho) = projection {
        ortho.area
    } else {
        return;
    };

    if let Ok((bg_layer, bg_transform)) = backround_query.get_single() {
        let background_rect = background_rect(bg_layer, bg_transform);

        clamp_camera_to_edge(
            &mut camera_transform,
            background_rect,
            camera_rect,
        );

        // Apply screen shake after camera is clamped so that camera still shakes at the edges
        if let Some(camera_shake) = camera_shake {
            camera_shake.apply(&mut camera_transform);
        }

        // Apply another clamp so we don't show the edge of the level
        clamp_camera_to_edge(
            &mut camera_transform,
            background_rect,
            camera_rect,
        );
    }
}

fn background_rect(bg_layer: &LayerMetadata, bg_transform: &Transform) -> Rect {
    let bg_width = (bg_layer.c_wid * bg_layer.grid_size) as f32;
    let bg_height = (bg_layer.c_hei * bg_layer.grid_size) as f32;

    // The backround width and height actually have 3 pixels extra padding on the far
    // right/upper sides. This accounts for that.
    let bg_max = Vec2::new(bg_width - 3.0, bg_height - 3.0);

    // bottom left corner of the background is zero/minimum corner, because
    // that's how LDtk imports it.
    Rect::from_corners(
        bg_max + bg_transform.translation.xy(),
        bg_transform.translation.xy(),
    )
}

fn clamp_camera_to_edge(
    camera_transform: &mut Transform,
    background_rect: Rect,
    camera_rect: Rect,
) {
    let xy = camera_transform.translation.xy().clamp(
        background_rect.min + camera_rect.half_size(),
        background_rect.max - camera_rect.half_size(),
    );
    camera_transform.translation = xy.extend(camera_transform.translation.z);
}

#[derive(Resource, Clone)]
pub struct CameraShake {
    strength: f32,
    c_offset: Vec2,
    freq: f32,
    dir: Vec2,
    timer: Timer,
    sub_timer: Timer,
}

impl CameraShake {
    pub fn new(strength: f32, t: f32, freq: f32) -> Self {
        let rand_a = thread_rng().gen_range(0.0..=360.0);
        let dir = Vec2::from_angle(rand_a as f32 * PI * 2.0);

        Self {
            strength,
            freq,
            timer: Timer::from_seconds(t, TimerMode::Once),
            sub_timer: Timer::from_seconds(t / freq, TimerMode::Repeating),
            c_offset: Vec2::ZERO,
            dir,
        }
    }

    pub fn apply(&self, camera_transform: &mut Transform) {
        camera_transform.translation.x += self.c_offset.x;
        camera_transform.translation.y += self.c_offset.y;
    }
}

pub fn update_screen_shake(
    mut commands: Commands,
    //    mut cam_query: Query<(Entity, &mut CameraShake, &mut Transform), (With<PlayerCamera>)>,
    time: Res<Time<Virtual>>,
    mut shake: ResMut<CameraShake>,
) {
    let freq = shake.freq;
    let ratio = shake.timer.fraction();
    let decay = 1.0 - ratio.powi(2);

    let t = freq * ratio * PI * 2.0;
    let s = t.sin();

    const TAN_FREQ_SCALE: f32 = 2.;
    const TAN_AMP_SCALE: f32 = 0.5;

    let tan_s = (TAN_FREQ_SCALE * t).sin();

    if shake.sub_timer.finished() {
        let rand_a = thread_rng().gen_range(0.0..=360.0);
        shake.dir = Vec2::from_angle(rand_a as f32 * PI * 2.0);
    }

    let val = s * decay;
    let tan_val = tan_s * decay * TAN_AMP_SCALE;

    let tan_dir = Vec3::Z.cross(shake.dir.extend(0.)).truncate();

    let delta = shake.dir * val * shake.strength;
    let tan_delta = tan_dir * tan_val * shake.strength;
    //    let angle = val * shake.strength * 0.0001;

    shake.c_offset = delta + tan_delta;

    shake.timer.tick(time.delta());
    shake.sub_timer.tick(time.delta());

    if shake.timer.finished() {
        commands.remove_resource::<CameraShake>();
    }
}

// fn cli_camera_at(
//     In(args): In<Vec<String>>,
//     mut q_cam: Query<&mut Transform, With<MainCamera>>,
// ) {
//     if args.len() != 2 {
//         error!("\"camera_at <x> <y>\"");
//         return;
//     }
//     if let Ok(mut xf_cam) = q_cam.get_single_mut() {
//         if let (Ok(x), Ok(y)) = (args[0].parse(), args[1].parse()) {
//             xf_cam.translation.x = x;
//             xf_cam.translation.y = y;
//         } else {
//             error!("\"camera_at <x> <y>\": args must be numeric values");
//         }
//     }
// }
//
// fn cli_camera_limits_noargs(q_cam: Query<&GameViewLimits, With<MainCamera>>) {
//     if let Ok(limits) = q_cam.get_single() {
//         info!(
//             "Game Camera limits: {} {} {} {}",
//             limits.0.min.x, limits.0.min.y, limits.0.max.x, limits.0.max.y
//         );
//     } else {
//         error!("Game Camera not found!");
//     }
// }
//
// fn cli_camera_limits_args(
//     In(args): In<Vec<String>>,
//     mut q_cam: Query<&mut GameViewLimits, With<MainCamera>>,
// ) {
//     if args.len() != 4 {
//         error!("\"camera_limits <x0> <y0> <x1> <y1>\"");
//         return;
//     }
//     if let Ok(mut limits) = q_cam.get_single_mut() {
//         if let (Ok(x0), Ok(y0), Ok(x1), Ok(y1)) = (
//             args[0].parse(),
//             args[1].parse(),
//             args[2].parse(),
//             args[3].parse(),
//         ) {
//             limits.0 = Rect::new(x0, y0, x1, y1);
//         } else {
//             error!("\"camera_limits <x0> <y0> <x1> <y1>\": args must be numeric values");
//         }
//     }
// }
