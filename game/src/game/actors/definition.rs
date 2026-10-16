
use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use thiserror::Error;

pub const EXPECTED_SCHEMA_VERSION: u32 = 2;

#[derive(Debug, Error)]
pub enum AssetLoadError {
    #[error("Schema version mismatch: expected {expected}, got {actual}")]
    SchemaVersionMismatch { expected: u32, actual: u32 },

    #[error("Missing required animation: {0}")]
    MissingAnimation(String),

    #[error("Invalid actor definition: {0}")]
    InvalidDefinition(String),

    #[error("TOML parsing error: {0}")]
    TomlError(#[from] toml::de::Error),

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnemyArchetype {
    pub schema_version: u32,

    pub id: String,

    pub base: Option<String>,

    pub behavior: Option<super::controller::ControllerSpec>,

    pub walking: Option<super::motion::WalkMotion>,

    #[serde(default)]
    pub actions: HashMap<String, String>,

    #[serde(default)]
    pub anim: HashMap<String, String>,

    #[serde(default)]
    pub setup: Option<EnemySetup>,

    #[serde(default)]
    pub health_regen: Option<EnemyHealthRegen>,

    #[serde(default)]
    pub contact_damage: Option<EnemyContactDamage>,

    #[serde(default)]
    pub flying: Option<EnemyFlyingSetup>,

    #[serde(default)]
    pub jump: Option<EnemyJumpSetup>,

    #[serde(default)]
    pub trap: Option<EnemyTrapSetup>,

    #[serde(default)]
    pub death: Option<EnemyDeathSetup>,

    #[serde(default)]
    pub defense: Option<EnemyDefenseSetup>,

    #[serde(default, rename = "override")]
    pub override_values: ArchetypeOverride,

    #[serde(flatten, default)]
    pub stats: Option<ArchetypeStats>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArchetypeStats {
    #[serde(default)]
    pub spawn_hp: u32,

    #[serde(default)]
    pub walk_speed: f32,

    #[serde(default)]
    pub chase_speed: f32,

    #[serde(default)]
    pub dmg_melee: u32,

    #[serde(default)]
    pub dmg_ranged: u32,

    #[serde(default)]
    pub vision_range: f32,

    #[serde(default)]
    pub melee_range: f32,

    #[serde(default)]
    pub reaction_time: u8,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EnemySetup {
    pub name: Option<String>,
    pub controller: Option<EnemySetupController>,
    pub collider: Option<EnemyColliderSetup>,
    pub spawn_offset: Option<[f32; 3]>,
    pub gfx: Option<EnemyGfxSetup>,
    pub facing: Option<EnemyFacingSetup>,
    pub facing_slots: Option<EnemyFacingSlotsSetup>,
    pub needs_line_of_sight: Option<bool>,
    pub trap_prey: Option<bool>,
    pub stat_minimums: Option<EnemyStatMinimums>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EnemySetupController {
    Flying,
    Ground,
    Jump,
    StationaryTrap,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct EnemyColliderSetup {
    pub half_extents: [f32; 2],
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EnemyGfxSetup {
    pub keys: Option<Vec<String>>,
    pub offset: Option<[f32; 3]>,
    pub kind: Option<EnemyGfxKind>,
    pub pixel_aligned: Option<bool>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EnemyGfxKind {
    PixelPerfect,
    World,
    VoidSplit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EnemyFacingSetup {
    Right,
    Left,
    RandomHorizontal,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnemyFacingSlotsSetup {
    pub right: String,
    pub left: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct EnemyStatMinimums {
    pub vision_range: f32,
    pub melee_range: f32,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct EnemyHealthRegen {
    pub amount: u32,
    pub interval_ticks: u32,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct EnemyContactDamage {
    pub damage: f32,
    pub cooldown_ticks: u32,
    pub pushback: Option<EnemyContactPushback>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct EnemyContactPushback {
    pub horizontal: f32,
    pub vertical: f32,
    pub ticks: u32,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct EnemyFlyingSetup {
    pub profile: EnemyFlyingProfile,
    #[serde(default)]
    pub patrol_radius: Option<f32>,
    #[serde(default)]
    pub idle_wait: Option<[u16; 2]>,
    #[serde(default)]
    pub stalk: Option<EnemyFlyingStalkSetup>,
    #[serde(default)]
    pub attack: Option<EnemyFlyingAttackSetup>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EnemyFlyingProfile {
    Diving,
    Patrol,
    SmoothWave,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct EnemyFlyingStalkSetup {
    pub kind: EnemyFlyingStalkKind,
    #[serde(default)]
    pub trigger_distance: Option<f32>,
    #[serde(default)]
    pub approach_ratio: Option<[f32; 2]>,
    #[serde(default)]
    pub target_distance: Option<f32>,
    #[serde(default)]
    pub min_target_distance: Option<f32>,
    #[serde(default)]
    pub x_tolerance: Option<f32>,
    #[serde(default)]
    pub y_tolerance: Option<f32>,
    #[serde(default)]
    pub windup_ticks: Option<u16>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EnemyFlyingStalkKind {
    DiveSetup,
    HorizontalLine,
    Pursuit,
    None,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct EnemyFlyingAttackSetup {
    pub kind: EnemyFlyingAttackKind,
    #[serde(default)]
    pub speed_multiplier: Option<f32>,
    #[serde(default)]
    pub collider_half_extents: Option<[f32; 2]>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EnemyFlyingAttackKind {
    Dash,
    Stationary,
    None,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct EnemyJumpSetup {
    #[serde(default)]
    pub idle_ticks: Option<[u32; 2]>,
    #[serde(default)]
    pub jump_prep_ticks: Option<u32>,
    #[serde(default)]
    pub jump_cancel_lock_ticks: Option<u32>,
    #[serde(default)]
    pub horizontal_speed: Option<f32>,
    #[serde(default)]
    pub fall_multiplier: Option<f32>,
    #[serde(default)]
    pub anger: Option<EnemyJumpAngerSetup>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct EnemyJumpAngerSetup {
    pub radius: f32,
    pub escape_distance: f32,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct EnemyTrapSetup {
    #[serde(default)]
    pub body_offset_y: Option<f32>,
    #[serde(default)]
    pub detection_half_extents: Option<[f32; 2]>,
    #[serde(default)]
    pub attack_extra_x: Option<f32>,
    #[serde(default)]
    pub attack_height: Option<f32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnemyDeathSetup {
    pub mode: EnemyDeathMode,
    #[serde(default)]
    pub anim_key: Option<String>,
    #[serde(default)]
    pub left_anim_key: Option<String>,
    #[serde(default)]
    pub right_anim_key: Option<String>,
    #[serde(default)]
    pub decay_key: Option<String>,
    #[serde(default)]
    pub fall_collider_half_extents: Option<[f32; 2]>,
    #[serde(default)]
    pub fall_gfx_offset_y: Option<f32>,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct EnemyDefenseSetup {
    #[serde(default)]
    pub invulnerable_from_frame: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EnemyDeathMode {
    Decay,
    Despawn,
    Tombstone,
    DeathFall,
}

impl Default for ArchetypeStats {
    fn default() -> Self {
        Self {
            spawn_hp: 0,
            walk_speed: 0.0,
            chase_speed: 0.0,
            dmg_melee: 0,
            dmg_ranged: 0,
            vision_range: 0.0,
            melee_range: 0.0,
            reaction_time: 0,
        }
    }
}

impl EnemySetup {
    pub fn merge_with_base(&mut self, base: &EnemySetup) {
        if self.name.is_none() {
            self.name.clone_from(&base.name);
        }
        if self.controller.is_none() {
            self.controller = base.controller;
        }
        if self.collider.is_none() {
            self.collider = base.collider;
        }
        if self.spawn_offset.is_none() {
            self.spawn_offset = base.spawn_offset;
        }
        if self.facing.is_none() {
            self.facing = base.facing;
        }
        if self.facing_slots.is_none() {
            self.facing_slots.clone_from(&base.facing_slots);
        }
        if self.needs_line_of_sight.is_none() {
            self.needs_line_of_sight = base.needs_line_of_sight;
        }
        if self.trap_prey.is_none() { self.trap_prey = base.trap_prey; }
        if self.stat_minimums.is_none() {
            self.stat_minimums = base.stat_minimums;
        }

        match (&mut self.gfx, &base.gfx) {
            (Some(gfx), Some(base_gfx)) => gfx.merge_with_base(base_gfx),
            (None, Some(base_gfx)) => self.gfx = Some(base_gfx.clone()),
            _ => {},
        }
    }
}

impl EnemyGfxSetup {
    fn merge_with_base(&mut self, base: &EnemyGfxSetup) {
        if self.keys.is_none() {
            self.keys.clone_from(&base.keys);
        }
        if self.offset.is_none() {
            self.offset = base.offset;
        }
        if self.kind.is_none() {
            self.kind = base.kind;
        }
        if self.pixel_aligned.is_none() {
            self.pixel_aligned = base.pixel_aligned;
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ArchetypeOverride {
    #[serde(default)]
    pub stats: Option<PartialArchetypeStats>,

    #[serde(default)]
    pub anim: Option<HashMap<String, String>>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PartialArchetypeStats {
    pub spawn_hp: Option<u32>,
    pub walk_speed: Option<f32>,
    pub chase_speed: Option<f32>,
    pub dmg_melee: Option<u32>,
    pub dmg_ranged: Option<u32>,
    pub vision_range: Option<f32>,
    pub melee_range: Option<f32>,
    pub reaction_time: Option<u8>,
}

impl EnemyArchetype {
    pub fn validate(&self) -> Result<(), AssetLoadError> {
        if self.schema_version != EXPECTED_SCHEMA_VERSION {
            return Err(AssetLoadError::SchemaVersionMismatch {
                expected: EXPECTED_SCHEMA_VERSION,
                actual: self.schema_version,
            });
        }

        if self.base.is_some() {
            return Ok(());
        }

        validate_base_archetype_stats(&self.id, self.stats.as_ref())
    }

    pub fn validate_animations(
        &self,
        preloaded: &theseeker_engine::assets::PreloadedAssets,
        referenced_animations: &[String],
    ) -> Result<(), AssetLoadError> {
        for anim_key in referenced_animations {
            self.validate_animation_reference(preloaded, anim_key)?;
        }
        Ok(())
    }

    fn validate_animation_reference(
        &self,
        preloaded: &theseeker_engine::assets::PreloadedAssets,
        anim_key: &str,
    ) -> Result<(), AssetLoadError> {
        if anim_key.starts_with("anim.") {
            return validate_animation_asset_exists(
                preloaded,
                anim_key,
                format!(
                    "Animation asset '{anim_key}' not found in preloaded assets"
                ),
            );
        }

        let Some(full_key) = self.anim.get(anim_key) else {
            return Err(AssetLoadError::MissingAnimation(format!(
                "Animation key '{anim_key}' not found in archetype anim map"
            )));
        };

        validate_animation_asset_exists(
            preloaded,
            full_key,
            format!(
                "Animation asset '{full_key}' (from key '{anim_key}') not found in preloaded assets"
            ),
        )
    }

    pub fn merge_with_base(&mut self, base: &EnemyArchetype) {
        if base.stats.is_some() {
            self.stats.clone_from(&base.stats);
        }

        if let Some(override_stats) = &self.override_values.stats {
            apply_archetype_stat_overrides(&mut self.stats, override_stats);
        }

        let mut merged_anims = base.anim.clone();
        merged_anims.extend(self.anim.clone());
        if let Some(override_anims) = &self.override_values.anim {
            merged_anims.extend(override_anims.clone());
        }
        self.anim = merged_anims;

        if self.behavior.is_none() {
            self.behavior.clone_from(&base.behavior);
        }

        if self.walking.is_none() { self.walking.clone_from(&base.walking); }

        let mut actions = base.actions.clone();
        actions.extend(self.actions.clone());
        self.actions = actions;

        match (&mut self.setup, &base.setup) {
            (Some(setup), Some(base_setup)) => {
                setup.merge_with_base(base_setup);
            },
            (None, Some(base_setup)) => {
                self.setup = Some(base_setup.clone());
            },
            _ => {},
        }
        if self.health_regen.is_none() {
            self.health_regen = base.health_regen;
        }
        if self.contact_damage.is_none() {
            self.contact_damage = base.contact_damage;
        }
        if self.flying.is_none() {
            self.flying = base.flying;
        } else if let (Some(flying), Some(base_flying)) =
            (&mut self.flying, base.flying)
        {
            flying.merge_with_base(base_flying);
        }
        if self.jump.is_none() {
            self.jump = base.jump;
        } else if let (Some(jump), Some(base_jump)) = (&mut self.jump, base.jump)
        {
            jump.merge_with_base(base_jump);
        }
        if self.trap.is_none() {
            self.trap = base.trap;
        } else if let (Some(trap), Some(base_trap)) = (&mut self.trap, base.trap)
        {
            trap.merge_with_base(base_trap);
        }
        if self.death.is_none() {
            self.death.clone_from(&base.death);
        }
        if self.defense.is_none() {
            self.defense = base.defense;
        } else if let (Some(defense), Some(base_defense)) =
            (&mut self.defense, base.defense)
        {
            defense.merge_with_base(base_defense);
        }
    }
}

fn validate_base_archetype_stats(
    id: &str,
    stats: Option<&ArchetypeStats>,
) -> Result<(), AssetLoadError> {
    let Some(stats) = stats else {
        return Err(AssetLoadError::InvalidDefinition(format!(
            "Base archetype '{id}' must have stats defined"
        )));
    };

    if [stats.walk_speed, stats.chase_speed, stats.vision_range, stats.melee_range].iter().any(|value| !value.is_finite() || *value < 0.0) {
        return Err(AssetLoadError::InvalidDefinition(format!("{id}: speeds and ranges must be finite and nonnegative")));
    }

    if stats.spawn_hp == 0 {
        return Err(AssetLoadError::InvalidDefinition(format!(
            "Base archetype '{id}' must have spawn_hp > 0"
        )));
    }
    if stats.vision_range == 0.0 {
        return Err(AssetLoadError::InvalidDefinition(format!(
            "Base archetype '{id}' must have vision_range > 0"
        )));
    }
    if stats.walk_speed == 0.0 && stats.chase_speed == 0.0 {
        return Err(AssetLoadError::InvalidDefinition(format!(
            "Base archetype '{id}' must have at least one movement speed > 0"
        )));
    }

    Ok(())
}

fn validate_animation_asset_exists(
    preloaded: &theseeker_engine::assets::PreloadedAssets,
    anim_key: &str,
    error_message: String,
) -> Result<(), AssetLoadError> {
    if preloaded
        .get_single_asset::<theseeker_engine::assets::animation::SpriteAnimation>(anim_key)
        .is_some()
    {
        return Ok(());
    }

    Err(AssetLoadError::MissingAnimation(error_message))
}

fn apply_archetype_stat_overrides(
    stats: &mut Option<ArchetypeStats>,
    override_stats: &PartialArchetypeStats,
) {
    let Some(stats) = stats else {
        return;
    };

    apply_optional_override(&mut stats.spawn_hp, override_stats.spawn_hp);
    apply_optional_override(&mut stats.walk_speed, override_stats.walk_speed);
    apply_optional_override(&mut stats.chase_speed, override_stats.chase_speed);
    apply_optional_override(&mut stats.dmg_melee, override_stats.dmg_melee);
    apply_optional_override(&mut stats.dmg_ranged, override_stats.dmg_ranged);
    apply_optional_override(&mut stats.vision_range, override_stats.vision_range);
    apply_optional_override(&mut stats.melee_range, override_stats.melee_range);
    apply_optional_override(&mut stats.reaction_time, override_stats.reaction_time);
}

fn apply_optional_override<T: Copy>(target: &mut T, value: Option<T>) {
    if let Some(value) = value {
        *target = value;
    }
}

impl EnemyDefenseSetup {
    fn merge_with_base(&mut self, base: EnemyDefenseSetup) {
        if self.invulnerable_from_frame.is_none() {
            self.invulnerable_from_frame = base.invulnerable_from_frame;
        }
    }
}

impl EnemyFlyingSetup {
    fn merge_with_base(&mut self, base: EnemyFlyingSetup) {
        if self.profile != base.profile {
            return;
        }

        macro_rules! inherit {
            ($($field:ident),+ $(,)?) => {
                $(
                    if self.$field.is_none() {
                        self.$field = base.$field;
                    }
                )+
            };
        }

        inherit!(
            patrol_radius,
            idle_wait
        );

        match (&mut self.stalk, base.stalk) {
            (Some(stalk), Some(base_stalk)) => stalk.merge_with_base(base_stalk),
            (None, Some(base_stalk)) => self.stalk = Some(base_stalk),
            _ => {},
        }

        match (&mut self.attack, base.attack) {
            (Some(attack), Some(base_attack)) => {
                attack.merge_with_base(base_attack);
            },
            (None, Some(base_attack)) => self.attack = Some(base_attack),
            _ => {},
        }
    }
}

impl EnemyFlyingStalkSetup {
    fn merge_with_base(&mut self, base: EnemyFlyingStalkSetup) {
        if self.kind != base.kind {
            return;
        }

        macro_rules! inherit {
            ($($field:ident),+ $(,)?) => {
                $(if self.$field.is_none() { self.$field = base.$field; })+
            };
        }

        inherit!(
            trigger_distance,
            approach_ratio,
            target_distance,
            min_target_distance,
            x_tolerance,
            y_tolerance,
            windup_ticks
        );
    }
}

impl EnemyFlyingAttackSetup {
    fn merge_with_base(&mut self, base: EnemyFlyingAttackSetup) {
        if self.kind != base.kind {
            return;
        }

        macro_rules! inherit {
            ($($field:ident),+ $(,)?) => {
                $(if self.$field.is_none() { self.$field = base.$field; })+
            };
        }

        inherit!(
            speed_multiplier,
            collider_half_extents
        );
    }
}

impl EnemyJumpSetup {
    fn merge_with_base(&mut self, base: EnemyJumpSetup) {
        macro_rules! inherit {
            ($($field:ident),+ $(,)?) => {
                $(if self.$field.is_none() { self.$field = base.$field; })+
            };
        }

        inherit!(
            idle_ticks,
            jump_prep_ticks,
            jump_cancel_lock_ticks,
            horizontal_speed,
            fall_multiplier,
            anger
        );
    }
}

impl EnemyTrapSetup {
    fn merge_with_base(&mut self, base: EnemyTrapSetup) {
        macro_rules! inherit {
            ($($field:ident),+ $(,)?) => {
                $(if self.$field.is_none() { self.$field = base.$field; })+
            };
        }

        inherit!(
            body_offset_y,
            detection_half_extents,
            attack_extra_x,
            attack_height
        );
    }
}
