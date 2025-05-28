use bevy::{
    ecs::query::QueryFilter,
    reflect::{DynamicEnum, DynamicVariant},
    state::commands,
};

use crate::prelude::*;

pub struct AppStatesPlugin;

impl Plugin for AppStatesPlugin {
    fn build(&self, app: &mut App) {
        // our states
        app.init_state::<AppState>();
        for state in enum_iterator::all::<AppState>() {
            app.add_systems(
                OnExit(state),
                // FIXME: vendor helper from iyes_bevy_extras?
                despawn_all_recursive::<With<StateDespawnMarker>>,
            );
        }
        app.add_systems(OnEnter(AppState::Restart), restart);
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

pub fn restart(mut next_state: ResMut<NextState<AppState>>) {
    next_state.set(AppState::InGame);
}

fn despawn_all_recursive<F: QueryFilter>(
    mut commands: Commands,
    q: Query<Entity, F>,
) {
    for entity in &q {
        commands.entity(entity).despawn_recursive();
    }
}
