use bevy::reflect::{DynamicEnum, DynamicVariant};

use crate::prelude::*;

pub struct AppStatesPlugin;

impl Plugin for AppStatesPlugin {
    fn build(&self, app: &mut App) {
        // our states
        app.init_state::<AppState>();
        for state in enum_iterator::all::<AppState>() {
            app.add_systems(
                OnExit(state),
                despawn_all_recursive::<With<StateDespawnMarker>>,
            );
        }
        app.add_systems(OnEnter(AppState::Restart), restart);
        app.register_clicommand_args("AppState", cli_appstate);
    }
}

/// State type: Which "screen" is the app in?
#[derive(Debug, PartialEq, Eq, Clone, Copy, Hash, Default, States)]
#[derive(Reflect)]
#[derive(enum_iterator::Sequence)]
pub enum AppState {
    /// Initial loading screen at startup
    #[default]
    AssetsLoading,
    /// Main Menu
    MainMenu,
    /// Gameplay
    InGame,
    Restart,
}

/// Marker for entities that should be despawned on `AppState` transition.
///
/// Use this on entities spawned when entering specific states, that need to
/// be cleaned up when exiting.
#[derive(Component)]
pub struct StateDespawnMarker;

/// CliCommand for switching state
fn cli_appstate(
    In(args): In<Vec<String>>,
    mut next: ResMut<NextState<AppState>>,
) {
    if args.len() != 1 {
        error!("\"appstate <Value>\"");
        return;
    }

    let dyn_state = DynamicEnum::new(&args[0], DynamicVariant::Unit);
    if let Some(state) = FromReflect::from_reflect(&dyn_state) {
        next.set(state);
    } else {
        error!("Invalid app state: {}", args[0]);
    }
}
pub fn restart(mut next_state: ResMut<NextState<AppState>>) {
    next_state.set(AppState::InGame);
}
