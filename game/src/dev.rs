use bevy::diagnostic::{FrameTimeDiagnosticsPlugin, LogDiagnosticsPlugin};

use crate::gamestate::{pause, unpause};
use crate::prelude::*;

pub struct DevPlugin;

impl Plugin for DevPlugin {
    fn build(&self, app: &mut App) {

        app.add_systems(
            GameTickUpdate,
            (
                pause.run_if(in_state(GameState::Playing)),
                unpause.run_if(in_state(GameState::Paused)),
            ),
        );
        app.add_plugins((
            FrameTimeDiagnosticsPlugin,
            LogDiagnosticsPlugin::default(),
            // FilterQueryInspectorPlugin::<(With<Enemy>)>::default(),
            // SteppingEguiPlugin::default().add_schedule(GameTickUpdate),
        ));
        #[cfg(feature = "inspector")]
        app.add_plugins(
            bevy_inspector_egui::quick::WorldInspectorPlugin::new(),
        );
    }
}



/// Temporary function to use during development
///
/// If there is no proper code to set up a camera in a given app state (or whatever)
/// yet, use this to spawn a default 2d camera.
#[allow(dead_code)]
fn debug_setup_camera(mut commands: Commands) {
    commands.spawn((
        Camera2dBundle::default(),
        StateDespawnMarker,
    ));
}

#[allow(dead_code)]
fn debug_spawn_player(mut commands: Commands) {
    use crate::game::player::PlayerBlueprint;

    commands.spawn((
        PlayerBlueprint,
        SpatialBundle { ..default() },
    ));
}
