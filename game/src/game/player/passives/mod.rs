use crate::game::player::skills::types;
use crate::game::{
    combat::{DamageInfo, Health},
    player::PlayerStatMod,
};
/// Trait-based passive system: each passive encapsulates its own stat mods and event reactions.
use bevy::prelude::*;

// Re-export individual passive implementations
pub mod battery_pack;
pub mod bloodstone;
pub mod critical_regeneration;
pub mod deadly_feather;
pub mod flaming_heart;
pub mod frenzied_attack;
pub mod glowing_shard;
pub mod heavy_boots;
pub mod ice_dagger;
pub mod kinetic_orb;
pub mod obsidian_necklace;
pub mod pack_killer;
pub mod permutator;
pub mod prime_sygil;
pub mod protective_spirit;
pub mod pulse_drive;
pub mod rabbits_foot;
pub mod runtime;
pub mod serpent_ring;
pub mod shadow_cloak;
pub mod sharpshooter;
pub mod vitality_overclock;

use battery_pack::BatteryPackEffect;
use bloodstone::BloodstoneEffect;
use critical_regeneration::CriticalRegenerationEffect;
use deadly_feather::DeadlyFeatherEffect;
use flaming_heart::FlamingHeartEffect;
use frenzied_attack::FrenziedAttackEffect;
use glowing_shard::GlowingShardEffect;
use heavy_boots::HeavyBootsEffect;
use ice_dagger::IceDaggerEffect;
use kinetic_orb::KineticOrbEffect;
use obsidian_necklace::ObsidianNecklaceEffect;
use pack_killer::PackKillerEffect;
use permutator::PermutatorEffect;
use prime_sygil::PrimeSygilEffect;
use protective_spirit::ProtectiveSpiritEffect;
use pulse_drive::PulseDriveEffect;
use rabbits_foot::RabbitsFootEffect;
use serpent_ring::SerpentRingEffect;
use shadow_cloak::ShadowCloakEffect;
use sharpshooter::SharpshooterEffect;
use vitality_overclock::VitalityOverclockEffect;

/// Events that passives can respond to.
#[derive(Event, Clone, Copy)]
pub enum PassiveEvent {
    /// Active skill was used by the player (emitted on activation)
    SkillUsed {
        owner: Entity,
        skill: types::SkillId,
    },
    /// Enemy was killed by the player
    EnemyKilled { owner: Entity },
    /// Enemy was killed by a backstab from the player
    BackstabKill { owner: Entity },
    /// XP orb was picked up
    XpOrbPickup,
    /// Player dealt damage
    DamageDealt(DamageInfo),
    /// Player took damage
    DamageTaken(DamageInfo),
    /// Damage entity hit a target (first hit only)
    DamageHit { damage_source: Entity },
    /// Critical hit occurred (source skill is provided when known)
    CriticalHit {
        damage_source: Entity,
        source_skill: Option<types::SkillId>,
    },
    /// Player's deterministic hit counter advanced (after the first successful hit of a damage source)
    HitCountAdvanced { owner: Entity, hit_count: u32 },
}

/// Actions that passives can trigger.
#[derive(Debug, Clone)]
pub enum PassiveAction {
    /// Heal the player by a specified amount
    Heal(u32),
    /// Damage the player by a specified amount
    Damage(u32),
    /// Reduce all cooldowns by the given seconds
    ReduceCooldowns(f32),
    /// Reset all cooldowns
    ResetCooldowns,
    /// Add energy to all channelled, energy-based skills (e.g., Whirl, Flicker)
    AddEnergy(f32),
    /// Fully refill energy for all channeled skills (e.g., Whirl, Flicker)
    RefillEnergyFull,
    /// Deprecated: legacy variant that only added Whirl energy
    /// Retained for backward compatibility; treated as `AddEnergy`.
    AddWhirlEnergy(f32),
    /// Modify a damage entity's properties
    ModifyDamage {
        damage_source: Entity,
        damage_multiplier: f32,
    },
    /// Schedule the next hit to be a critical strike for this player
    ScheduleNextCrit,
    /// Trigger all instant-cast skills currently equipped (ignores cooldowns)
    TriggerInstantSkills {
        /// Damage source that triggered the instant skills (if relevant).
        source_damage: Option<Entity>,
    },
}

/// Context provided to passives for decision making.
pub struct PassiveContext<'a> {
    pub health: &'a Health,
    pub velocity: Vec2,
    pub enemies_nearby: u32,
    pub grounded: bool,
    /// True if player has InAir marker (more truthful than !grounded due to predictive grounding)
    pub in_air: bool,
    pub buff_stacks: u32,
    /// Closest enemy distance if available from sensors
    pub closest_enemy_distance: Option<f32>,
    /// Whether locomotion state is Running (used by Sharpshooter)
    pub is_running: bool,
    /// Optional target position for per-target damage calculations
    pub target_position: Option<Vec2>,
    /// Player's last grounded position for distance calculations
    pub last_grounded_position: Option<Vec2>,
    /// Movement input from player (-1.0 = left, 0.0 = none/both, 1.0 = right)
    pub movement_input: f32,
    /// Whether jump key is currently pressed
    pub jump_pressed: bool,
    /// Number of enemies near the current damage target (for Pack Killer)
    pub enemies_near_target: u32,
    /// Target's current health percentage (0.0..=1.0) if known (per-target phase only)
    pub target_health_pct: Option<f32>,
    /// Whether a non-repeating skill sequence is active (last two used skills differ)
    pub rotation_active: bool,
}

pub struct PassiveContextInputs<'a> {
    pub health: Option<&'a Health>,
    pub velocity: Option<Vec2>,
    pub enemies_nearby: Option<u32>,
    pub grounded: bool,
    pub in_air: bool,
    pub buff_stacks: Option<u32>,
    pub closest_enemy_distance: Option<f32>,
    pub is_running: bool,
    pub target_position: Option<Vec2>,
    pub last_grounded_position: Option<Vec2>,
    pub movement_input: Option<f32>,
    pub jump_pressed: Option<bool>,
    pub enemies_near_target: Option<u32>,
    pub target_health_pct: Option<f32>,
    pub rotation_active: bool,
}

/// Default health value used when constructing PassiveContext without health data
static DEFAULT_HEALTH: Health = Health { current: 0, max: 0 };

impl<'a> PassiveContext<'a> {
    pub fn from_inputs(inputs: PassiveContextInputs<'a>) -> Self {
        Self {
            health: inputs.health.unwrap_or(&DEFAULT_HEALTH),
            velocity: inputs.velocity.unwrap_or(Vec2::ZERO),
            enemies_nearby: inputs.enemies_nearby.unwrap_or(0),
            grounded: inputs.grounded,
            in_air: inputs.in_air,
            buff_stacks: inputs.buff_stacks.unwrap_or(0),
            closest_enemy_distance: inputs.closest_enemy_distance,
            is_running: inputs.is_running,
            target_position: inputs.target_position,
            last_grounded_position: inputs.last_grounded_position,
            movement_input: inputs.movement_input.unwrap_or(0.0),
            jump_pressed: inputs.jump_pressed.unwrap_or(false),
            enemies_near_target: inputs.enemies_near_target.unwrap_or(0),
            target_health_pct: inputs.target_health_pct,
            rotation_active: inputs.rotation_active,
        }
    }
}

/// Modifiers that passives can apply to damage sources.
#[derive(Debug, Clone)]
pub struct DamageModifiers {
    pub damage_multiplier: f32,
    pub can_backstab: bool,
    /// Number of targets the current damage source would hit this tick
    pub current_target_count: usize,
}

impl Default for DamageModifiers {
    fn default() -> Self {
        Self {
            damage_multiplier: 1.0,
            can_backstab: false,
            current_target_count: 0,
        }
    }
}

/// Core trait implemented by all passive effects.
pub trait PassiveEffect: Send + Sync + 'static {
    /// Called once per tick to modify player stats
    fn modify_stats(&self, stats: &mut PlayerStatMod, context: &PassiveContext);

    /// Called when calculating damage properties
    fn modify_damage(
        &self,
        _modifiers: &mut DamageModifiers,
        _context: &PassiveContext,
    ) {
    }

    /// Called when passive events occur
    fn on_event(
        &self,
        _event: &PassiveEvent,
        _context: &PassiveContext,
    ) -> Vec<PassiveAction> {
        vec![]
    }

    /// Animation slots this passive affects (name, value)
    fn animation_slots(&self) -> Vec<(&'static str, bool)> {
        vec![]
    }

    /// Priority for order-dependent processing (higher runs first)
    fn priority(&self) -> i32 {
        0
    }

    /// Human-readable name (debugging)
    fn name(&self) -> &'static str;
}

/// Registry to get passive implementations from enum values
pub fn get_passive_implementation(
    passive: &super::Passive,
) -> Box<dyn PassiveEffect> {
    use super::Passive;

    match passive {
        Passive::Bloodstone => Box::new(BloodstoneEffect),
        Passive::ObsidianNecklace => Box::new(ObsidianNecklaceEffect),
        Passive::CriticalRegeneration => Box::new(CriticalRegenerationEffect),
        Passive::FlamingHeart => Box::new(FlamingHeartEffect),
        Passive::IceDagger => Box::new(IceDaggerEffect),
        Passive::PackKiller => Box::new(PackKillerEffect),
        Passive::GlowingShard => Box::new(GlowingShardEffect),
        Passive::SerpentRing => Box::new(SerpentRingEffect),
        Passive::RabbitsFoot => Box::new(RabbitsFootEffect),
        Passive::HeavyBoots => Box::new(HeavyBootsEffect),
        Passive::DeadlyFeather => Box::new(DeadlyFeatherEffect),
        Passive::FrenziedAttack => Box::new(FrenziedAttackEffect),
        Passive::VitalityOverclock => Box::new(VitalityOverclockEffect),
        Passive::ProtectiveSpirit => Box::new(ProtectiveSpiritEffect),
        Passive::Sharpshooter => Box::new(SharpshooterEffect),
        Passive::KineticOrb => Box::new(KineticOrbEffect),
        Passive::BatteryPack => Box::new(BatteryPackEffect),
        Passive::ShadowCloak => Box::new(ShadowCloakEffect),
        Passive::PrimeSygil => Box::new(PrimeSygilEffect),
        Passive::PulseDrive => Box::new(PulseDriveEffect),
        Passive::Permutator => Box::new(PermutatorEffect),
    }
}
