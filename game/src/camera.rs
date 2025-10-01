//! Everything to do with the in-game camera(s)

use rand::{rng, Rng};
use std::f32::consts::PI;

use crate::game::enemy::Enemy;
use crate::game::player::skills::types::flicker_strike_metadata;
use crate::game::player::{states::FlickerStriking, Player};

// use crate::graphics::post_processing::darkness::DarknessSettings;
use crate::graphics::post_processing::darkness::DarknessSettings;
use crate::graphics::post_processing::vignette::VignetteSettings;
use crate::level::MainBackround;
use crate::prelude::*;
use bevy::core_pipeline::core_2d::Camera2d;
use bevy::ecs::query::QuerySingleError;
use bevy::ecs::schedule::IntoScheduleConfigs;
use bevy::render::camera::{Camera, OrthographicProjection, Projection};
use bevy::render::view::RenderLayers;

// Scale factor for the camera's orthographic projection.
// 1/5 = 0.2 means 5 game pixels = 1 screen pixel at default zoom.
const PROJECTION_SCALE: f32 = 1.0 / 5.0;

pub struct CameraPlugin;

impl Plugin for CameraPlugin {
    fn build(&self, app: &mut App) {
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
pub struct GameViewLimits(#[allow(dead_code)] Rect);

pub(crate) fn setup_main_camera(mut commands: Commands) {
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
    player_query: Query<
        (&Transform, Has<FlickerStriking>),
        (With<Player>, Without<MainCamera>),
    >,
    enemy_query: Query<&Transform, (With<Enemy>, Without<Player>)>,
    time: Res<Time>,
    camera_shake: Option<Res<CameraShake>>, // pause lerp while shaking
) {
    let Ok((player_transform, is_flicker_striking)) = player_query.single()
    else {
        return;
    };

    if is_flicker_striking {
        // During Flicker Strike: camera focuses on average of enemies in range (excluding player)
        let player_pos = player_transform.translation.truncate();
        let mut positions_sum = Vec2::ZERO;
        let mut position_count = 0;

        // Add all enemies within Flicker Strike range
        for enemy_transform in enemy_query.iter() {
            let enemy_pos = enemy_transform.translation.truncate();
            if player_pos.distance(enemy_pos)
                <= flicker_strike_metadata().range
            {
                positions_sum += enemy_pos;
                position_count += 1;
            }
        }

        // Set target to average enemy position, or player position if no enemies in range
        if position_count > 0 {
            rig.target = positions_sum / position_count as f32;
        } else {
            // Fallback to player position if no enemies in range
            rig.target = player_pos;
        }

        // Pause lerping for the first few ticks of an active screen shake
        let pause_follow = camera_shake
            .as_ref()
            .map(|s| s.pause_lerp_ticks > 0)
            .unwrap_or(false);
        if !pause_follow {
            if (rig.camera_position - rig.target).length() < PROJECTION_SCALE {
                rig.camera_position = rig.target;
            } else {
                rig.camera_position = rig.camera_position.lerp(
                    rig.target,
                    time.delta_secs() * rig.move_speed,
                );
            }
        }
    } else {
        // Normal camera behavior with lead/lag when not Flicker Striking
        let delta_x = player_transform.translation.x - rig.target.x;

        match rig.lead_direction {
            LeadDirection::Backward => {
                if delta_x < rig.lead_amount {
                    rig.target.x =
                        player_transform.translation.x - rig.lead_amount
                } else if delta_x > rig.lead_amount + rig.lead_buffer {
                    rig.lead_direction = LeadDirection::Forward
                }
            },
            LeadDirection::Forward => {
                if delta_x > -rig.lead_amount {
                    rig.target.x =
                        player_transform.translation.x + rig.lead_amount
                } else if delta_x < -rig.lead_amount - rig.lead_buffer {
                    rig.lead_direction = LeadDirection::Backward
                }
            },
        }

        rig.target.y = player_transform.translation.y;

        // Pause lerping for the first few ticks of an active screen shake
        let pause_follow = camera_shake
            .as_ref()
            .map(|s| s.pause_lerp_ticks > 0)
            .unwrap_or(false);
        if !pause_follow {
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
    }
}

/// Camera updates the camera position to smoothly interpolate to the
/// rig location. also applies camera shake, and limits camera within the level boundaries
pub(crate) fn update_camera(
    mut camera_query: Query<(&mut Transform, &Projection), With<MainCamera>>,
    rig: Res<CameraRig>,
    backround_query: Query<
        (&LayerMetadata, &Transform),
        (With<MainBackround>, Without<MainCamera>),
    >,
    camera_shake: Option<Res<CameraShake>>,
) {
    let (mut camera_transform, projection) = match camera_query.single_mut() {
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

    if let Ok((bg_layer, bg_transform)) = backround_query.single() {
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
    /// Pause camera follow lerp for the first N game ticks of the shake
    pause_lerp_ticks: u32,
}

impl CameraShake {
    pub fn new(strength: f32, t: f32, freq: f32) -> Self {
        let rand_a = rng().random_range(0.0..=360.0);
        let dir = Vec2::from_angle(rand_a as f32 * PI * 2.0);

        Self {
            strength,
            freq,
            timer: Timer::from_seconds(t, TimerMode::Once),
            sub_timer: Timer::from_seconds(t / freq, TimerMode::Repeating),
            c_offset: Vec2::ZERO,
            dir,
            pause_lerp_ticks: 2,
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
        let rand_a = rng().random_range(0.0..=360.0);
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
    // Count down the initial lerp pause in game ticks
    if shake.pause_lerp_ticks > 0 {
        shake.pause_lerp_ticks -= 1;
    }

    if shake.timer.finished() {
        commands.remove_resource::<CameraShake>();
    }
}
