use bevy::app::Update;
use bevy::ecs::system::SystemParam;
use bevy::prelude::{resource_equals, IntoSystemConfigs, Res};
use leafwing_input_manager::prelude::ActionState;
use strum_macros::Display;

use crate::game::player::{Player, PlayerAction};
use crate::prelude::{App, Plugin, Query, ResMut, Resource, With};

pub(crate) struct PlayerWeaponPlugin;

impl Plugin for PlayerWeaponPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(PlayerMeleeWeapon::default());
        app.insert_resource(PlayerRangedWeapon::default());
        app.insert_resource(PlayerCombatStyle::default());
        app.add_systems(
            Update,
            (
                swap_combat_style,
                swap_melee_weapon.run_if(resource_equals(
                    PlayerCombatStyle::Melee,
                )),
            ),
        );
    }
}

#[derive(Resource, Default, PartialEq, Eq, Display)]
pub enum PlayerCombatStyle {
    Ranged,
    #[default]
    Melee,
}

#[derive(Resource, Default, PartialEq, Eq, Display)]
pub enum PlayerMeleeWeapon {
    Hammer,
    #[default]
    Sword,
}

#[derive(Resource, Default, PartialEq, Eq, Display)]
pub enum PlayerRangedWeapon {
    #[default]
    Bow,
}

#[derive(SystemParam)]
pub struct CurrentWeapon<'w> {
    combat_style: Res<'w, PlayerCombatStyle>,
    melee_weapon: Res<'w, PlayerMeleeWeapon>,
    ranged_weapon: Res<'w, PlayerRangedWeapon>,
}

impl CurrentWeapon<'_> {
    pub fn get_anim_key(&self, action: &str) -> String {
        let weapon_str = self.to_string();
        format!("anim.player.{weapon_str}{action}")
    }

    /// Retrieves the Whirling skill animation key for the currently equipped melee weapon.
    pub fn whirling_anim_key(&self) -> String {
        let weapon_str = self.melee_weapon.to_string();
        format!("anim.player.{weapon_str}Whirling")
    }
}

impl std::fmt::Display for CurrentWeapon<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let weapon = match *self.combat_style {
            PlayerCombatStyle::Ranged => self.ranged_weapon.to_string(),
            PlayerCombatStyle::Melee => self.melee_weapon.to_string(),
        };
        write!(f, "{weapon}")
    }
}

fn swap_combat_style(
    mut combat_style: ResMut<PlayerCombatStyle>,
    query: Query<&ActionState<PlayerAction>, With<Player>>,
) {
    for action_state in &query {
        if action_state.just_pressed(&PlayerAction::SwapCombatStyle) {
            *combat_style = match *combat_style {
                PlayerCombatStyle::Ranged => PlayerCombatStyle::Melee,
                PlayerCombatStyle::Melee => PlayerCombatStyle::Ranged,
            };
        }
    }
}

fn swap_melee_weapon(
    mut weapon: ResMut<PlayerMeleeWeapon>,
    query: Query<&ActionState<PlayerAction>, With<Player>>,
) {
    for action_state in &query {
        if action_state.just_pressed(&PlayerAction::SwapMeleeWeapon) {
            *weapon = match *weapon {
                PlayerMeleeWeapon::Sword => PlayerMeleeWeapon::Hammer,
                PlayerMeleeWeapon::Hammer => PlayerMeleeWeapon::Sword,
            };
        }
    }
}
