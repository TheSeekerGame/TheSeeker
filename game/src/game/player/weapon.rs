//! The player's weapons: what is equipped, and how each weapon attacks.
use bevy::math::Vec2;
use bevy::prelude::Component;
use std::collections::HashMap;
use strum_macros::Display;
use theseeker_engine::prelude::{Commands, OnEnter};

use crate::game::physics::Knockback;
use crate::game::player::states::AttackVariant;
use crate::prelude::{App, AppState, Plugin, Resource};

pub(crate) struct PlayerWeaponPlugin;

impl Plugin for PlayerWeaponPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PlayerMeleeWeapon>();
        app.add_systems(
            OnEnter(AppState::InGame),
            initialize_resources,
        );
    }
}

#[derive(Resource, Default, PartialEq, Eq, Display, Copy, Clone, Debug, Hash)]
pub enum PlayerMeleeWeapon {
    Hammer,
    #[default]
    Sword,
}

impl PlayerMeleeWeapon {
    pub const ALL: [Self; 2] = [Self::Sword, Self::Hammer];

    pub fn name(self) -> &'static str {
        match self {
            Self::Sword => "Sword",
            Self::Hammer => "Hammer",
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            Self::Sword => "Quick and balanced.",
            Self::Hammer => "Slow, crushing blows.",
        }
    }

    pub fn icon_path(self) -> &'static str {
        match self {
            Self::Sword => "items/weapons/SwordIcon.png",
            Self::Hammer => "items/weapons/HammerIcon.png",
        }
    }

    /// How this weapon attacks.
    pub fn attack(self) -> &'static WeaponAttack {
        WeaponType::from(self).attack()
    }
}

#[derive(PartialEq, Eq, Display, Copy, Clone, Debug, Hash)]
pub enum PlayerRangedWeapon {
    Bow,
}

impl PlayerRangedWeapon {
    pub fn name(self) -> &'static str {
        match self {
            Self::Bow => "Bow",
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            Self::Bow => "Fires the arrows in the ammo slot.",
        }
    }

    pub fn icon_path(self) -> &'static str {
        match self {
            Self::Bow => "items/weapons/BowIcon.png",
        }
    }
}

#[derive(PartialEq, Eq, Display, Copy, Clone, Debug, Hash)]
pub enum PlayerAmmo {
    Arrows,
}

impl PlayerAmmo {
    pub const ALL: [Self; 1] = [Self::Arrows];

    pub fn name(self) -> &'static str {
        match self {
            Self::Arrows => "Arrows",
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            Self::Arrows => "Ammunition for the bow.",
        }
    }

    pub fn icon_path(self) -> &'static str {
        match self {
            Self::Arrows => "items/weapons/ArrowsIcon.png",
        }
    }
}

#[derive(Component, Debug, Clone)]
#[derive(Default)]
pub struct WeaponInventory {
    owned_ranged: Vec<PlayerRangedWeapon>,
    equipped_ranged: Option<PlayerRangedWeapon>,
    equipped_ammo: Option<PlayerAmmo>,
    ammo_counts: HashMap<PlayerAmmo, u32>,
}


impl WeaponInventory {
    pub fn owned_ranged(&self) -> &[PlayerRangedWeapon] {
        &self.owned_ranged
    }

    pub fn equipped_ranged(&self) -> Option<PlayerRangedWeapon> {
        self.equipped_ranged
    }

    pub fn equipped_ammo(&self) -> Option<PlayerAmmo> {
        self.equipped_ammo
    }

    pub fn has_ranged(&self, weapon: PlayerRangedWeapon) -> bool {
        self.owned_ranged.contains(&weapon)
    }

    pub fn unlock_ranged(&mut self, weapon: PlayerRangedWeapon) -> bool {
        if self.has_ranged(weapon) {
            return false;
        }

        self.owned_ranged.push(weapon);
        true
    }

    pub fn equip_ranged(&mut self, weapon: PlayerRangedWeapon) -> bool {
        if !self.has_ranged(weapon) {
            return false;
        }

        self.equipped_ranged = Some(weapon);
        true
    }

    pub fn add_ammo(&mut self, ammo: PlayerAmmo, amount: u32) {
        if amount == 0 {
            return;
        }

        let entry = self.ammo_counts.entry(ammo).or_insert(0);
        *entry = entry.saturating_add(amount);
    }

    pub fn count_ammo(&self, ammo: PlayerAmmo) -> u32 {
        self.ammo_counts.get(&ammo).copied().unwrap_or(0)
    }

    pub fn has_ammo(&self, ammo: PlayerAmmo, amount: u32) -> bool {
        self.count_ammo(ammo) >= amount
    }

    pub fn consume_ammo(&mut self, ammo: PlayerAmmo, amount: u32) -> bool {
        if amount == 0 {
            return true;
        }

        let Some(current) = self.ammo_counts.get_mut(&ammo) else {
            return false;
        };
        if *current < amount {
            return false;
        }

        *current -= amount;
        if *current == 0 {
            self.ammo_counts.remove(&ammo);
        }
        true
    }

    pub fn equip_ammo(&mut self, ammo: PlayerAmmo) {
        self.equipped_ammo = Some(ammo);
    }

    pub fn grant_bow_bundle(&mut self, arrow_count: u32) {
        self.unlock_ranged(PlayerRangedWeapon::Bow);
        self.equip_ranged(PlayerRangedWeapon::Bow);
        self.add_ammo(PlayerAmmo::Arrows, arrow_count);
        self.equip_ammo(PlayerAmmo::Arrows);
    }

    pub fn grant_arrows(&mut self, amount: u32) {
        self.add_ammo(PlayerAmmo::Arrows, amount);
        self.equip_ammo(PlayerAmmo::Arrows);
    }

    pub fn can_fire_bow(&self) -> bool {
        self.equipped_ranged == Some(PlayerRangedWeapon::Bow)
            && self.equipped_ammo == Some(PlayerAmmo::Arrows)
            && self.has_ammo(PlayerAmmo::Arrows, 1)
    }

    pub fn consume_bow_ammo(&mut self) -> bool {
        if !self.can_fire_bow() {
            return false;
        }

        self.consume_ammo(PlayerAmmo::Arrows, 1)
    }
}

/// A weapon the player attacks with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WeaponType {
    Sword,
    Hammer,
    Bow,
}

impl WeaponType {
    pub fn is_melee(self) -> bool {
        self != Self::Bow
    }

    pub fn attack(self) -> &'static WeaponAttack {
        match self {
            Self::Sword => &SWORD,
            Self::Hammer => &HAMMER,
            Self::Bow => &BOW,
        }
    }
}

impl From<PlayerMeleeWeapon> for WeaponType {
    fn from(weapon: PlayerMeleeWeapon) -> Self {
        match weapon {
            PlayerMeleeWeapon::Sword => Self::Sword,
            PlayerMeleeWeapon::Hammer => Self::Hammer,
        }
    }
}

/// How a weapon attacks: damage, knockback, impact sound, animations, and the
/// timing of each swing direction.
#[derive(Debug)]
pub struct WeaponAttack {
    pub damage: f32,
    /// Knockback dealt to targets.
    pub pushback: Push,
    /// Knockback the attacker takes on a melee swing.
    pub self_pushback: Push,
    /// Spark-audio slot played on impact.
    pub hit_slot: &'static str,
    /// Animations for attacking while standing, running, and airborne.
    pub idle_animation: &'static str,
    pub run_animation: &'static str,
    pub air_animation: &'static str,
    /// Forward, up, and down swings, in `AttackVariant` order.
    swings: [Swing; 3],
}

impl WeaponAttack {
    pub fn swing(&self, variant: AttackVariant) -> Swing {
        self.swings[variant as usize]
    }
}

/// One swing direction's timing, in ticks.
#[derive(Debug, Clone, Copy)]
pub struct Swing {
    /// How long a melee hitbox stays active.
    pub hitbox_ticks: u32,
    /// How long the player stays in the attack state.
    pub state_ticks: u32,
    /// Cooldown before the next attack, shortened by cooldown reduction.
    pub cooldown_ticks: u32,
}

/// Knockback as a per-tick displacement held for a number of ticks.
#[derive(Debug, Clone, Copy)]
pub struct Push {
    pub strength: f32,
    pub ticks: u32,
}

impl Push {
    pub fn toward(self, direction: Vec2) -> Knockback {
        Knockback::new(direction * self.strength, self.ticks)
    }

    /// Knockback along x, to the right for `sign` 1 and to the left for -1.
    pub fn horizontal(self, sign: f32) -> Knockback {
        Knockback::new(Vec2::new(sign * self.strength, 0.0), self.ticks)
    }
}

/// Melee knockback: twice the bow's strength, and a short recoil for the attacker.
const MELEE_PUSHBACK: Push = Push { strength: 90.0 / 96.0, ticks: 12 };
const MELEE_SELF_PUSHBACK: Push = Push { strength: 90.0 / 96.0, ticks: 3 };

const SWORD: WeaponAttack = WeaponAttack {
    damage: 33.0,
    pushback: MELEE_PUSHBACK,
    self_pushback: MELEE_SELF_PUSHBACK,
    hit_slot: "SwordHit",
    idle_animation: "anim.player.SwordBasicIdle",
    run_animation: "anim.player.SwordBasicRun",
    air_animation: "anim.player.SwordBasicAir",
    swings: [
        Swing { hitbox_ticks: 14, state_ticks: 32, cooldown_ticks: 32 },
        Swing { hitbox_ticks: 16, state_ticks: 36, cooldown_ticks: 36 },
        Swing { hitbox_ticks: 18, state_ticks: 40, cooldown_ticks: 40 },
    ],
};

const HAMMER: WeaponAttack = WeaponAttack {
    damage: 55.0,
    pushback: MELEE_PUSHBACK,
    self_pushback: MELEE_SELF_PUSHBACK,
    hit_slot: "HammerHit",
    idle_animation: "anim.player.HammerBasicIdle",
    run_animation: "anim.player.HammerBasicRun",
    air_animation: "anim.player.HammerBasicAir",
    swings: [
        Swing { hitbox_ticks: 22, state_ticks: 56, cooldown_ticks: 48 },
        Swing { hitbox_ticks: 26, state_ticks: 56, cooldown_ticks: 54 },
        Swing { hitbox_ticks: 30, state_ticks: 56, cooldown_ticks: 60 },
    ],
};

const BOW: WeaponAttack = WeaponAttack {
    damage: 22.0,
    pushback: Push { strength: 60.0 / 96.0, ticks: 12 },
    self_pushback: Push { strength: 0.0, ticks: 0 },
    hit_slot: "BowHit",
    idle_animation: "anim.player.BowBasicIdle",
    run_animation: "anim.player.BowBasicRun",
    air_animation: "anim.player.BowBasicAir",
    swings: [
        Swing { hitbox_ticks: 10, state_ticks: 40, cooldown_ticks: 32 },
        Swing { hitbox_ticks: 12, state_ticks: 40, cooldown_ticks: 36 },
        Swing { hitbox_ticks: 14, state_ticks: 40, cooldown_ticks: 40 },
    ],
};

fn initialize_resources(mut commands: Commands) {
    commands.insert_resource(PlayerMeleeWeapon::default());
}
