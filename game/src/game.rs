//! Everything related to gameplay and game mechanics.
//!
//! Create sub-modules for different aspects of the gameplay.

use crate::prelude::*;

pub mod player;
mod wall;

pub struct GameplayPlugin;

impl Plugin for GameplayPlugin {
    fn build(&self, app: &mut App) {
        // All LDTK stuff should be defined here,
        // so it is all in one place and easy to change!
        // Don't scatter it across the sub-modules/plugins!
        app.register_ldtk_int_cell::<wall::WallBundle>(1);

        // Add the plugins for each game mechanic
        app.add_plugins((
            player::PlayerPlugin,
            wall::WallPlugin,
        ));
    }
}
