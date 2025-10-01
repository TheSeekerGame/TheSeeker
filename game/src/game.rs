//! Everything related to gameplay and game mechanics.
//!
//! Create sub-modules for different aspects of the gameplay.

use self::enemy::{EnemyBlueprintBundle, EnemySpawnerBundle};
use self::player::PlayerBlueprintBundle;
use crate::game::merchant::MerchantBlueprintBundle;
use crate::game::yak::YakBlueprintBundle;
use crate::prelude::*;

pub mod combat;
pub mod effects;
pub mod enemy;
mod game_over;
pub mod gentstate;
mod merchant;
pub mod physics;
pub mod player;
mod wall;
mod yak;

pub struct GameplayPlugin;

impl Plugin for GameplayPlugin {
    fn build(&self, app: &mut App) {
        // All LDTK stuff should be defined here,
        // so it is all in one place and easy to change!
        // Don't scatter it across the sub-modules/plugins!
        app.register_ldtk_int_cell::<wall::WallBundle>(17);
        app.register_ldtk_int_cell::<wall::WallBundle>(18);
        app.register_ldtk_int_cell::<wall::WallBundle>(19);
        app.register_ldtk_int_cell::<wall::WallBundle>(20);
        app.register_ldtk_entity::<PlayerBlueprintBundle>("Player");
        app.register_ldtk_entity::<MerchantBlueprintBundle>("Merchant");
        app.register_ldtk_entity::<YakBlueprintBundle>("Yak");
        app.register_ldtk_entity::<EnemyBlueprintBundle>("Enemy");
        app.register_ldtk_entity::<EnemySpawnerBundle>("EnemySpawner");

        // Add the plugins for each game mechanic
        app.add_plugins((
            combat::CombatPlugin,
            physics::PhysicsPlugin,
            effects::EffectsPlugin,
            player::PlayerPlugin,
            enemy::EnemyPlugin,
            merchant::MerchantPlugin,
            yak::YakPlugin,
            wall::WallPlugin,
            game_over::GameOverPlugin,
        ));
    }
}
