use bevy::ecs::query::QueryFilter;

use crate::assets::GameUiAssets;
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

        // Start the first expedition once all required collections and audio are ready.
        app.add_systems(
            Update,
            start_first_expedition.run_if(in_state(AppState::AssetsLoading)),
        );
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

#[derive(bevy::ecs::system::SystemParam)]
struct StartupAssets<'w> {
    preloaded: Res<'w, PreloadedAssets>,
    server: Res<'w, AssetServer>,
    game_ui: Option<Res<'w, GameUiAssets>>,
    pixel_art: Res<'w, crate::ui::pixel_canvas::ArtLibrary>,
}

fn start_first_expedition(
    mut next_state: ResMut<NextState<AppState>>,
    time: Res<Time<Real>>,
    audio: Option<Res<theseeker_engine::audio::AudioEngine>>,
    assets: StartupAssets,
    mut min_wait_timer: Local<Option<Timer>>,
    mut commands: Commands,
    mut starter: crate::ui::mainmenu::WorldStarter,
) {
    let timer = min_wait_timer.get_or_insert_with(|| {
        Timer::from_seconds(
            0.5,
            TimerMode::Once,
        )
    });
    timer.tick(time.delta());

    if !timer.is_finished() || audio.is_some_and(|audio| !audio.preparation_ready(&assets.preloaded, &assets.server)) {
        return;
    }

    if assets.game_ui.is_none() || !assets.pixel_art.is_ready(&assets.server) {
        return;
    }

    info!("All startup asset collections loaded, starting the first expedition");
    starter.generated(&mut commands, &mut next_state, AppState::AssetsLoading);
}

fn despawn_all_recursive<F: QueryFilter>(
    mut commands: Commands,
    q: Query<Entity, F>,
) {
    for entity in &q {
        // Both a parent and its descendants may carry the state marker.
        commands.entity(entity).try_despawn();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn state_cleanup_handles_marked_parents_and_children_in_one_flush() {
        let mut app = App::new();
        app.set_error_handler(bevy::ecs::error::panic);
        app.add_systems(Update, despawn_all_recursive::<With<StateDespawnMarker>>);
        let parent = app.world_mut().spawn(StateDespawnMarker).id();
        let child = app.world_mut().spawn((StateDespawnMarker, ChildOf(parent))).id();
        app.update();
        assert!(app.world().get_entity(parent).is_err());
        assert!(app.world().get_entity(child).is_err());
    }
}
