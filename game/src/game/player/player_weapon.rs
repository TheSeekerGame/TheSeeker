use bevy::app::Update;
use leafwing_input_manager::prelude::ActionState;
use strum_macros::Display;

use crate::game::player::{Player, PlayerAction};
use crate::prelude::{App, Plugin, Query, ResMut, Resource, With};

pub(crate) struct PlayerWeaponPlugin;

impl Plugin for PlayerWeaponPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(PlayerWeapon::default());
        app.add_systems(Update, swap_weapon);
    }
}

#[derive(Resource, Default, PartialEq, Eq, Display)]
pub enum PlayerWeapon {
    Bow,
    #[default]
    Sword,
}

impl PlayerWeapon {
    pub fn get_anim_key(&self, action: &str) -> String {
        let weapon_str = self.to_string();
        format!("anim.player.{weapon_str}{action}")
    }
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
