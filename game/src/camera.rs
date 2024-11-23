//! Everything to do with the in-game camera(s)

use std::f32::consts::PI;

use bevy::core_pipeline::bloom::BloomSettings;
use bevy::core_pipeline::prepass::DepthPrepass;
use bevy::core_pipeline::tonemapping::Tonemapping;
use iyes_perf_ui::PerfUiCompleteBundle;
use ran::ran_f64_range;

use crate::game::player::Player;
use crate::graphics::darkness::DarknessSettings;
use crate::graphics::dof::{DepthOfFieldMode, DepthOfFieldSettings};
use crate::level::MainBackround;
use crate::prelude::*;

const CAMERA_RIG_MOVE_SPEED: f32 = 1.9;

pub struct CameraPlugin;

impl Plugin for CameraPlugin {
    fn build(&self, app: &mut App) {
        app.register_clicommand_args("camera_at", cli_camera_at);
        app.register_clicommand_noargs(
            "camera_limits",
            cli_camera_limits_noargs,
        );
        app.register_clicommand_args("camera_limits", cli_camera_limits_args);
        app.add_systems(
            OnEnter(AppState::InGame),
            setup_main_camera,
        );
        // app.add_systems(Update, (manage_camera_projection,));

        app.insert_resource(CameraRig {
            target: Default::default(),
            camera: Default::default(),
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

/// For spawning the main gameplay camera
#[derive(Bundle)]
struct MainCameraBundle {
    camera: Camera3dBundle,
    limits: GameViewLimits,
    marker: MainCamera,
    despawn: StateDespawnMarker,
}

/// Marker component for the main gameplay camera
#[derive(Component)]
pub struct MainCamera;

#[derive(Resource)]
/// Tracks the target location of the camera, as well as internal state for interpolation.
pub struct CameraRig {
    /// The camera is moved towards this position smoothly
    target: Vec2,
    /// the "base" position of the camera before screen shake is applied
    camera: Vec2,
}

/// Limits to the viewable gameplay area.
///
/// The main camera should never display anything outside of these limits.
#[derive(Component)]
pub struct GameViewLimits(Rect);

pub(crate) fn setup_main_camera(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    commands.spawn((
        PerfUiCompleteBundle::default(),
        StateDespawnMarker,
    ));
    let mut camera = Camera2dBundle {
        camera: Camera {
            hdr: true,
            ..default()
        },
        tonemapping: Tonemapping::None,
        ..default()
    };
    camera.projection.scale = 1.0 / 5.0;

    let mut camera3d = Camera3dBundle {
        camera: Camera {
            hdr: true,
            ..default()
        },
        tonemapping: Tonemapping::None,
        ..default()
    };

    camera3d.projection = Projection::Orthographic(camera.projection);
    camera3d.transform = camera.transform;
    camera3d.frustum = camera.frustum;
    camera3d.transform.translation.z = 0.25;

    // TODO make tilemap write to depth buffer somehow.
    // bring up in meeting!

    commands.spawn((
        MainCameraBundle {
            camera: camera3d,
            marker: MainCamera,
            despawn: StateDespawnMarker,
            // TODO: manage this from somewhere
            limits: GameViewLimits(Rect::new(0.0, 0.0, 640.0, 480.0)),
        },
        // Needed so that depth buffers are stored so depth of field works
        DepthPrepass,
        DepthOfFieldSettings {
            mode: DepthOfFieldMode::Bokeh,
            focal_distance: 0.25,
            sensor_height: 0.008,
            aperture_f_stops: 1.0,
            max_circle_of_confusion_diameter: 68.8,
            max_depth: 500.0,
        },
        BloomSettings::NATURAL,
        DarknessSettings {
            bg_light_level: 1.0,
            lantern_position: Default::default(),
            lantern: 0.0,
            lantern_color: Vec3::new(0.965, 0.882, 0.678),
            bg_light_color: Vec3::new(0.761, 0.773, 0.8),
        },
        Name::new("MainCamera"),
    ));

    let debug_material = materials.add(StandardMaterial { ..default() });
}

fn manage_camera_projection(// mut q_cam: Query<&mut OrthographicProjection, With<MainCamera>>,
                            // mut q_window: Query<&Window, With<PrimaryWindow>>,
) {
    // TODO
}

/// Updates the Camera rig (ie, the camera target) based on where the player is going.
fn camera_rig_follow_player(
    mut rig: ResMut<CameraRig>,
    player_query: Query<&Transform, (With<Player>, Without<MainCamera>)>,
    // Keeps track if the camera is leading ahead, or behind the player
    mut lead_backward: Local<bool>,
    time: Res<Time>,
) {
    let Ok(player_transform) = player_query.get_single() else {
        return;
    };
    // define how far away the player can get going in the unanticipated direction
    // before the camera switches to track that direction
    let max_err = 10.0;
    // Define how far ahead the camera will lead the player by
    let lead_amount = 20.0;

    // Default state is to predict the player goes forward, ie "right"
    let delta_x = player_transform.translation.x - rig.target.x;

    if !*lead_backward {
        if delta_x > -lead_amount {
            rig.target.x = player_transform.translation.x + lead_amount
        } else if delta_x < -lead_amount - max_err {
            *lead_backward = !*lead_backward
        }
    } else {
        if delta_x < lead_amount {
            rig.target.x = player_transform.translation.x - lead_amount
        } else if delta_x > lead_amount + max_err {
            *lead_backward = !*lead_backward
        }
    }

    rig.target.y = player_transform.translation.y;

    rig.camera = rig.camera.lerp(
        rig.target,
        time.delta_seconds() * CAMERA_RIG_MOVE_SPEED,
    );
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
    let Ok((mut camera_transform, projection)) = camera_query.get_single_mut()
    else {
        return;
    };

    let Projection::Orthographic(ortho_projection) = projection else {
        return;
    };

    camera_transform.translation.x = rig.camera.x;
    camera_transform.translation.y = rig.camera.y;

    if let Ok((bg_layer, bg_transform)) = backround_query.get_single() {
        let camera_rect = ortho_projection.area;
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
        let rand_a = ran_f64_range(0.0..=360.0);
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
        let rand_a = ran_f64_range(0.0..=360.0);
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

fn cli_camera_at(
    In(args): In<Vec<String>>,
    mut q_cam: Query<&mut Transform, With<MainCamera>>,
) {
    if args.len() != 2 {
        error!("\"camera_at <x> <y>\"");
        return;
    }
    if let Ok(mut xf_cam) = q_cam.get_single_mut() {
        if let (Ok(x), Ok(y)) = (args[0].parse(), args[1].parse()) {
            xf_cam.translation.x = x;
            xf_cam.translation.y = y;
        } else {
            error!("\"camera_at <x> <y>\": args must be numeric values");
        }
    }
}

fn cli_camera_limits_noargs(q_cam: Query<&GameViewLimits, With<MainCamera>>) {
    if let Ok(limits) = q_cam.get_single() {
        info!(
            "Game Camera limits: {} {} {} {}",
            limits.0.min.x, limits.0.min.y, limits.0.max.x, limits.0.max.y
        );
    } else {
        error!("Game Camera not found!");
    }
}

fn cli_camera_limits_args(
    In(args): In<Vec<String>>,
    mut q_cam: Query<&mut GameViewLimits, With<MainCamera>>,
) {
    if args.len() != 4 {
        error!("\"camera_limits <x0> <y0> <x1> <y1>\"");
        return;
    }
    if let Ok(mut limits) = q_cam.get_single_mut() {
        if let (Ok(x0), Ok(y0), Ok(x1), Ok(y1)) = (
            args[0].parse(),
            args[1].parse(),
            args[2].parse(),
            args[3].parse(),
        ) {
            limits.0 = Rect::new(x0, y0, x1, y1);
        } else {
            error!("\"camera_limits <x0> <y0> <x1> <y1>\": args must be numeric values");
        }
    }
}
