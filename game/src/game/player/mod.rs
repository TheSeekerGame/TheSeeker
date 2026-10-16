mod collision_view;
pub use passives::definition::{Passive, PassiveRollSpec};
mod autoaim;
mod collision;
mod dust;
mod equipment;
pub mod material_inventory;
mod player_action;
mod player_anim;
pub mod progression;
pub mod weapon;

mod input_buffer;
pub mod passives;
mod sensors;
pub mod skills;
pub mod states;
pub(crate) mod foreground;

// Thin orchestration module
pub mod orchestration;
// Focused splits
pub mod crits;
pub mod spawn;
pub mod spawns;
pub mod stats;
mod scatter_homing;
// Pogo is handled inside states::skill::attacking; effects live under crate::game::effects

pub(crate) use input_buffer::InputBuffer;
pub use material_inventory::{
    CraftMaterial, MaterialInventory,
};
pub use sensors::{EnemyProximitySensor, LastGroundedPosition};
pub(crate) use sensors::GroundSensor;
pub(crate) use sensors::{RecoveryCollisionWorld, SafeGroundedPosition};
use strum::IntoEnumIterator;
use theseeker_engine::animation::SpriteAnimationBundle;
use theseeker_engine::gent::{Gent, GentPhysicsBundle, TransformGfxFromGent};
use bevy::sprite::Sprite;
use bevy::transform::components::Transform;
use std::collections::HashMap;

use crate::game::combat::{Health, MAX_HEALTH};
use crate::prelude::*;

use super::physics::Knockback;

pub use player_action::PlayerAction;

pub use states::{
    Attacking, Dashing, Elevator, Falling, Grounded, Idle, Ready, Running,
    Whirling,
};

pub use orchestration::PlayerPlugin; // re-export thin orchestration plugin
pub use progression::{AdaptationProgress, Alignment, Essence};
pub(crate) use progression::EssenceOrbCadence;

/// Orders player behavior, transitions, collisions, and animations relative to each other.
pub use orchestration::{PlayerBehaviorSet, PlayerStateSet};
pub use stats::{EnemiesNearby, PlayerStatMod, PlayerStats};

// TODO: switch to a dedicated player spawnpoint entity
#[derive(Bundle, Default)]
pub struct PlayerBlueprintBundle {
    marker: PlayerBlueprint,
}

#[derive(Bundle)]
pub struct PlayerGentBundle {
    player: Player,
    marker: Gent,
    phys: GentPhysicsBundle,
}

#[derive(Bundle)]
pub struct PlayerGfxBundle {
    marker: PlayerGfx,
    gent2gfx: TransformGfxFromGent,
    sprite: Sprite,
    transform: Transform,
    animation: SpriteAnimationBundle,
}

#[derive(Component, Default)]
pub struct PlayerBlueprint;

#[derive(Component)]
pub struct Player;

#[derive(Component)]
pub struct PlayerGfx {
    pub e_gent: Entity,
    /// The resting sprite anchor, restored when an action releases presentation.
    pub base_offset: Vec3,
}

pub(crate) const PLAYER_GFX_OFFSET: Vec3 = Vec3::new(0.0, 1.0, 0.0);

#[derive(Component, Debug, Clone)]
pub struct Passives {
    /// Currently equipped passives (up to 4)
    pub equipped: Vec<Passive>,
    /// Unlocked passives available in the inventory but not equipped
    pub inventory: Vec<Passive>,
    /// Progression data for unlocked passives
    pub owned: HashMap<Passive, PassiveInstance>,
}

#[derive(Debug, Clone)]
pub struct PassiveInstance {
    /// Rolled value for randomizable passives, None for binary passives.
    pub roll: Option<f32>,
}

impl PassiveInstance {
    pub fn new(roll: Option<f32>) -> Self {
        Self { roll }
    }
}

impl Passives {
    /// Maximum number of passives player can have equipped at once
    pub const MAX_EQUIPPED: usize = 4;
}

impl Default for Passives {
    fn default() -> Self {
        Passives {
            equipped: Vec::with_capacity(Passives::MAX_EQUIPPED),
            inventory: Vec::new(),
            owned: HashMap::new(),
        }
    }
}

impl Passives {
    /// Add a passive to the inventory without equipping it.
    pub fn add_passive(&mut self, passive: Passive) {
        let default_roll = passive.roll_spec().map(|s| s.default);
        self.unlock_with_roll(passive, default_roll);
    }

    pub fn unlock_with_roll(
        &mut self,
        passive: Passive,
        roll: Option<f32>,
    ) -> bool {
        let was_unlocked = self.is_unlocked(passive);
        self.store_passive_roll(passive, roll);

        let already_visible = self.equipped.contains(&passive)
            || self.inventory.contains(&passive);
        if !already_visible {
            self.inventory.push(passive);
        }

        !was_unlocked
    }

    fn store_passive_roll(&mut self, passive: Passive, roll: Option<f32>) {
        self.owned
            .entry(passive)
            .and_modify(|existing| {
                existing.roll = roll.or(existing.roll);
            })
            .or_insert_with(|| PassiveInstance::new(roll));
    }

    pub fn is_unlocked(&self, passive: Passive) -> bool {
        self.owned.contains_key(&passive)
    }

    pub fn passive_roll(&self, passive: Passive) -> Option<f32> {
        self.owned.get(&passive).and_then(|entry| entry.roll)
    }

    pub fn passive_effective_value(&self, passive: Passive) -> Option<f32> {
        passive
            .roll_spec()
            .map(|spec| self.passive_roll(passive).unwrap_or(spec.default))
    }

    pub fn set_passive_roll(&mut self, passive: Passive, roll: f32) {
        let entry = self
            .owned
            .entry(passive)
            .or_insert_with(|| PassiveInstance::new(Some(roll)));
        entry.roll = Some(roll);

        let already_visible = self.equipped.contains(&passive)
            || self.inventory.contains(&passive);
        if !already_visible {
            self.inventory.push(passive);
        }
    }

    pub fn unlock_all_max(&mut self) {
        for passive in Passive::iter() {
            self.unlock_or_maximize_passive(passive);
        }
    }

    fn unlock_or_maximize_passive(&mut self, passive: Passive) {
        let best_roll = passive.roll_spec().map(PassiveRollSpec::best_value);
        if !self.is_unlocked(passive) {
            self.unlock_with_roll(passive, best_roll);
            return;
        }

        let Some(roll) = best_roll else {
            return;
        };
        self.set_passive_roll(passive, roll);
    }

    /// Equip a passive from inventory.
    pub fn equip_passive(&mut self, passive: Passive) -> bool {
        if !self.is_unlocked(passive) {
            return false;
        }
        let Some(inv_pos) = self.inventory.iter().position(|p| *p == passive)
        else {
            return false;
        };

        // Elemental crystals are mutually exclusive; equipping one
        // auto-unequips the other crystal.
        self.move_conflicting_passive_to_inventory(passive);

        if self.equipped.len() >= Passives::MAX_EQUIPPED {
            return false;
        }

        self.inventory.remove(inv_pos);
        if !self.equipped.contains(&passive) {
            self.equipped.push(passive);
        }
        true
    }

    fn move_conflicting_passive_to_inventory(&mut self, passive: Passive) {
        let Some(conflict) = passive.mutually_exclusive_with() else {
            return;
        };
        let Some(eq_pos) = self.equipped.iter().position(|p| *p == conflict)
        else {
            return;
        };

        self.equipped.remove(eq_pos);
        if !self.inventory.contains(&conflict) {
            self.inventory.push(conflict);
        }
    }

    /// Unequip a passive back to inventory
    pub fn unequip_passive(&mut self, passive: Passive) -> bool {
        let Some(pos) = self.equipped.iter().position(|p| *p == passive)
        else {
            return false;
        };

        self.equipped.remove(pos);
        if !self.inventory.contains(&passive) {
            self.inventory.push(passive);
        }
        true
    }

    /// Check if a passive is equipped.
    pub fn contains(&self, passive: &Passive) -> bool {
        self.equipped.iter().any(|p| p == passive)
    }
}

#[derive(Component, Debug, Clone)]
pub struct SkillInventory {
    /// Currently equipped skills (up to 5 with Battery Pack)
    pub equipped: Vec<skills::types::SkillId>,
    /// Skills in inventory but not equipped
    pub inventory: Vec<skills::types::SkillId>,
}

impl SkillInventory {
    /// Base number of skills player can have equipped without Battery Pack.
    pub const MAX_EQUIPPED: usize = 4;
}

impl Default for SkillInventory {
    fn default() -> Self {
        SkillInventory {
            equipped: vec![
                skills::types::SkillId::Attack,
                skills::types::SkillId::TetherNode,
            ],
            inventory: Vec::new(),
        }
    }
}

impl SkillInventory {
    pub const DEBUG_UNLOCK_ORDER: [skills::types::SkillId; 19] = [
        skills::types::SkillId::Attack,
        skills::types::SkillId::TetherNode,
        skills::types::SkillId::BowAttack,
        skills::types::SkillId::Dash,
        skills::types::SkillId::Whirl,
        skills::types::SkillId::Stealth,
        skills::types::SkillId::DashStrike,
        skills::types::SkillId::FlickerStrike,
        skills::types::SkillId::BurningDash,
        skills::types::SkillId::AmplifiedBell,
        skills::types::SkillId::Spinner,
        skills::types::SkillId::ExplosiveMine,
        skills::types::SkillId::IceNova,
        skills::types::SkillId::QuarkPylon,
        skills::types::SkillId::Spit,
        skills::types::SkillId::RedBerry,
        skills::types::SkillId::BlueBerry,
        skills::types::SkillId::MossBerry,
        skills::types::SkillId::PortableTent,
    ];

    pub fn is_unlocked(&self, skill: &skills::types::SkillId) -> bool {
        self.equipped.iter().any(|equipped| equipped == skill)
            || self.inventory.iter().any(|stored| stored == skill)
    }

    pub fn unlock_skill(&mut self, skill: skills::types::SkillId) -> bool {
        if self.is_unlocked(&skill) {
            return false;
        }

        self.inventory.push(skill);
        true
    }

    pub fn unlock_skill_with_capacity(
        &mut self,
        skill: skills::types::SkillId,
        max_slots: usize,
    ) -> bool {
        let unlocked = self.unlock_skill(skill);
        if unlocked {
            self.auto_equip_skill(skill, max_slots);
        }
        unlocked
    }

    pub fn auto_equip_skill(
        &mut self,
        skill: skills::types::SkillId,
        max_slots: usize,
    ) -> bool {
        self.equip_skill_with_capacity(skill, max_slots)
    }

    pub fn unlock_all_skills(&mut self, max_slots: usize) {
        for skill in Self::DEBUG_UNLOCK_ORDER {
            self.unlock_and_auto_equip_skill(skill, max_slots);
        }
    }

    fn unlock_and_auto_equip_skill(
        &mut self,
        skill: skills::types::SkillId,
        max_slots: usize,
    ) {
        self.unlock_skill(skill);
        if self.equipped.len() < max_slots {
            self.auto_equip_skill(skill, max_slots);
        }
    }

    /// Equip a skill from inventory
    /// Equip a skill from inventory with a provided capacity limit
    pub fn equip_skill_with_capacity(
        &mut self,
        skill: skills::types::SkillId,
        max_slots: usize,
    ) -> bool {
        if self.equipped.len() >= max_slots {
            return false;
        }
        self.equip_skill_from_inventory(skill)
    }

    fn equip_skill_from_inventory(
        &mut self,
        skill: skills::types::SkillId,
    ) -> bool {
        let Some(pos) = self.inventory.iter().position(|s| *s == skill)
        else {
            return false;
        };

        self.inventory.remove(pos);
        self.equipped.push(skill);
        true
    }

    /// Unequip a skill back to inventory
    pub fn unequip_skill(&mut self, skill: skills::types::SkillId) -> bool {
        if let Some(pos) = self.equipped.iter().position(|s| *s == skill) {
            self.equipped.remove(pos);
            self.inventory.push(skill);
            return true;
        }
        false
    }

    pub fn remove_skill(&mut self, skill: skills::types::SkillId) -> bool {
        let equipped_len = self.equipped.len();
        let inventory_len = self.inventory.len();
        self.equipped.retain(|existing| *existing != skill);
        self.inventory.retain(|existing| *existing != skill);
        self.equipped.len() != equipped_len
            || self.inventory.len() != inventory_len
    }

    /// Get skill at a specific slot, returns None if the slot is empty.
    pub fn get_skill_at_slot(
        &self,
        slot: usize,
    ) -> Option<skills::types::SkillId> {
        self.equipped.get(slot).copied()
    }

    /// Check if skill is equipped
    pub fn is_equipped(&self, skill: &skills::types::SkillId) -> bool {
        self.equipped.iter().any(|s| s == skill)
    }
}

impl SkillInventory {
    /// Maximum equipped skill slots supported by current passives (4 by default, 5 with Battery Pack)
    pub fn max_slots_for(passives: &Passives) -> usize {
        if passives.contains(&Passive::BatteryPack) {
            5
        } else {
            4
        }
    }
}

/// Ensure equipped skills do not exceed capacity when passives change (e.g., Battery Pack unequipped)
pub(crate) fn enforce_skill_slot_capacity(
    mut query: Query<(&Passives, &mut SkillInventory), With<Player>>,
) {
    for (passives, mut skills) in query.iter_mut() {
        let max_slots = SkillInventory::max_slots_for(passives);
        enforce_skill_capacity_for_inventory(&mut skills, max_slots);
    }
}

fn enforce_skill_capacity_for_inventory(
    skills: &mut SkillInventory,
    max_slots: usize,
) {
    if skills.equipped.len() <= max_slots {
        return;
    }

    let overflow = skills.equipped.split_off(max_slots);
    let missing = overflow
        .into_iter()
        .rev()
        .filter(|skill| !skills.inventory.contains(skill))
        .collect::<Vec<_>>();
    skills.inventory.extend(missing);
}

// Pseudo-States
// Components that enable behaviours when attached and store runtime state for them.

/// Locks velocity briefly after a successful hit for hit-freeze feedback.
/// Tracks the last attack entity to prevent repeated triggers by the same hitbox.
#[derive(Component, Default, Debug)]
pub struct HitFreezeTime(u32, Option<Entity>);

#[derive(Component, Default, Debug)]
pub struct JumpCount(u8);

impl JumpCount {
    pub fn reset(&mut self) {
        self.0 = 2;
    }
}

/// Indicates that sliding is tracked for this entity
#[derive(Component, Default, Debug)]
pub struct WallSlideTime(f32);

/// Tracks the cooldown for the available energy for the players whirl
#[derive(Component, Default, Debug)]
pub struct WhirlAbility {
    pub energy: f32,
}

/// Energy pool for Flicker Strike skill
#[derive(Component, Default, Debug)]
pub struct FlickerAbility {
    pub energy: f32,
}

fn update_serpentring_health(
    mut query: Query<(&mut Health, &Passives), With<Player>>,
) {
    let Ok((mut health, passives)) = query.single_mut() else {
        return;
    };

    let target_max = serpent_ring_target_max_health(passives);
    if health.max != target_max {
        health.max = target_max;
        health.current = health.current.min(health.max);
    }
}

/// Increases player's attack and applies constant health degeneration.
fn apply_vitality_overclock(
    mut query: Query<(&Passives, &mut Health), With<Player>>,
    mut tick: Local<u32>,
) {
    *tick += 1;
    for (passives, mut health) in query.iter_mut() {
        apply_vitality_overclock_decay(passives, &mut health, *tick);
    }
}

/// Manages the Grounded component based on GroundSensor data
/// This runs after sensors are updated to sync the component state
fn manage_grounded_component(
    query: Query<
        (
            Entity,
            &sensors::GroundSensor,
            Has<Grounded>,
            Has<states::Jumping>,
        ),
        With<Player>,
    >,
    mut commands: Commands,
) {
    for (entity, ground_sensor, has_grounded, is_jumping) in query.iter() {
        sync_grounded_component(
            &mut commands,
            entity,
            ground_sensor.is_grounded,
            has_grounded,
            is_jumping,
        );
    }
}

fn serpent_ring_target_max_health(passives: &Passives) -> u32 {
    if !passives.contains(&Passive::SerpentRing) {
        return MAX_HEALTH;
    }

    let retention = passives
        .passive_effective_value(Passive::SerpentRing)
        .unwrap_or(0.5);
    ((MAX_HEALTH as f32) * retention)
        .round()
        .clamp(1.0, MAX_HEALTH as f32) as u32
}

fn apply_vitality_overclock_decay(
    passives: &Passives,
    health: &mut Health,
    tick: u32,
) {
    if !passives.contains(&Passive::VitalityOverclock) {
        return;
    }

    let deg_rate = if passives.contains(&Passive::SerpentRing) {
        40
    } else {
        20
    };
    if tick.is_multiple_of(deg_rate) && health.current > 1 {
        health.current = health.current.saturating_sub(1).max(1);
    }
}

fn sync_grounded_component(
    commands: &mut Commands,
    entity: Entity,
    is_grounded: bool,
    has_grounded: bool,
    is_jumping: bool,
) {
    if is_jumping {
        remove_grounded_if_present(commands, entity, has_grounded);
        return;
    }

    if is_grounded && !has_grounded {
        commands.entity(entity).insert(Grounded);
    } else if !is_grounded && has_grounded {
        commands.entity(entity).remove::<Grounded>();
    }
}

fn remove_grounded_if_present(
    commands: &mut Commands,
    entity: Entity,
    has_grounded: bool,
) {
    if has_grounded {
        commands.entity(entity).remove::<Grounded>();
    }
}
