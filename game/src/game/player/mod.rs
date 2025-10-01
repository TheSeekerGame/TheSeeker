mod collision;
mod equipment;
mod player_action;
mod player_anim;
pub mod weapon;

mod input_buffer;
pub mod passives;
mod sensors;
pub mod skills;
pub mod states;

// Thin orchestration module
pub mod orchestration;
// Focused splits
pub mod autoaim;
pub mod crits;
pub mod spawn;
pub mod spawns;
pub mod stats;
// Pogo is handled inside states::skill::attacking; effects live under crate::game::effects

use input_buffer::InputBuffer;
pub use sensors::{EnemyProximitySensor, LastGroundedPosition};
use strum::IntoEnumIterator;
use strum_macros::EnumIter;
use theseeker_engine::animation::SpriteAnimationBundle;
use theseeker_engine::gent::{Gent, GentPhysicsBundle, TransformGfxFromGent};
// use theseeker_engine::physics::LinearVelocity;
use bevy::sprite::Sprite;
use bevy::transform::components::Transform;

use crate::game::combat::{DamageInfo, Health, MAX_HEALTH};
use crate::prelude::*;

// (legacy constants removed; see state modules for current values)

use super::physics::Knockback;
//

pub use player_action::PlayerAction;

// Re-export states for backward compatibility
pub use states::{
    Attacking, BurningDashing, DashStrike, Dashing, Falling, FlickerStriking,
    Grounded, Idle, OverridesLocomotion, Ready, Running, Whirling,
};

pub use orchestration::PlayerPlugin; // re-export thin orchestration plugin

/// Orders player behavior, transitions, collisions, and animations relative to each other.
pub use orchestration::PlayerStateSet;
// Re-export moved types for compatibility
pub use autoaim::BowAutoAimState;
/// Reset auto-aim state when stealth toggles to avoid stale locks across boundaries.
pub(crate) fn reset_autoaim_on_stealth_change(
    mut added: Query<
        Entity,
        (
            With<Player>,
            Added<crate::game::effects::stealthed::StealthEffect>,
        ),
    >,
    mut removed: RemovedComponents<crate::game::effects::stealthed::StealthEffect>,
    players: Query<Entity, With<Player>>,
    mut state: ResMut<autoaim::BowAutoAimState>,
) {
    let mut touched = false;
    // On stealth added, clear any cooldown/locks to allow immediate reacquisition
    for _ in added.iter_mut() {
        touched = true;
    }
    // On stealth removed, also clear state to avoid lingering decisiveness
    for e in removed.read() {
        if players.contains(e) {
            touched = true;
        }
    }
    if touched {
        state.decisive = false;
        state.target = None;
        state.lock_ticks = 0;
        state.cooldown_ticks = 0;
        state.linger_ticks = 0;
        state.linger_used = false;
        state.target_enemy = None;
    }
}
pub use autoaim::BowAutoAimState as _BowAutoAimStateForVisibility; // ensure module visible
pub use stats::{EnemiesNearby, PlayerStatMod, PlayerStats, StatType};

// TODO: switch to a dedicated player spawnpoint entity
#[derive(Bundle, LdtkEntity, Default)]
pub struct PlayerBlueprintBundle {
    marker: PlayerBlueprint,
}

#[derive(Bundle)]
pub struct PlayerGentBundle {
    player: Player,
    marker: Gent,
    phys: GentPhysicsBundle,
    coyote_time: CoyoteTime,
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
}

#[derive(Component, Debug)]
pub struct Passives {
    /// Currently equipped passives (up to 4)
    pub equipped: Vec<Passive>,
    /// Passives available in the inventory but not equipped
    pub inventory: Vec<Passive>,
}

impl Passives {
    /// Maximum number of passives player can have equipped at once
    pub const MAX_EQUIPPED: usize = 4;
}

impl Default for Passives {
    fn default() -> Self {
        let passives: Vec<Passive> = Passive::iter().collect();
        Passives {
            equipped: Vec::with_capacity(Passives::MAX_EQUIPPED),
            inventory: passives,
        }
    }
}

impl Passives {
    /// Add a passive to the inventory (not equipped)
    pub fn add_passive(&mut self, passive: Passive) {
        self.inventory.push(passive);
    }

    /// Equip a passive from inventory
    pub fn equip_passive(&mut self, passive: Passive) -> bool {
        if self.equipped.len() < Passives::MAX_EQUIPPED {
            if let Some(pos) = self.inventory.iter().position(|p| *p == passive)
            {
                self.inventory.remove(pos);
                self.equipped.push(passive);
                return true;
            }
        }
        false
    }

    /// Unequip a passive back to inventory
    pub fn unequip_passive(&mut self, passive: Passive) -> bool {
        if let Some(pos) = self.equipped.iter().position(|p| *p == passive) {
            self.equipped.remove(pos);
            self.inventory.push(passive);
            return true;
        }
        false
    }

    /// Check if passive is equipped
    pub fn is_equipped(&self, passive: &Passive) -> bool {
        self.equipped.iter().any(|p| p == passive)
    }

    /// Iterator over equipped passives
    pub fn iter(&self) -> std::slice::Iter<Passive> {
        self.equipped.iter()
    }

    /// Get total passive count (equipped + inventory)
    pub fn total_count(&self) -> usize {
        self.equipped.len() + self.inventory.len()
    }

    /// Check if a passive is equipped (compatibility method)
    pub fn contains(&self, passive: &Passive) -> bool {
        self.equipped.iter().any(|p| p == passive)
    }
}

#[derive(Component, Debug)]
pub struct SkillInventory {
    /// Currently equipped skills (up to 4)
    pub equipped: Vec<skills::types::SkillId>,
    /// Skills in inventory but not equipped
    pub inventory: Vec<skills::types::SkillId>,
}

impl SkillInventory {
    /// Maximum number of skills player can have equipped at once
    pub const MAX_EQUIPPED: usize = 4;
}

impl Default for SkillInventory {
    fn default() -> Self {
        // Default equipped skills, BurningDash is in inventory but not equipped
        SkillInventory {
            equipped: vec![
                skills::types::SkillId::Attack,
                skills::types::SkillId::Dash,
                skills::types::SkillId::Whirl,
                skills::types::SkillId::Stealth,
            ],
            inventory: vec![
                skills::types::SkillId::DashStrike,
                skills::types::SkillId::BurningDash,
                skills::types::SkillId::FlickerStrike,
                skills::types::SkillId::AmplifiedBell,
                skills::types::SkillId::Spinner,
                skills::types::SkillId::ExplosiveMine,
                skills::types::SkillId::IceNova,
            ],
        }
    }
}

impl SkillInventory {
    /// Equip a skill from inventory
    pub fn equip_skill(&mut self, skill: skills::types::SkillId) -> bool {
        if self.equipped.len() < SkillInventory::MAX_EQUIPPED {
            if let Some(pos) = self.inventory.iter().position(|s| *s == skill) {
                self.inventory.remove(pos);
                self.equipped.push(skill);
                return true;
            }
        }
        false
    }

    /// Equip a skill from inventory with a provided capacity limit
    pub fn equip_skill_with_capacity(
        &mut self,
        skill: skills::types::SkillId,
        max_slots: usize,
    ) -> bool {
        if self.equipped.len() < max_slots {
            if let Some(pos) = self.inventory.iter().position(|s| *s == skill) {
                self.inventory.remove(pos);
                self.equipped.push(skill);
                return true;
            }
        }
        false
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

    /// Get skill at a specific slot (0-3), returns None if slot empty
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
        // If capacity reduced, move overflow skills back to inventory, newest last first
        while skills.equipped.len() > max_slots {
            if let Some(removed) = skills.equipped.pop() {
                // Only add back to inventory if not already present
                if !skills.inventory.contains(&removed) {
                    skills.inventory.push(removed);
                }
            } else {
                break;
            }
        }
    }
}

// they could also be components...limit only by the pickup/gain function instead of sized hashmap
#[derive(Debug, Eq, PartialEq, Hash, EnumIter, Clone, Copy)]
pub enum Passive {
    /// Heal after killing an enemy
    Bloodstone,
    /// Crit on every 2nd and 3rd hit when on low health
    FlamingHeart,
    /// Deal double damage when backstabbing
    IceDagger,
    /// Defense scaling based on number of enemies nearby
    GlowingShard,
    /// Crits lower cooldown of all abilities by 0.5 seconds
    ObsidianNecklace,
    /// Doubled damage & defence while standing still,but halved while moving
    HeavyBoots,
    /// Move faster, get cooldown redudction, but take double damage
    SerpentRing,
    /// Sacrifice health but get increased cooldown reduction for every consecutive hit within 3 seconds
    FrenziedAttack,
    /// Deal more damage to clustered enemies, less to isolated ones
    PackKiller,
    /// Get increased defense when you're in the air, but become more vulnerable when on the ground.
    DeadlyFeather,
    /// Scale damage based on distance between you and nearest enemy
    Sharpshooter,
    /// Limits the damage taken from any attack to 1/3 of your maximum health.
    ProtectiveSpirit,
    /// Gain 1 extra jump.
    RabbitsFoot,
    /// Critical hits heal you
    CriticalRegeneration,
    /// Increases damage based on health percentage, at the cost of constant health degeneration.
    VitalityOverclock,
    /// Spawns 3 orbs rotating around the player that damage nearby enemies.
    KineticOrb,
    /// Adds one extra active Skill slot (5th slot) while equipped.
    BatteryPack,
    /// Stay stealthed while dealing damage; disables stealth damage/heal bonuses.
    ShadowCloak,
    /// Deal double damage to enemies above 80% health.
    PrimeSygil,
    /// Dash Strike crits trigger all instant skills
    PulseDrive,
    /// Halves cooldowns and energy while alternating skills; disabled on repeats.
    Permutator,
}

impl Passive {
    pub fn name(&self) -> &str {
        match self {
            Passive::Bloodstone => "Bloodstone",
            Passive::FlamingHeart => "Flaming Heart",
            Passive::IceDagger => "Ice Dagger",
            Passive::GlowingShard => "Glowing Shard",
            Passive::ObsidianNecklace => "Obsidian Necklace",
            Passive::HeavyBoots => "Heavy Boots",
            Passive::SerpentRing => "Serpent Ring",
            Passive::FrenziedAttack => "Frenzied Attack",
            Passive::PackKiller => "Pack Killer",
            Passive::DeadlyFeather => "Deadly Feather",
            Passive::Sharpshooter => "Sharpshooter",
            Passive::ProtectiveSpirit => "Protective Spirit",
            Passive::RabbitsFoot => "Elastic Accelerator",
            Passive::CriticalRegeneration => "Critical Regeneration",
            Passive::VitalityOverclock => "Vitality Overclock",
            Passive::KineticOrb => "Kinetic Orb",
            Passive::BatteryPack => "Battery Pack",
            Passive::ShadowCloak => "Shadow Cloak",
            Passive::PrimeSygil => "Prime Sygil",
            Passive::PulseDrive => "Pulse Drive",
            Passive::Permutator => "Permutator",
        }
    }

    pub fn description(&self) -> &str {
        match self {
            Passive::Bloodstone => "Heal after kills",
            Passive::FlamingHeart => "Crit on every 2nd and 3rd hit when on low health",
            Passive::IceDagger => "Deal double damage when backstabbing. Backstab kills reset all skill cooldowns.",
            Passive::GlowingShard => "Defense scaling based on number of enemies nearby",
            Passive::ObsidianNecklace => "Get +11% crit chance. Crits lower cooldown of all abilities by 0.5 seconds",
            Passive::HeavyBoots => "Doubled damage & defence while standing still,but halved while moving",
            Passive::SerpentRing => "Move faster, get cooldown redudction, but your life gets cut in half",
            Passive::FrenziedAttack => "Sacrifice health but get increased cooldown reduction for every consecutive hit within 3 seconds",
            Passive::PackKiller => "Deal more damage to packs of enemies",
            Passive::DeadlyFeather => "Get +30% cooldown reduction and rhythmic crits when airborne, but -50% defense when grounded",
            Passive::Sharpshooter => "Scale damage based on distance between you and nearest enemy. Distance value only updated when Running.",
            Passive::ProtectiveSpirit => "Damage you take from any attack is limited to 1/3rd of your maximum health",
            Passive::RabbitsFoot => "Gain 1 extra jump, move 20% faster, and get +9% crit chance",
            Passive::CriticalRegeneration => "Critical hits heal you",
            Passive::VitalityOverclock => "Gain increased damage based on health percentage, at the cost of constant health degeneration",
            Passive::KineticOrb => "Three orbs orbit you, damaging up to 3 enemies per full rotation. Orb speed scales with your 1152‑tick average speed.",
            Passive::BatteryPack => "Adds a 5th active skill slot. The extra slot is removed when unequipped.",
            Passive::ShadowCloak => "While stealthed, dealing damage does not break stealth. Disables stealth’s double-damage and lifesteal effects while active.",
            Passive::PrimeSygil => "Deal double damage to enemies above 80% health.",
            Passive::PulseDrive =>
                "Dash Strike critical hits instantly trigger all equipped instant skills.",
            Passive::Permutator => "Halves all cooldowns and energy cost while alternating skills. Repeating a skill disables the effect until you use a different one.",
        }
    }

    pub fn icon_path(&self) -> &'static str {
        match self {
            Passive::Bloodstone => "items/passives/Bloodstone.png",
            Passive::FlamingHeart => "items/passives/FlamingHeart.png",
            Passive::IceDagger => "items/passives/IceDagger.png",
            Passive::GlowingShard => "items/passives/GlowingShard.png",
            Passive::ObsidianNecklace => "items/passives/ObsidianNecklace.png",
            Passive::HeavyBoots => "items/passives/HeavyBoots.png",
            Passive::SerpentRing => "items/passives/SerpentRing.png",
            Passive::FrenziedAttack => "items/passives/FrenziedAttack.png",
            Passive::PackKiller => "items/passives/PackKiller.png",
            Passive::DeadlyFeather => "items/passives/DeadlyFeather.png",
            Passive::Sharpshooter => "items/passives/Sharpshooter.png",
            Passive::ProtectiveSpirit => "items/passives/ProtectiveSpirit.png",
            Passive::RabbitsFoot => "items/passives/RabbitsFoot.png",
            Passive::CriticalRegeneration => {
                "items/passives/CriticalRegeneration.png"
            },
            Passive::VitalityOverclock => {
                "items/passives/VitalityOverclock.png"
            },
            Passive::KineticOrb => "items/passives/KineticOrb.png",
            Passive::BatteryPack => "items/passives/BatteryPack.png",
            Passive::ShadowCloak => "items/passives/ShadowCloak.png",
            Passive::PrimeSygil => "items/passives/PrimeSygil.png",
            Passive::PulseDrive => "items/passives/PulseDrive.png",
            Passive::Permutator => "items/passives/Permutator.png",
        }
    }
}

// Spawn/despawn logic lives in spawn.rs. Legacy helpers have been removed across states.

// Pseudo-States
// Components that enable behaviours when attached and store runtime state for them.

/// Locks velocity briefly after a successful hit for hit-freeze feedback.
/// Tracks the last attack entity to prevent repeated triggers by the same hitbox.
#[allow(dead_code)]
#[derive(Component, Default, Debug)]
pub struct HitFreezeTime(u32, Option<Entity>);

#[derive(Component, Default, Debug)]
pub struct CoyoteTime(#[allow(dead_code)] f32);

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
// Legacy helpers removed

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

// Stealth component lives under effects::stealthed. Other legacy notes removed for clarity.

#[derive(Default, Component)]
pub struct BuffTick {
    pub falloff: u32,
    pub stacks: u32,
}

fn track_hits(
    mut query: Query<
        (
            Entity,
            &Passives,
            &mut Health,
            &mut BuffTick,
        ),
        With<Player>,
    >,
    mut _damage_events: EventReader<DamageInfo>,
) {
    if let Ok((_player_e, _passives, mut _health, mut buff)) = query.single_mut() {
        // Legacy FrenziedAttack stacking removed in favor of runtime cooldown-to-health conversion
        // Preserve BuffTick maintenance for potential future use elsewhere
        buff.falloff = buff.falloff.saturating_sub(1);
        if buff.falloff == 0 {
            buff.stacks = 0
        }
    }
}

//on gain passive event?
fn update_serpentring_health(
    mut query: Query<(&mut Health, &Passives), With<Player>>,
) {
    if let Ok((mut health, passives)) = query.single_mut() {
        let target_max = if passives.contains(&Passive::SerpentRing) {
            ((MAX_HEALTH as f32) / 2.0) as u32
        } else {
            MAX_HEALTH
        };
        if health.max != target_max {
            health.max = target_max;
            if health.current > health.max {
                health.current = health.max;
            }
        }
    }
}

/// Increases player's attack and applies constant health degeneration.
fn apply_vitality_overclock(
    mut query: Query<(&Passives, &mut Health), With<Player>>,
    mut tick: Local<u32>,
) {
    *tick += 1;
    for (passives, mut health) in query.iter_mut() {
        if passives.contains(&Passive::VitalityOverclock) {
            let deg_rate = if passives.contains(&Passive::SerpentRing) {
                40
            } else {
                20
            };
            // Every n ticks (depending on deg_tick rate), apply the health degeneration.
            if *tick % deg_rate == 0 && health.current > 1 {
                let mut deg = 1;
                health.current = health.current.saturating_sub(deg).max(1);
            }
        }
    }
}

/// Manages the Grounded component based on GroundSensor data
/// This runs after sensors are updated to sync the component state
fn manage_grounded_component(
    mut query: Query<
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
        // Use hysteresis to prevent flickering between grounded and not grounded
        const GROUND_THRESHOLD: f32 = 1.1; // Slightly above GROUNDED_THRESHOLD (1.0)
        const UNGROUND_THRESHOLD: f32 = 2.0; // Must be clearly off ground to transition

        // CRITICAL: Never add Grounded while jumping - let the jump complete naturally
        if is_jumping {
            // Remove Grounded if jumping (shouldn't happen but be defensive)
            if has_grounded {
                commands.entity(entity).remove::<Grounded>();
            }
            continue;
        }

        // Add Grounded if close enough to ground and not already grounded
        if ground_sensor.distance < GROUND_THRESHOLD && !has_grounded {
            commands.entity(entity).insert(Grounded);
        }
        // Remove Grounded only if clearly off the ground
        else if ground_sensor.distance > UNGROUND_THRESHOLD && has_grounded {
            commands.entity(entity).remove::<Grounded>();
        }
        // In the hysteresis zone (GROUND_THRESHOLD to UNGROUND_THRESHOLD), maintain current state
    }
}

// Manages dash cooldown countdown
// moved to cooldowns::manage_dash_cooldown

// Manages stealth cooldown countdown
// moved to cooldowns::manage_stealth_cooldown

// Activates stealth effect when player uses stealth ability
// moved to effects::stealth::stealth_activation_system

// Updates stealth duration and visual effects
// moved to effects::stealth::stealth_effect_system

// Restores visibility when stealth ends
// moved to effects::stealth::stealth_exit_visibility_system

// Breaks stealth when player deals damage
// moved to effects::stealth::stealth_damage_break_system
