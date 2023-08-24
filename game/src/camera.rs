//! Everything to do with the in-game camera(s)

use crate::prelude::*;

pub struct CameraPlugin;

impl Plugin for CameraPlugin {
    fn build(&self, app: &mut App) {
        app.register_clicommand_args("camera_at", cli_camera_at);
        app.register_clicommand_args("camera_limits", cli_camera_limits);
        app.add_systems(
            OnEnter(AppState::InGame),
            setup_main_camera,
        );
        app.add_systems(Update, manage_camera_projection);
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

/// Limits to the viewable gameplay area.
///
/// The main camera should never display anything outside of these limits.
#[derive(Component)]
pub struct GameViewLimits(Rect);

fn setup_main_camera(mut commands: Commands) {
    let mut camera = Camera2dBundle::default();

    camera.projection.scale = 1.0 / 6.0;

    commands.spawn(MainCameraBundle {
        camera,
        marker: MainCamera,
        despawn: StateDespawnMarker,
        // TODO: manage this from somewhere
        limits: GameViewLimits(Rect::new(0.0, 0.0, 640.0, 480.0)),
    });
}

fn manage_camera_projection(
    // mut q_cam: Query<&mut OrthographicProjection, With<MainCamera>>,
    // mut q_window: Query<&Window, With<PrimaryWindow>>,
) {
    // TODO
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

fn cli_camera_limits(
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
