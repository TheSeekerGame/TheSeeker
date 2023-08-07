//! Everything to do with the in-game camera(s)

use crate::prelude::*;

pub struct CameraPlugin;

impl Plugin for CameraPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(AppState::InGame), setup_main_camera);
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

fn setup_main_camera(
    mut commands: Commands,
) {
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
