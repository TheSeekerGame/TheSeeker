//! Data-driven AI system for enemies.
//! 
//! This module provides a flexible finite state machine (FSM) based AI system
//! that replaces the hard-coded enemy behaviors with data-driven logic loaded
//! from TOML assets.
//! 
//! ## Architecture
//! 
//! The system follows a Sensor→Brain→Actuator pattern:
//! - **Sensors**: Gather world information into components (target, ground, range)
//! - **Brain**: Evaluates FSM rules using sensor data, queues actions
//! - **Actuators**: Execute queued actions to affect the world
//! 
//! ## Key Design: One FSM Per Archetype
//! 
//! Each enemy archetype gets its own compiled FSM where animation keys
//! are pre-resolved using that archetype's `[anim]` table. This ensures
//! tier variants (spider_small_t2) can override animations while sharing
//! the same base FSM logic.

pub mod asset;
pub mod brain;
pub mod sensors;
pub mod systems;

pub use self::asset::{EnemyArchetype, EnemyFsm};
pub use self::brain::{CompiledFsm, CompiledRule, CompiledAction, CompiledCondition};

use bevy::prelude::*;
use std::collections::HashMap;

/// Resource that holds the current level's seed for deterministic RNG
#[derive(Resource, Default)]
pub struct LevelSeed(pub u32);

/// Plugin for the AI system
pub struct AiPlugin;

impl Plugin for AiPlugin {
    fn build(&self, app: &mut App) {
        // Initialize asset types
        app.init_asset::<EnemyArchetype>();
        app.init_asset::<EnemyFsm>();
        app.init_asset::<CompiledFsm>();
        
        // Initialize level seed resource
        app.init_resource::<LevelSeed>();
    }
}

/// Compile FSMs with archetype-specific animation mappings.
/// Each archetype's FSM has its short animation keys (e.g. "Attack") 
/// expanded to full asset keys (e.g. "anim.spider.RangedAttack").
pub fn compile_all_fsms(
    fsm_assets: &Assets<EnemyFsm>,
    arch_assets: &Assets<EnemyArchetype>,
    compiled_assets: &mut Assets<CompiledFsm>,
    preloaded: &crate::assets::PreloadedAssets,
) -> HashMap<String, Handle<CompiledFsm>> {
    let mut compiled_map = HashMap::new();

    for (_arch_handle, archetype) in arch_assets.iter() {
        let arch_id = &archetype.id;

        // Resolve the source FSM asset referenced by this archetype.
        let brain_file_name = archetype
            .brain
            .as_deref()
            .unwrap_or("generic_melee.fsm.toml");
        let brain_name_trimmed = brain_file_name.trim_end_matches(".fsm.toml");
        let fsm_key = format!("brain.{}", brain_name_trimmed);

        let Some(fsm_handle) = preloaded.get_single_asset::<EnemyFsm>(&fsm_key) else {
            error!("Archetype '{}' references brain '{}' but that asset isn't loaded.", arch_id, fsm_key);
            continue;
        };

        let Some(fsm_asset) = fsm_assets.get(&fsm_handle) else {
            error!("FSM asset for '{}' not found in Assets collection", fsm_key);
            continue;
        };

        let compiled_key = format!("brain.{}", arch_id);

        match fsm_asset.compile(Some(archetype)) {
            Ok(compiled) => {
                // Validate that all referenced animations exist
                let animation_keys = compiled.collect_animation_keys();
                if let Err(e) = archetype.validate_animations(preloaded, &animation_keys) {
                    error!("Animation validation failed for archetype '{}': {}", arch_id, e);
                    continue;
                }
                
                let handle = compiled_assets.add(compiled);
                compiled_map.insert(compiled_key.clone(), handle.clone());
            }
            Err(e) => {
                error!("Failed to compile FSM for archetype '{}': {}", arch_id, e);
            }
        }
    }

    compiled_map
}

// Re-export the main components for easier access
pub use crate::ai::components::*;
pub use self::script_bundle::{ScriptBundle, EnemyArchHandle};

// Core components used by the AI system
mod components {
    use bevy::prelude::*;
    use super::brain::{CompiledFsm, CompiledAction};
    
    /// Runtime instance of an enemy's FSM state.
    #[derive(Component)]
    pub struct FsmInstance {
        /// Handle to the compiled state machine asset
        pub brain: Handle<CompiledFsm>,
        /// Current logic state ID
        pub logic: u16,
        /// Current movement state ID  
        pub movement: u16,
        /// Per-track timers (ticks spent in current logic/movement state)
        pub timers: [u16; 2],
        /// LCG state for deterministic randomness
        pub rng_state: u32,
        /// Queued actions to execute this frame
        pub actions: Vec<CompiledAction>,
        /// Ticks since the current animation loop started (reset by AnimLoop)
        pub anim_tick: u16,
        /// ScriptPlayer slot bits – bit i == ON
        pub slot_bits: u32,
        /// Internal cooldown timers (ticks remaining)
        pub cooldowns: Vec<u16>,
        /// Ticks since entering current state or changing animation (for at_state_tick)
        pub state_tick: u16,
        /// Current animation key to detect changes
        pub current_anim_key: Option<String>,
    }

    /// Sensor tracking current target entity and distance.
    #[derive(Component, Default)]
    pub struct TargetSensor {
        pub entity: Option<Entity>,
        pub dist2: f32,
    }

    /// Sensor tracking whether enemy is on ground.
    #[derive(Component, Default)]
    pub struct GroundSensor {
        pub on: bool,
    }

    /// Sensor tracking whether target is in melee or aggro range.
    #[derive(Component, Default)]
    pub struct RangeSensor {
        pub in_melee: bool,
        pub in_aggro: bool,
    }

    /// Component to track turn cooldown and prevent rapid flipping at walls
    #[derive(Component)]
    pub struct TurnCooldown {
        pub timer: u16,
    }

    /// Sensor indicating whether the entity's health is zero.
    #[derive(Component, Default)]
    pub struct HealthSensor {
        pub zero: bool,
    }

    impl Default for TurnCooldown {
        fn default() -> Self {
            Self { timer: 0 }
        }
    }
} 

// Bundle construction separated for cleaner organization
mod script_bundle {
    use bevy::prelude::*;
    use super::components::*;
    use super::asset::EnemyArchetype;
    use super::brain::CompiledFsm;
    
    /// Handle to the archetype that spawned this enemy.
    /// Preserves deterministic archetype resolution without HashMap iteration.
    #[derive(Component, Deref, DerefMut, Clone)]
    #[repr(transparent)]
    pub struct EnemyArchHandle(pub Handle<EnemyArchetype>);
    
    /// Bundle for spawning entities with the new AI system
    #[derive(Bundle)]
    pub struct ScriptBundle {
        pub fsm: FsmInstance,
        pub target_sensor: TargetSensor,
        pub ground_sensor: GroundSensor,
        pub range_sensor: RangeSensor,
        pub turn_cooldown: TurnCooldown,
        pub arch_handle: EnemyArchHandle,
        pub health_sensor: HealthSensor,
    }

    impl ScriptBundle {
        /// Create a new ScriptBundle from an archetype ID
        pub fn from_arch(
            arch_id: &str,
            entity: Entity,
            arch_assets: &Assets<EnemyArchetype>,
            fsm_assets: &Assets<CompiledFsm>,
            preloaded: &crate::assets::PreloadedAssets,
            level_seed: u32,
        ) -> Option<Self> {
            // Get the archetype handle from preloaded assets
            let arch_handle = preloaded.get_single_asset(&format!("arch.{}", arch_id))?;
            debug!("Found archetype handle for: arch.{}", arch_id);
            
            // Get the archetype asset
            let archetype = arch_assets.get(&arch_handle)?;
            debug!("Got archetype asset: {:?}", archetype.id);
            
            // The compiled FSM is stored per-archetype under key `brain.{arch_id}`
            // so lookup that directly.  This guarantees the animation map matches
            // the exact tier/variant of this enemy.
            let brain_key = format!("brain.{}", arch_id);
            debug!("Looking for brain key: {}", brain_key);
            
            let brain_handle = preloaded.get_single_asset::<CompiledFsm>(&brain_key)?;
            debug!("Found brain handle for: {} -> {:?}", brain_key, brain_handle.id());
            
            // Get the compiled FSM to find start states
            let compiled_fsm = fsm_assets.get(&brain_handle)?;
            debug!("Got compiled FSM with start states: logic={}, movement={}", 
                compiled_fsm.inner.start_logic, compiled_fsm.inner.start_movement);
            
            // Create FSM instance with start states
            let fsm = FsmInstance {
                brain: brain_handle,
                logic: compiled_fsm.inner.start_logic,
                movement: compiled_fsm.inner.start_movement,
                timers: [0, 0],
                // Use the full 64-bit entity bits for a stronger, collision-resistant seed
                // as per §4.4 of the specification.
                rng_state: {
                    let bits = entity.to_bits();          // 64-bit unique identifier
                    // Mix upper and lower 32 bits then xor with the level seed
                    ((bits ^ (bits >> 32)) as u32) ^ level_seed
                },
                // Pre-allocate a slightly larger action buffer to avoid growth when
                // many delayed/state-delayed actions fire on the same frame. The small
                // extra memory per entity (8 more enum slots ≈ 64 bytes) pays off by
                // ensuring zero allocs up to fairly complex behaviours.
                actions: Vec::with_capacity(16),
                anim_tick: 0,
                slot_bits: 0,
                cooldowns: vec![0; compiled_fsm.inner.cooldown_names.len()],
                state_tick: 0,
                current_anim_key: None,
            };
            
            Some(Self {
                fsm,
                target_sensor: TargetSensor::default(),
                ground_sensor: GroundSensor::default(),
                range_sensor: RangeSensor::default(),
                turn_cooldown: TurnCooldown::default(),
                arch_handle: EnemyArchHandle(arch_handle.clone()),
                health_sensor: HealthSensor::default(),
            })
        }
    }
} 