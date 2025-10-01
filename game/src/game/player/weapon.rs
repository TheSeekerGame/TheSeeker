//! Weapon resources and swap systems.
//!
//! Tracks current combat style and weapon selections, provides base damage and
//! pushback tuning, and exposes helpers to derive animation keys for the
//! equipped weapon.
use bevy::ecs::schedule::IntoScheduleConfigs;
use bevy::ecs::system::SystemParam;
use bevy::prelude::{in_state, Res};
use leafwing_input_manager::prelude::ActionState;
use strum_macros::Display;
use theseeker_engine::prelude::{Commands, DetectChanges, OnEnter};
use theseeker_engine::time::GameTickUpdate;

use crate::game::player::{Player, PlayerAction};
use crate::prelude::{App, AppState, Plugin, Query, ResMut, Resource, With};

use super::{Passive, Passives};

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
            (swap_combat_style, swap_melee_weapon)
                .chain()
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

#[derive(Resource, Default, PartialEq, Eq, Display, Copy, Clone)]
pub enum PlayerCombatStyle {
    Ranged,
    #[default]
    Melee,
}

#[derive(Resource, Default, PartialEq, Eq, Display, Copy, Clone)]
pub enum PlayerMeleeWeapon {
    Hammer,
    #[default]
    Sword,
}

impl PlayerMeleeWeapon {
    // Weapon pushback tuning (per-tick displacement magnitudes)
    const SWORD_SELF_PUSHBACK: f32 = 90.0 / 96.0;
    const SWORD_SELF_PUSHBACK_TICKS: u32 = 3;
    const SWORD_PUSHBACK: f32 = 90.0 / 96.0; // 2x stronger than ranged
    const SWORD_PUSHBACK_TICKS: u32 = 12; // 2x longer than ranged

    const HAMMER_SELF_PUSHBACK: f32 = 90.0 / 96.0;
    const HAMMER_SELF_PUSHBACK_TICKS: u32 = 3;
    const HAMMER_PUSHBACK: f32 = 90.0 / 96.0; // 2x stronger than ranged
    const HAMMER_PUSHBACK_TICKS: u32 = 12; // 2x longer than ranged

    /// Damage-collider lifetime based on melee weapon and specific passives.
    /// Sword: 16 → 12 with SerpentRing|FrenziedAttack; Hammer: 24 → 18 with SerpentRing|FrenziedAttack.
    pub fn damage_collider_lifetime(&self, passives: &Passives) -> u32 {
        let has_serpent_ring_or_frenzied_attack = passives
            .contains(&Passive::SerpentRing)
            || passives.contains(&Passive::FrenziedAttack);

        match self {
            Self::Hammer => {
                if has_serpent_ring_or_frenzied_attack {
                    18
                } else {
                    24
                }
            },
            Self::Sword => {
                if has_serpent_ring_or_frenzied_attack {
                    12
                } else {
                    16
                }
            },
        }
    }

    pub fn base_damage(&self) -> f32 {
        match self {
            Self::Hammer => 55.0,
            Self::Sword => 33.0,
        }
    }

    pub fn pushback_values(&self) -> PushbackValues {
        let (pushback, pushback_ticks, self_pushback, self_pushback_ticks) =
            match self {
                Self::Hammer => (
                    Self::HAMMER_PUSHBACK,
                    Self::HAMMER_PUSHBACK_TICKS,
                    Self::HAMMER_SELF_PUSHBACK,
                    Self::HAMMER_SELF_PUSHBACK_TICKS,
                ),
                Self::Sword => (
                    Self::SWORD_PUSHBACK,
                    Self::SWORD_PUSHBACK_TICKS,
                    Self::SWORD_SELF_PUSHBACK,
                    Self::SWORD_SELF_PUSHBACK_TICKS,
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

#[derive(Resource, Default, PartialEq, Eq, Display, Copy, Clone)]
pub enum PlayerRangedWeapon {
    #[default]
    Bow,
}

impl PlayerRangedWeapon {
    const BOW_PUSHBACK: f32 = 60.0 / 96.0;
    const BOW_PUSHBACK_TICKS: u32 = 12;

    pub fn base_damage(&self) -> f32 {
        match self {
            Self::Bow => 22.0,
        }
    }

    pub fn pushback_values(&self) -> PushbackValues {
        match self {
            Self::Bow => PushbackValues {
                pushback: Self::BOW_PUSHBACK,
                pushback_ticks: Self::BOW_PUSHBACK_TICKS,
                self_pushback: 0.0,
                self_pushback_ticks: 0,
            },
        }
    }
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

    pub fn combat_style(&self) -> PlayerCombatStyle {
        *self.combat_style
    }

    pub fn melee_weapon(&self) -> PlayerMeleeWeapon {
        *self.melee_weapon
    }

    pub fn ranged_weapon(&self) -> PlayerRangedWeapon {
        *self.ranged_weapon
    }

    pub fn get_anim_key(&self, action: &str) -> String {
        let weapon_str = self.to_string();
        format!("anim.player.{weapon_str}{action}")
    }

    /// Whirling animation key for the current melee weapon.
    pub fn whirling_anim_key(&self) -> String {
        let weapon_str = self.melee_weapon.to_string();
        format!("anim.player.{weapon_str}Whirling")
    }

    pub fn is_wielding_hammer(&self) -> bool {
        self.combat_style() == PlayerCombatStyle::Melee
            && self.melee_weapon() == PlayerMeleeWeapon::Hammer
    }

    pub fn has_bow_equipped(&self) -> bool {
        self.combat_style() == PlayerCombatStyle::Ranged
            && self.ranged_weapon() == PlayerRangedWeapon::Bow
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
