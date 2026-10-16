use crate::game::player::skills::types;
use crate::game::player::states::{HorizontalInputIntent, InAir};
use crate::game::player::{EnemiesNearby, EnemyProximitySensor, LastGroundedPosition, PlayerAction};
use crate::game::{
    combat::{DamageInfo, Health},
};
use bevy::ecs::query::QueryData;
use bevy::prelude::*;
use leafwing_input_manager::prelude::ActionState;
use theseeker_engine::physics::LinearVelocity;

pub mod definition;
pub mod blood_shards;
pub mod frenzied_attack;
pub mod permutator;
pub mod pulse_echo;
pub mod runtime;
mod evaluation;
pub use evaluation::{ordered_passives, PassiveEvaluation};

/// Messages that passives can respond to.
#[derive(Message, Clone, Copy)]
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
    /// Reduce all cooldowns by the given seconds
    ReduceCooldowns(f32),
    /// Reset all cooldowns
    ResetCooldowns,
    /// Add energy to all channelled, energy-based skills (e.g., Whirl, Flicker)
    AddEnergy(f32),
    /// Fully refill energy for all channeled skills (e.g., Whirl, Flicker)
    RefillEnergyFull,
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
    /// True if player has InAir marker (more truthful than !grounded due to predictive grounding)
    pub in_air: bool,
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

/// Health used when the owner has none.
static NO_HEALTH: Health = Health { current: 0, max: 0 };

impl<'a> PassiveContext<'a> {
    /// A context that knows only the owner's health.
    pub fn with_health(health: &'a Health) -> Self {
        Self {
            health,
            velocity: Vec2::ZERO,
            enemies_nearby: 0,
            in_air: false,
            target_position: None,
            last_grounded_position: None,
            movement_input: 0.0,
            jump_pressed: false,
            enemies_near_target: 0,
            target_health_pct: None,
            rotation_active: false,
        }
    }

    /// This context aimed at a damage target.
    pub fn at_target(self, position: Vec2, health_pct: Option<f32>, enemies_near_target: u32) -> Self {
        Self {
            target_position: Some(position),
            target_health_pct: health_pct,
            enemies_near_target,
            ..self
        }
    }
}

/// What passives read about their owner, queried alike wherever passives run. Health is
/// passed in separately because some of those systems also heal the owner.
#[derive(QueryData)]
pub struct PassiveOwner {
    velocity: Option<&'static LinearVelocity>,
    action_state: Option<&'static ActionState<PlayerAction>>,
    horizontal_intent: Option<&'static HorizontalInputIntent>,
    enemies_nearby: Option<&'static EnemiesNearby>,
    in_air: Has<InAir>,
    enemy_proximity: Option<&'static EnemyProximitySensor>,
    last_grounded: Option<&'static LastGroundedPosition>,
    permutator: Option<&'static permutator::PermutatorState>,
}

impl PassiveOwnerItem<'_, '_> {
    pub fn context<'a>(&self, health: Option<&'a Health>) -> PassiveContext<'a> {
        // The melee-range sensor is more precise than the coarse count when present.
        let enemies_nearby = self.enemy_proximity.map_or_else(
            || self.enemies_nearby.map_or(0, |enemies| enemies.0),
            |sensor| sensor.enemies_in_melee_range.len() as u32,
        );
        PassiveContext {
            velocity: self.velocity.map_or(Vec2::ZERO, |velocity| velocity.0),
            enemies_nearby,
            in_air: self.in_air,
            last_grounded_position: self.last_grounded.map(|grounded| grounded.position),
            movement_input: self.horizontal_intent.map_or(0.0, |intent| intent.movement),
            jump_pressed: self.action_state.is_some_and(|input| input.pressed(&PlayerAction::Jump)),
            rotation_active: self.permutator.is_some_and(|state| state.active),
            ..PassiveContext::with_health(health.unwrap_or(&NO_HEALTH))
        }
    }
}

/// Modifiers that passives can apply to damage sources.
#[derive(Debug, Clone)]
pub struct DamageModifiers {
    pub damage_multiplier: f32,
    pub can_backstab: bool,
    pub backstab_multiplier: f32,
}

impl Default for DamageModifiers {
    fn default() -> Self {
        Self {
            damage_multiplier: 1.0,
            can_backstab: false,
            backstab_multiplier: 2.0,
        }
    }
}

/// Returns the set of projectile directions after applying passive modifiers.
/// The returned vectors are normalized; caller is responsible for scaling by projectile speed.
pub fn projectile_directions(
    passives: &super::Passives,
    base_direction: Vec2,
) -> Vec<Vec2> {
    let len = base_direction.length();
    if len <= f32::EPSILON {
        return Vec::new();
    }
    let dir = base_direction / len;
    if passives.contains(&super::Passive::Prism) {
        let angle = dir.y.atan2(dir.x);
        let prism_angle_deg = passives
            .passive_effective_value(super::Passive::Prism)
            .unwrap_or(5.0);
        let offset = prism_angle_deg.to_radians();
        return vec![
            Vec2::from_angle(angle),
            Vec2::from_angle(angle + offset),
            Vec2::from_angle(angle - offset),
        ];
    }
    vec![dir]
}
