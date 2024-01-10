//! Everything related to gameplay and game mechanics.
//!
//! Create sub-modules for different aspects of the gameplay.

use crate::prelude::*;

use self::player::PlayerBlueprintBundle;

pub mod player;
mod wall;

pub struct GameplayPlugin;

impl Plugin for GameplayPlugin {
    fn build(&self, app: &mut App) {
        // All LDTK stuff should be defined here,
        // so it is all in one place and easy to change!
        // Don't scatter it across the sub-modules/plugins!
        app.register_ldtk_int_cell::<wall::WallBundle>(17)
            .register_ldtk_entity::<PlayerBlueprintBundle>("Player")
            .insert_resource(Gravity::default());

        // Add the plugins for each game mechanic
        app.add_plugins((player::PlayerPlugin, wall::WallPlugin));
    }
}
