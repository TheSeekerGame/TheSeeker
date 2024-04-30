//! Everything to do with the in-game camera(s)

use crate::game::player::Player;
use crate::graphics::darkness::DarknessSettings;
use crate::prelude::*;
use bevy::core_pipeline::bloom::BloomSettings;
use bevy::core_pipeline::tonemapping::Tonemapping;
use ran::ran_f64_range;

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
            trauma: 0.0,
        });
        app.add_systems(GameTickUpdate, camera_rig_follow_player);
        app.add_systems(GameTickUpdate, update_rig_trauma);
        app.add_systems(
            GameTickUpdate,
            update_camera_rig.after(camera_rig_follow_player),
        );
    }
}

/// For spawning the main gameplay camera
#[derive(Bundle)]
struct MainCameraBundle {
    camera: Camera2dBundle,
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
    /// Applies screen shake based on this amount.
    ///
    /// value decreases over time. To use, add some amount based on impact intensity.
    ///
    /// 10.0 is a lot; death? 1.0 minor impact
    trauma: f32,
}

/// Limits to the viewable gameplay area.
///
/// The main camera should never display anything outside of these limits.
#[derive(Component)]
pub struct GameViewLimits(Rect);

fn setup_main_camera(mut commands: Commands) {
    let mut camera = Camera2dBundle {
        camera: Camera {
            hdr: true,
            ..default()
        },
        tonemapping: Tonemapping::AcesFitted,
        ..default()
    };
    camera.projection.scale = 1.0 / 6.0;

    commands.spawn((
        MainCameraBundle {
            camera,
            marker: MainCamera,
            despawn: StateDespawnMarker,
            // TODO: manage this from somewhere
            limits: GameViewLimits(Rect::new(0.0, 0.0, 640.0, 480.0)),
        },
        BloomSettings::NATURAL,
        DarknessSettings {
            bg_light_level: 1.0,
            lantern_position: Default::default(),
            lantern: 0.0,
            lantern_color: Vec3::new(0.965, 0.882, 0.678),
            bg_light_color: Vec3::new(0.761, 0.773, 0.8),
        },
    ));
}

fn manage_camera_projection(// mut q_cam: Query<&mut OrthographicProjection, With<MainCamera>>,
                            // mut q_window: Query<&Window, With<PrimaryWindow>>,
) {
    // TODO
}

/// Updates the Camera rig (ie, the camera target) based on where the player is going.
fn camera_rig_follow_player(
    mut rig: ResMut<CameraRig>,
    q_player: Query<&Transform, (With<Player>, Without<MainCamera>)>,
    // Keeps track if the camera is leading ahead, or behind the player
    mut lead_bckwrd: Local<bool>,
) {
    let Ok(player_xform) = q_player.get_single() else {
        return;
    };
    // define how far away the player can get going in the unanticipated direction
    // before the camera switches to track that direction
    let max_err = 10.0;
    // Define how far ahead the camera will lead the player by
    let lead_amnt = 15.0;

    // Default state is to predict the player goes forward, ie "right"
    let delta_x = player_xform.translation.x - rig.target.x;

    if !*lead_bckwrd {
        if delta_x > -lead_amnt {
            rig.target.x = player_xform.translation.x + lead_amnt
        } else if delta_x < -lead_amnt - max_err {
            *lead_bckwrd = !*lead_bckwrd
        }
    } else {
        if delta_x < lead_amnt {
            rig.target.x = player_xform.translation.x - lead_amnt
        } else if delta_x > lead_amnt + max_err {
            *lead_bckwrd = !*lead_bckwrd
        }
    }
    //rig.position.x = player_xform.translation.x;

    rig.target.y = player_xform.translation.y;
}

/// Tiny system that just makes sure trauma is always going down linearly
pub(crate) fn update_rig_trauma(
    mut rig: ResMut<CameraRig>,
    time: Res<Time>,
    keyboard_input: Res<ButtonInput<KeyCode>>,
) {
    rig.trauma = (rig.trauma - 1.75 * time.delta_seconds()).max(0.0);
    #[cfg(feature = "dev")]
    // Tests different levels based on number key if in dev
    {
        let number_keys = [
            KeyCode::Digit1,
            KeyCode::Digit2,
            KeyCode::Digit3,
            KeyCode::Digit4,
            KeyCode::Digit5,
            KeyCode::Digit6,
            KeyCode::Digit7,
            KeyCode::Digit8,
            KeyCode::Digit9,
            KeyCode::Digit0,
        ];
        for (i, key) in number_keys.iter().enumerate() {
            if keyboard_input.pressed(*key) {
                rig.trauma = (i as f32 + 1.0) * 0.1;
            }
        }
    }
}

/// Camera updates the camera position to smoothly interpolate to the
/// rig location. also applies camera shake
pub(crate) fn update_camera_rig(
    mut q_cam: Query<&mut Transform, With<MainCamera>>,
    mut rig: ResMut<CameraRig>,
    time: Res<Time>,
) {
    let Ok(mut cam_xform) = q_cam.get_single_mut() else {
        return;
    };

    let speed = 2.34;

    let new_xy = rig.camera.lerp(rig.target, time.delta_seconds() * speed);

    rig.camera = new_xy;

    let shake = rig.trauma.powi(3);

    // screen shake amounts
    let angle = 2.5 * (std::f32::consts::PI / 180.0) * shake * ran_f64_range(-1.0..=1.0) as f32;
    let offset_x = 20.0 * shake * ran_f64_range(-1.0..=1.0) as f32;
    let offset_y = 20.0 * shake * ran_f64_range(-1.0..=1.0) as f32;

    cam_xform.rotation.z = 0.0 + angle;
    cam_xform.translation.x = rig.camera.x + offset_x;
    cam_xform.translation.y = rig.camera.y + offset_y;
}

fn cli_camera_at(In(args): In<Vec<String>>, mut q_cam: Query<&mut Transform, With<MainCamera>>) {
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
