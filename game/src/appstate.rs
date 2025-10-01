use bevy::ecs::query::QueryFilter;

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

        // Simple state transition from AssetsLoading to InGame
        app.add_systems(
            Update,
            transition_to_ingame.run_if(in_state(AppState::AssetsLoading)),
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

fn transition_to_ingame(
    mut next_state: ResMut<NextState<AppState>>,
    dynamic_assets: Option<Res<DynamicAssets>>,
    asset_server: Res<AssetServer>,
    time: Res<Time>,
    mut min_wait_timer: Local<Option<Timer>>,
) {
    // Initialize minimum wait timer (prevents flashing)
    if min_wait_timer.is_none() {
        *min_wait_timer = Some(Timer::from_seconds(
            0.5,
            TimerMode::Once,
        ));
    }

    // Tick the minimum wait timer
    if let Some(ref mut timer) = min_wait_timer.as_mut() {
        timer.tick(time.delta());

        // Only check assets after minimum wait time
        if timer.finished() {
            if let Some(dynamic_assets) = dynamic_assets {
                // Check if all dynamic assets are loaded
                let total_assets = dynamic_assets.iter_assets().count();
                let loaded_count = dynamic_assets
                    .iter_assets()
                    .filter(|(_, asset)| {
                        asset.load(&asset_server).iter().all(|handle| {
                            matches!(
                                asset_server.get_load_state(handle),
                                Some(bevy::asset::LoadState::Loaded)
                            )
                        })
                    })
                    .count();

                if loaded_count == total_assets {
                    info!("All {} dynamic assets loaded, transitioning to InGame state", total_assets);
                    next_state.set(AppState::InGame);
                }
            } else {
                // No dynamic assets to wait for
                info!("No dynamic assets, transitioning to InGame state");
                next_state.set(AppState::InGame);
            }
        }
    }
}

fn despawn_all_recursive<F: QueryFilter>(
    mut commands: Commands,
    q: Query<Entity, F>,
) {
    for entity in &q {
        commands.entity(entity).despawn();
    }
}
