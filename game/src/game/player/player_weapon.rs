use bevy::ecs::system::SystemParam;
use bevy::prelude::{in_state, IntoSystemConfigs, Res};
use leafwing_input_manager::prelude::ActionState;
use strum_macros::Display;
use theseeker_engine::prelude::{Commands, DetectChanges, OnEnter};
use theseeker_engine::time::GameTickUpdate;

use crate::game::player::{Player, PlayerAction};
use crate::prelude::{App, AppState, Plugin, Query, ResMut, Resource, With};

use super::{PlayerConfig, PlayerStateSet};

pub(crate) struct PlayerWeaponPlugin;

impl Plugin for PlayerWeaponPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PlayerMeleeWeapon>();
        app.init_resource::<PlayerRangedWeapon>();
        app.init_resource::<PlayerCombatStyle>();
        app.add_systems(
            OnEnter(AppState::InGame),
            initialize_resources,
        );
        app.add_systems(
            GameTickUpdate,
            ((swap_combat_style, swap_melee_weapon)
                .chain()
                .before(PlayerStateSet::Behavior))
            .run_if(in_state(AppState::InGame)),
        );
    }
}

pub struct PushbackValues {
    pub pushback: f32,
    pub pushback_ticks: u32,
    pub self_pushback: f32,
    pub self_pushback_ticks: u32,
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

impl PlayerMeleeWeapon {
    pub fn pushback_values(&self, config: &PlayerConfig) -> PushbackValues {
        let (pushback, pushback_ticks, self_pushback, self_pushback_ticks) =
            match self {
                Self::Hammer => (
                    config.hammer_pushback,
                    config.hammer_pushback_ticks,
                    config.hammer_self_pushback,
                    config.hammer_self_pushback_ticks,
                ),
                Self::Sword => (
                    config.sword_pushback,
                    config.sword_pushback_ticks,
                    config.sword_self_pushback,
                    config.sword_self_pushback_ticks,
                ),
            };

        PushbackValues {
            pushback,
            pushback_ticks,
            self_pushback,
            self_pushback_ticks,
        }
    }
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
    pub fn is_changed(&self) -> bool {
        self.combat_style.is_changed()
            || self.melee_weapon.is_changed()
            || self.ranged_weapon.is_changed()
    }

    pub fn get_anim_key(&self, action: &str) -> String {
        let weapon_str = self.to_string();
        format!("anim.player.{weapon_str}{action}")
    }

    /// Retrieves the Whirling skill animation key for the currently equipped melee weapon.
    pub fn whirling_anim_key(&self) -> String {
        let weapon_str = self.melee_weapon.to_string();
        format!("anim.player.{weapon_str}Whirling")
    }

    pub fn is_wielding_hammer(&self) -> bool {
        *self.combat_style == PlayerCombatStyle::Melee
            && *self.melee_weapon == PlayerMeleeWeapon::Hammer
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

fn initialize_resources(mut commands: Commands) {
    commands.insert_resource(PlayerMeleeWeapon::default());
    commands.insert_resource(PlayerRangedWeapon::default());
    commands.insert_resource(PlayerCombatStyle::default());
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

fn is_current_weapon_changed(current_weapon: CurrentWeapon) -> bool {
    current_weapon.is_changed()
}
