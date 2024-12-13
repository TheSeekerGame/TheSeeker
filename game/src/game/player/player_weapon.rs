use leafwing_input_manager::prelude::ActionState;

use crate::game::player::{Player, PlayerAction};
use crate::prelude::{
    App, GameTickUpdate, Plugin, Query, ResMut, Resource, With,
};

pub(crate) struct PlayerWeaponPlugin;

impl Plugin for PlayerWeaponPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(PlayerWeapon::default());
        app.add_systems(GameTickUpdate, swap_weapon);
    }
}

#[derive(Resource, Default, PartialEq, Eq)]
pub enum PlayerWeapon {
    Bow,
    #[default]
    Sword,
}

fn swap_weapon(
    mut weapon: ResMut<PlayerWeapon>,
    query: Query<&ActionState<PlayerAction>, With<Player>>,
) {
    for action_state in &query {
        if action_state.just_pressed(&PlayerAction::SwapWeapon) {
            *weapon = match *weapon {
                PlayerWeapon::Bow => PlayerWeapon::Sword,
                PlayerWeapon::Sword => PlayerWeapon::Bow,
            };
        }
    }
}
