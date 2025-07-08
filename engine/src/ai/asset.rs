//! Asset definitions for data-driven enemy AI.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use serde_json;
use std::collections::HashMap;
use thiserror::Error;
use super::brain::{CompiledFsm, compile_fsm};

/// Expected schema version for asset compatibility
pub const EXPECTED_SCHEMA_VERSION: u32 = 1;

/// Errors that can occur during asset loading
#[derive(Debug, Error)]
pub enum AssetLoadError {
    #[error("Schema version mismatch: expected {expected}, got {actual}")]
    SchemaVersionMismatch { expected: u32, actual: u32 },
    
    #[error("Missing required animation: {0}")]
    MissingAnimation(String),
    
    #[error("Invalid FSM reference: {0}")]
    InvalidFsmReference(String),
    
    #[error("TOML parsing error: {0}")]
    TomlError(#[from] toml::de::Error),
    
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
    
    #[error("FSM compilation error: {0}")]
    FsmCompilationError(String),
}

/// Enemy archetype asset - defines stats, animations, and behavior reference.
/// Base archetypes define all fields; override archetypes inherit and modify.
#[derive(Debug, Clone, Serialize, Deserialize, TypePath, Asset)]
pub struct EnemyArchetype {
    /// Asset schema version for compatibility checking
    pub schema_version: u32,
    
    /// Unique identifier for this archetype
    pub id: String,
    
    /// Base archetype to inherit from (for tier variants)
    pub base: Option<String>,
    
    /// Brain FSM reference (path to .fsm.toml file)
    pub brain: Option<String>,
    
    /// Animation mappings: FSM key → asset key (e.g. "Attack" → "anim.spider.RangedAttack")
    #[serde(default)]
    pub anim: HashMap<String, String>,
    
    /// Override values (for dynamic assets)
    #[serde(default, rename = "override")]
    pub override_values: ArchetypeOverride,
    
    /// Stats (optional for override assets) - MUST BE LAST due to flatten
    #[serde(flatten, default)]
    pub stats: Option<ArchetypeStats>,
}

/// Stats that can be defined on an archetype
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArchetypeStats {
    /// Starting HP
    #[serde(default)]
    pub spawn_hp: u32,
    
    /// Walking/patrol speed in pixels/second
    #[serde(default)]
    pub walk_speed: f32,
    
    /// Chase speed in pixels/second 
    #[serde(default)]
    pub chase_speed: f32,
    
    /// Gravity acceleration in pixels/tick²
    #[serde(default = "default_fall_accel")]
    pub fall_accel: f32,
    
    /// Melee attack damage
    #[serde(default)]
    pub dmg_melee: u32,
    
    /// Ranged attack damage
    #[serde(default)]
    pub dmg_ranged: u32,
    
    /// Vision range for aggro in pixels
    #[serde(default)]
    pub vision_range: f32,
    
    /// Melee attack range in pixels
    #[serde(default)]
    pub melee_range: f32,
}

impl Default for ArchetypeStats {
    fn default() -> Self {
        Self {
            spawn_hp: 0,
            walk_speed: 0.0,
            chase_speed: 0.0,
            fall_accel: default_fall_accel(),
            dmg_melee: 0,
            dmg_ranged: 0,
            vision_range: 0.0,
            melee_range: 0.0,
        }
    }
}

fn default_fall_accel() -> f32 {
    2.4 // Default gravity from enemy config
}

/// Override section for dynamic assets
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ArchetypeOverride {
    #[serde(default)]
    pub stats: Option<PartialArchetypeStats>,
    
    #[serde(default)]
    pub anim: Option<HashMap<String, String>>,
}

/// Partial stats for overrides - all fields optional
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PartialArchetypeStats {
    pub spawn_hp: Option<u32>,
    pub walk_speed: Option<f32>,
    pub chase_speed: Option<f32>,
    pub fall_accel: Option<f32>,
    pub dmg_melee: Option<u32>,
    pub dmg_ranged: Option<u32>,
    pub vision_range: Option<f32>,
    pub melee_range: Option<f32>,
}

/// FSM asset - describes enemy behavior
#[derive(Debug, Clone, Serialize, Deserialize, TypePath, Asset)]
pub struct EnemyFsm {
    /// Asset schema version
    pub schema_version: u32,
    
    /// Starting states for each track
    pub start: FsmStartStates,
    
    /// State definitions
    #[serde(default)]
    pub state: Vec<FsmState>,
    
    /// Transition rules
    #[serde(default)]
    pub transition: Vec<FsmTransition>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FsmStartStates {
    pub logic: String,
    pub movement: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FsmState {
    pub track: String,
    pub name: String,
    
    #[serde(default)]
    pub on_enter: Vec<FsmAction>,
    
    #[serde(default)]
    pub on_exit: Vec<FsmAction>,
    
    #[serde(default)]
    pub tick: Vec<FsmAction>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FsmTransition {
    pub track: String,
    pub from: String,
    pub to: String,
    
    #[serde(default = "default_priority")]
    pub priority: u16,
    
    #[serde(default)]
    pub conditions: Vec<FsmCondition>,
    
    #[serde(default)]
    pub actions: Vec<FsmAction>,
    
    #[serde(default)]
    pub terminal: bool,
}

fn default_priority() -> u16 {
    100
}

/// FSM conditions - can be either a string key or object with parameters
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum FsmCondition {
    Simple(String),
    WithParam { 
        #[serde(flatten)]
        params: HashMap<String, serde_json::Value> 
    },
}

/// FSM actions - similar flexible format
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum FsmAction {
    Simple(String),
    WithParam {
        #[serde(flatten)]
        params: HashMap<String, serde_json::Value>
    },
}

impl EnemyArchetype {
    pub fn validate(&self) -> Result<(), AssetLoadError> {
        if self.schema_version != EXPECTED_SCHEMA_VERSION {
            return Err(AssetLoadError::SchemaVersionMismatch {
                expected: EXPECTED_SCHEMA_VERSION,
                actual: self.schema_version,
            });
        }
        
        // Base archetypes must have complete stats
        if self.base.is_none() {
            // This is a base archetype, must have stats
            let stats = self.stats.as_ref()
                .ok_or_else(|| AssetLoadError::InvalidFsmReference(
                    format!("Base archetype '{}' must have stats defined", self.id)
                ))?;
            
            // Check that essential stats are non-zero
            if stats.spawn_hp == 0 {
                return Err(AssetLoadError::InvalidFsmReference(
                    format!("Base archetype '{}' must have spawn_hp > 0", self.id)
                ));
            }
            if stats.vision_range == 0.0 {
                return Err(AssetLoadError::InvalidFsmReference(
                    format!("Base archetype '{}' must have vision_range > 0", self.id)
                ));
            }
            if stats.walk_speed == 0.0 && stats.chase_speed == 0.0 {
                return Err(AssetLoadError::InvalidFsmReference(
                    format!("Base archetype '{}' must have at least one movement speed > 0", self.id)
                ));
            }
        }
        
        Ok(())
    }
    
    /// Verify all animations referenced by FSM exist in archetype's anim map
    pub fn validate_animations(&self, preloaded: &crate::assets::PreloadedAssets, referenced_animations: &[String]) -> Result<(), AssetLoadError> {
        // Check that all animations referenced by the FSM exist in the archetype's anim map
        for anim_key in referenced_animations {
            // Skip if it's already a full asset key
            if anim_key.starts_with("anim.") {
                if preloaded.get_single_asset::<crate::assets::animation::SpriteAnimation>(anim_key).is_none() {
                    return Err(AssetLoadError::MissingAnimation(format!("Animation asset '{}' not found in preloaded assets", anim_key)));
                }
            } else {
                // It's a short key, must exist in archetype's anim map
                if !self.anim.contains_key(anim_key) {
                    return Err(AssetLoadError::MissingAnimation(format!("Animation key '{}' not found in archetype anim map", anim_key)));
                }
                
                // Also verify the resolved animation exists
                if let Some(full_key) = self.anim.get(anim_key) {
                    if preloaded.get_single_asset::<crate::assets::animation::SpriteAnimation>(full_key).is_none() {
                        return Err(AssetLoadError::MissingAnimation(format!("Animation asset '{}' (from key '{}') not found in preloaded assets", full_key, anim_key)));
                    }
                }
            }
        }
        Ok(())
    }
    
    /// Merge override archetype with base, inheriting stats/anims then applying overrides
    pub fn merge_with_base(&mut self, base: &EnemyArchetype) {
        // Always start with base stats if the base has them
        if base.stats.is_some() {
            self.stats = base.stats.clone();
        }
        
        // Apply stat overrides
        if let Some(override_stats) = &self.override_values.stats {
            if let Some(stats) = &mut self.stats {
                // Macro to reduce boilerplate for stat overrides
                macro_rules! apply_override {
                    ($($field:ident),+ $(,)?) => {
                        $(
                            if let Some(value) = override_stats.$field {
                                stats.$field = value;
                            }
                        )+
                    };
                }
                
                apply_override!(
                    spawn_hp,
                    walk_speed,
                    chase_speed,
                    fall_accel,
                    dmg_melee,
                    dmg_ranged,
                    vision_range,
                    melee_range
                );
            }
        }
        
        // Merge animations (base first, then our anims, then overrides)
        let mut merged_anims = base.anim.clone();
        merged_anims.extend(self.anim.clone());
        if let Some(override_anims) = &self.override_values.anim {
            merged_anims.extend(override_anims.clone());
        }
        self.anim = merged_anims;
        
        // Use our brain if specified, otherwise fall back to base
        if self.brain.is_none() {
            self.brain = base.brain.clone();
        }
    }
}

impl EnemyFsm {
    pub fn validate(&self) -> Result<(), AssetLoadError> {
        if self.schema_version != EXPECTED_SCHEMA_VERSION {
            return Err(AssetLoadError::SchemaVersionMismatch {
                expected: EXPECTED_SCHEMA_VERSION,
                actual: self.schema_version,
            });
        }
        
        // Validate that start states will exist (actual existence check happens during compilation)
        if self.start.logic.is_empty() {
            return Err(AssetLoadError::InvalidFsmReference("Start logic state cannot be empty".to_string()));
        }
        if self.start.movement.is_empty() {
            return Err(AssetLoadError::InvalidFsmReference("Start movement state cannot be empty".to_string()));
        }
        
        // Check that start states are defined in the state list
        let logic_exists = self.state.iter().any(|s| s.track == "logic" && s.name == self.start.logic);
        let movement_exists = self.state.iter().any(|s| s.track == "movement" && s.name == self.start.movement);
        
        if !logic_exists {
            return Err(AssetLoadError::InvalidFsmReference(format!("Start logic state '{}' not defined in state list", self.start.logic)));
        }
        if !movement_exists {
            return Err(AssetLoadError::InvalidFsmReference(format!("Start movement state '{}' not defined in state list", self.start.movement)));
        }
        
        Ok(())
    }
    
    pub fn compile(&self, archetype: Option<&crate::ai::EnemyArchetype>) -> Result<CompiledFsm, String> {
        compile_fsm(self, archetype)
    }
} 