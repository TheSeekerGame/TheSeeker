// use bevy::prelude::*; // not needed here

use crate::game::player::weapon::{PlayerMeleeWeapon, PlayerRangedWeapon};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SkillId {
    Attack,
    Dash,
    DashStrike,
    Whirl,
    Stealth,
    BurningDash,
    FlickerStrike,
    AmplifiedBell,
    Spinner,
    ExplosiveMine,
    IceNova,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Variant {
    // For Attack
    Forward,
    Up,
    Down,
    // For Dash
    Horizontal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CooldownMode {
    /// Tick down using current CDR each tick (affected by mid-cooldown changes)
    RateBased,
    /// Snapshot duration at start; tick down by 1 per tick
    #[allow(dead_code)]
    SnapshotBased,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CooldownSpec {
    pub min_ticks: u32,
    pub max_ticks: u32,
    pub mode: CooldownMode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ActiveWindowSpec {
    pub duration_ticks: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum SkillWeaponKind {
    Sword = 0,
    Hammer = 1,
    Bow = 2,
}

impl SkillWeaponKind {
    #[allow(dead_code)]
    pub const ALL: [SkillWeaponKind; 3] = [
        SkillWeaponKind::Sword,
        SkillWeaponKind::Hammer,
        SkillWeaponKind::Bow,
    ];

    pub const fn as_index(self) -> usize {
        self as usize
    }
}

impl From<PlayerMeleeWeapon> for SkillWeaponKind {
    fn from(value: PlayerMeleeWeapon) -> Self {
        match value {
            PlayerMeleeWeapon::Sword => SkillWeaponKind::Sword,
            PlayerMeleeWeapon::Hammer => SkillWeaponKind::Hammer,
        }
    }
}

impl From<PlayerRangedWeapon> for SkillWeaponKind {
    fn from(value: PlayerRangedWeapon) -> Self {
        match value {
            PlayerRangedWeapon::Bow => SkillWeaponKind::Bow,
        }
    }
}

impl Variant {
    #[allow(dead_code)]
    pub const ATTACK_VARIANTS: [Variant; 3] = [
        Variant::Forward,
        Variant::Up,
        Variant::Down,
    ];

    pub fn attack_index(self) -> Option<usize> {
        match self {
            Variant::Forward => Some(0),
            Variant::Up => Some(1),
            Variant::Down => Some(2),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AttackVariantMetadata {
    pub active_window: ActiveWindowSpec,
    pub state_duration_ticks: u32,
    pub cooldown: CooldownSpec,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AttackAnimationMetadata {
    pub idle: &'static str,
    pub run: &'static str,
    pub air: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AttackWeaponMetadata {
    pub weapon: SkillWeaponKind,
    pub variants: [AttackVariantMetadata; 3],
    pub animations: AttackAnimationMetadata,
}

pub fn attack_variant_metadata(
    weapon: SkillWeaponKind,
    variant: Variant,
) -> &'static AttackVariantMetadata {
    let weapon_index = weapon.as_index();
    let variant_index = variant.attack_index().unwrap_or(0);
    debug_assert!(
        variant_index
            < super::attack::ATTACK_METADATA[weapon_index]
                .variants
                .len()
    );
    &super::attack::ATTACK_METADATA[weapon_index].variants[variant_index]
}

pub fn attack_animation_metadata(
    weapon: SkillWeaponKind,
) -> &'static AttackAnimationMetadata {
    &super::attack::ATTACK_METADATA[weapon.as_index()].animations
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DashMetadata {
    pub duration_ticks: u32,
    pub cooldown: CooldownSpec,
    pub animation_key: &'static str,
    pub overrides_locomotion: bool,
}

pub(crate) use super::dash::DASH_METADATA;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DashStrikeVariantMetadata {
    pub state_duration_ticks: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DashStrikeMetadata {
    pub cooldown: CooldownSpec,
    pub animation_key: &'static str,
    pub variants: [DashStrikeVariantMetadata; 3],
}

pub fn dash_strike_metadata() -> &'static DashStrikeMetadata {
    &super::dash_strike::DASH_STRIKE_METADATA
}

pub fn dash_strike_variant_metadata(
    variant: Variant,
) -> &'static DashStrikeVariantMetadata {
    let index = match variant {
        Variant::Forward => 0,
        Variant::Up => 1,
        Variant::Down => 2,
        Variant::Horizontal => 0,
    };
    &super::dash_strike::DASH_STRIKE_METADATA.variants[index]
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WhirlMetadata {
    pub max_energy: f32,
    pub min_energy_to_start: f32,
    pub drain_per_second: f32,
    pub regen_per_second: f32,
    pub animations: [&'static str; 3],
}

pub fn whirl_metadata() -> &'static WhirlMetadata {
    &super::whirl::WHIRL_METADATA
}

pub fn whirl_animation_key(kind: SkillWeaponKind) -> &'static str {
    &whirl_metadata().animations[kind.as_index()]
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BurningDashMetadata {
    pub min_health_to_start: u32,
    pub delayed_cooldown: CooldownSpec,
    pub animation_key: &'static str,
}

pub fn burning_dash_metadata() -> &'static BurningDashMetadata {
    &super::burning_dash::BURNING_DASH_METADATA
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SpinnerMetadata {
    pub cooldown: CooldownSpec,
}

pub fn spinner_metadata() -> &'static SpinnerMetadata {
    &super::spinner::SPINNER_METADATA
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AmplifiedBellMetadata {
    pub cooldown: CooldownSpec,
}

pub fn amplified_bell_metadata() -> &'static AmplifiedBellMetadata {
    &super::amplified_bell::AMPLIFIED_BELL_METADATA
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IceNovaMetadata {
    pub cooldown: CooldownSpec,
}

pub fn ice_nova_metadata() -> &'static IceNovaMetadata {
    &super::ice_nova::ICE_NOVA_METADATA
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StealthMetadata {
    pub cooldown: CooldownSpec,
}

pub fn stealth_metadata() -> &'static StealthMetadata {
    &super::stealth::STEALTH_METADATA
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ExplosiveMineMetadata {
    pub max_energy: f32,
    pub chunk_cost: f32,
    pub regen_per_tick: f32,
}

pub fn explosive_mine_metadata() -> &'static ExplosiveMineMetadata {
    &super::explosive_mine::EXPLOSIVE_MINE_METADATA
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FlickerStrikeMetadata {
    pub max_energy: f32,
    pub chunk_cost: f32,
    pub regen_per_second: f32,
    pub range: f32,
}

pub fn flicker_strike_metadata() -> &'static FlickerStrikeMetadata {
    &super::flicker_strike::FLICKER_STRIKE_METADATA
}

impl SkillId {
    /// Skills that can be triggered instantly without interrupting core state machines.
    pub fn is_instant(self) -> bool {
        matches!(
            self,
            SkillId::Stealth
                | SkillId::Spinner
                | SkillId::IceNova
                | SkillId::ExplosiveMine
                | SkillId::AmplifiedBell
        )
    }
}

// Removed unused SkillsStatsContext resource
