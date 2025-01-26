//! Everything related to gameplay and game mechanics.
//!
//! Create sub-modules for different aspects of the gameplay.

use switches::{PuzzleBundle, SwitchBundle};

use self::enemy::{EnemyBlueprintBundle, EnemySpawnerBundle};
use self::player::PlayerBlueprintBundle;
use crate::game::merchant::MerchantBlueprintBundle;
use crate::game::yak::YakBlueprintBundle;
use crate::prelude::*;

pub mod attack;
pub mod enemy;
mod game_over;
pub mod gentstate;
mod merchant;
pub mod physics;
pub mod player;
mod switches;
mod wall;
mod xp_orbs;
mod yak;
mod pickups;
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

        app.register_ldtk_entity::<SwitchBundle>("Switch1")
            .register_ldtk_entity::<SwitchBundle>("Switch2")
            .register_ldtk_entity::<SwitchBundle>("Switch3")
            .register_ldtk_entity::<SwitchBundle>("Switch4")
            .register_ldtk_entity::<SwitchBundle>("Switch5")
            .register_ldtk_entity::<SwitchBundle>("Switch6")
            .register_ldtk_entity::<SwitchBundle>("Switch7")
            .register_ldtk_entity::<SwitchBundle>("Switch8")
            .register_ldtk_entity::<SwitchBundle>("Switch9")
            .register_ldtk_entity::<SwitchBundle>("Switch10")
            .register_ldtk_entity::<SwitchBundle>("Switch11")
            .register_ldtk_entity::<SwitchBundle>("Switch12");

        app.register_ldtk_entity::<PuzzleBundle>("Puzzle1")
            .register_ldtk_entity::<PuzzleBundle>("Puzzle2")
            .register_ldtk_entity::<PuzzleBundle>("Puzzle3")
            .register_ldtk_entity::<PuzzleBundle>("Puzzle4")
            .register_ldtk_entity::<PuzzleBundle>("Puzzle5")
            .register_ldtk_entity::<PuzzleBundle>("Puzzle6")
            .register_ldtk_entity::<PuzzleBundle>("Puzzle7")
            .register_ldtk_entity::<PuzzleBundle>("Puzzle8")
            .register_ldtk_entity::<PuzzleBundle>("Puzzle9")
            .register_ldtk_entity::<PuzzleBundle>("Puzzle10")
            .register_ldtk_entity::<PuzzleBundle>("Puzzle11")
            .register_ldtk_entity::<PuzzleBundle>("Puzzle12");

        // Add the plugins for each game mechanic
        app.add_plugins((
            player::PlayerPlugin,
            enemy::EnemyPlugin,
            merchant::MerchantPlugin,
            yak::YakPlugin,
            attack::AttackPlugin,
            wall::WallPlugin,
            game_over::GameOverPlugin,
            xp_orbs::XpPlugin,
            switches::SwitchesPlugin,
            pickups::PickupPlugin,
        ));
    }
}
