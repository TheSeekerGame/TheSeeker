//! Level Management
//!
//! This is where we manage everything related to managing the map /
//! the in-game playable space/area.
//! A "level" is one "room" in the game, connected to others.
//!
//! The player should be able to walk between them,
//! and we dynamically switch (load/unload) levels as needed.
//!
//! This module is the framework for level transitions, loading/unloading,
//! general map setup, and other such managerial stuff.
//!
//! Any of the stuff that actually *happens* within the map when you
//! play the game, doesn't belong here. Put that stuff under [`crate::game`].

use crate::prelude::*;

pub struct LevelManagerPlugin;

impl Plugin for LevelManagerPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(LevelSelection::Identifier(
            "Level_0".into(),
        ));
        app.add_systems(
            OnEnter(AppState::InGame),
            game_level_init,
        );
    }
}

/// System to perform initial setup when entering the gameplay state, load the starting level.
fn game_level_init(mut commands: Commands, preloaded: Res<PreloadedAssets>) {
    // TODO: per-level asset management instead of preloaded assets
    // TODO: when we have save files, use that to choose the level to init at
    
    #[cfg(not(feature = "dev"))]
    commands.spawn((
        StateDespawnMarker,
        LdtkWorldBundle {
            ldtk_handle: preloaded
                .get_single_asset("level.01")
                .expect("Expected asset key 'level.01'"),
            ..Default::default()
        },
    ));
    #[cfg(feature = "dev")]
    commands.spawn((
        StateDespawnMarker,
        LdtkWorldBundle {
            ldtk_handle: preloaded
                .get_single_asset("level.dev")
                .expect("Expected asset key 'level.dev'"),
            ..Default::default()
        },
    ));
}
