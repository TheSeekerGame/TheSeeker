use crate::game::gentstate::GentState;
use bevy::ecs::system::Command;
use bevy::ecs::world::EntityWorldMut;
use bevy::prelude::*;
use std::collections::HashSet;
use theseeker_engine::effects::{
    FadeCurve, GhostColorMode, GhostMovement, GhostingSource, ScaleCurve,
};

use crate::game::player::skills::channeled::ChanneledSkill;
use crate::game::player::skills::flicker_strike::FlickerOutroTransition;
use crate::game::player::skills::types::{SkillId, SkillWeaponKind};
use crate::game::effects::stealthed::StealthEffect;

// Organization: locomotion vs skill states
pub mod locomotion;
pub mod skill;

// Shared movement tuning used by locomotion states (per-tick displacements)
pub const WALL_PUSHBACK: f32 = 0.729;
pub const WALL_PUSHBACK_TICKS: u32 = 24;
#[allow(dead_code)]
pub const MAX_COYOTE_TIME: f32 = 0.1;

// Re-export state plugins
pub use locomotion::falling::FallingStatePlugin;
pub use locomotion::idle::IdleStatePlugin;
pub use locomotion::jumping::JumpingStatePlugin;
pub use locomotion::running::RunStatePlugin;
pub use skill::attacking::AttackingStatePlugin;
pub use skill::burning_dashing::BurningDashingStatePlugin;
pub use skill::dashing::DashingStatePlugin;
pub use skill::flicker_striking::FlickerStrikingStatePlugin;
pub use skill::ready::ReadyStatePlugin;
pub use skill::whirling::WhirlingStatePlugin;

#[derive(Clone, Copy)]
pub struct StateMetadata {
    pub name: &'static str,
    pub remove: fn(&mut EntityWorldMut) -> bool,
    pub on_enter: Option<fn(&mut World, Entity)>,
    pub on_exit: Option<fn(&mut World, Entity)>,
}

#[derive(Clone, Copy)]
pub struct ActionStateMetadata {
    pub state: StateMetadata,
    pub overrides_locomotion: bool,
}

pub trait PlayerLocomotionState: Component {
    fn metadata() -> &'static StateMetadata;
}

pub trait PlayerActionState: Component {
    fn metadata() -> &'static ActionStateMetadata;
}

fn remove_component_if_present<T: Component>(entity: &mut EntityWorldMut) -> bool {
    if entity.contains::<T>() {
        entity.remove::<T>();
        true
    } else {
        false
    }
}

fn cleanup_dash(world: &mut World, entity: Entity) {
    if let Ok(mut entity_mut) = world.get_entity_mut(entity) {
        entity_mut.remove::<InAir>();
        entity_mut.remove::<GhostingSource>();
    }
}

fn cleanup_dash_strike(world: &mut World, entity: Entity) {
    if let Ok(mut entity_mut) = world.get_entity_mut(entity) {
        entity_mut.remove::<InAir>();
        entity_mut.remove::<GhostingSource>();
    }
}

fn cleanup_burning_dash(world: &mut World, entity: Entity) {
    if let Ok(mut entity_mut) = world.get_entity_mut(entity) {
        entity_mut.remove::<InAir>();
        entity_mut.remove::<GhostingSource>();
        entity_mut.remove::<ChanneledSkill>();
    }
}

fn cleanup_flicker_strike(world: &mut World, entity: Entity) {
    if let Ok(mut entity_mut) = world.get_entity_mut(entity) {
        entity_mut.remove::<InAir>();
        entity_mut.remove::<GhostingSource>();
        entity_mut.remove::<ChanneledSkill>();
        entity_mut.remove::<FlickerOutroTransition>();
    }
}

fn cleanup_whirling(world: &mut World, entity: Entity) {
    if let Ok(mut entity_mut) = world.get_entity_mut(entity) {
        entity_mut.remove::<ChanneledSkill>();
    }
}

struct RunCleanupHook {
    entity: Entity,
    func: fn(&mut World, Entity),
}

impl Command for RunCleanupHook {
    fn apply(self, world: &mut World) {
        (self.func)(world, self.entity);
    }
}

pub(super) fn queue_state_cleanup(
    commands: &mut Commands,
    entity: Entity,
    func: fn(&mut World, Entity),
) {
    commands.queue(RunCleanupHook { entity, func });
}

fn apply_dash_fx(world: &mut World, entity: Entity) {
    if let Ok(mut entity_mut) = world.get_entity_mut(entity) {
        let max_ticks = entity_mut
            .get::<Dashing>()
            .map(|state| state.max_ticks)
            .unwrap_or(0);
        let ghost_lifetime_ticks = (max_ticks * 5).max(29);
        let initial_alpha = if entity_mut.contains::<StealthEffect>() {
            0.2
        } else {
            0.5
        };

        entity_mut.insert(InAir);
        entity_mut.insert(GhostingSource {
            spawn_interval_ticks: 1,
            ghost_lifetime_ticks,
            initial_alpha,
            fade_curve: FadeCurve::Linear,
            color_mode: GhostColorMode::Original,
            scale_over_time: ScaleCurve::Constant,
            offset: Vec2::ZERO,
            movement: GhostMovement::Static,
            ticks_since_last_spawn: 0,
        });
    }
}

fn apply_dash_strike_fx(world: &mut World, entity: Entity) {
    if let Ok(mut entity_mut) = world.get_entity_mut(entity) {
        let max_ticks = entity_mut
            .get::<DashStrike>()
            .map(|state| state.max_ticks)
            .unwrap_or(0);
        let ghost_lifetime_ticks = (max_ticks * 5).max(29);
        let initial_alpha = if entity_mut.contains::<StealthEffect>() {
            0.2
        } else {
            0.5
        };

        entity_mut.insert(InAir);
        entity_mut.insert(GhostingSource {
            spawn_interval_ticks: 1,
            ghost_lifetime_ticks,
            initial_alpha,
            fade_curve: FadeCurve::Linear,
            color_mode: GhostColorMode::Original,
            scale_over_time: ScaleCurve::Constant,
            offset: Vec2::ZERO,
            movement: GhostMovement::Static,
            ticks_since_last_spawn: 0,
        });
    }
}

fn apply_burning_dash_fx(world: &mut World, entity: Entity) {
    if let Ok(mut entity_mut) = world.get_entity_mut(entity) {
        entity_mut.insert(ChanneledSkill::new(SkillId::BurningDash));
        entity_mut.insert(InAir);
        entity_mut.insert(GhostingSource {
            spawn_interval_ticks: 8,
            ghost_lifetime_ticks: 16,
            initial_alpha: 0.5,
            fade_curve: FadeCurve::Linear,
            color_mode: GhostColorMode::Original,
            scale_over_time: ScaleCurve::Constant,
            offset: Vec2::ZERO,
            movement: GhostMovement::Static,
            ticks_since_last_spawn: 0,
        });
    }
}

fn apply_flicker_strike_fx(world: &mut World, entity: Entity) {
    if let Ok(mut entity_mut) = world.get_entity_mut(entity) {
        entity_mut.insert(ChanneledSkill::new(SkillId::FlickerStrike));
        entity_mut.insert(InAir);
        entity_mut.insert(GhostingSource {
            spawn_interval_ticks: 0,
            ghost_lifetime_ticks: 100,
            initial_alpha: 0.5,
            fade_curve: FadeCurve::Linear,
            color_mode: GhostColorMode::Original,
            scale_over_time: ScaleCurve::Constant,
            offset: Vec2::ZERO,
            movement: GhostMovement::Static,
            ticks_since_last_spawn: 0,
        });
    }
}

fn apply_whirling_fx(world: &mut World, entity: Entity) {
    if let Ok(mut entity_mut) = world.get_entity_mut(entity) {
        entity_mut.insert(ChanneledSkill::new(SkillId::Whirl));
    }
}

macro_rules! define_locomotion_meta {
    ( $( ($ty:ty, $const:ident, $name:literal, $on_enter:expr, $on_exit:expr) ),+ $(,)? ) => {
        $(
            const $const: StateMetadata = StateMetadata {
                name: $name,
                remove: remove_component_if_present::<$ty>,
                on_enter: $on_enter,
                on_exit: $on_exit,
            };

            impl PlayerLocomotionState for $ty {
                fn metadata() -> &'static StateMetadata {
                    &$const
                }
            }
        )+

        const PLAYER_LOCOMOTION_STATE_REGISTRY: &[StateMetadata] = &[
            $( $const ),+
        ];
    };
}

macro_rules! define_action_meta {
    ( $( ($ty:ty, $const:ident, $name:literal, $overrides:expr, $on_enter:expr, $on_exit:expr) ),+ $(,)? ) => {
        $(
            const $const: ActionStateMetadata = ActionStateMetadata {
                state: StateMetadata {
                    name: $name,
                    remove: remove_component_if_present::<$ty>,
                    on_enter: $on_enter,
                    on_exit: $on_exit,
                },
                overrides_locomotion: $overrides,
            };

            impl PlayerActionState for $ty {
                fn metadata() -> &'static ActionStateMetadata {
                    &$const
                }
            }
        )+

        const PLAYER_ACTION_STATE_REGISTRY: &[ActionStateMetadata] = &[
            $( $const ),+
        ];
    };
}

// Locomotion states (mutually exclusive)
#[derive(Component, Default, Debug)]
#[component(storage = "SparseSet")]
pub struct Idle;
impl GentState for Idle {}

#[derive(Component, Default, Debug)]
#[component(storage = "SparseSet")]
pub struct Running;
impl GentState for Running {}

#[derive(Component, Debug)]
#[component(storage = "SparseSet")]
pub struct Jumping {
    pub tick: u32,
    pub release_tick: Option<u32>,
    pub jump_count: u8, // Which jump is this (1 for first jump, 2 for double jump)
}

impl Jumping {
    pub fn new() -> Self {
        Self {
            tick: 0,
            release_tick: None,
            jump_count: 1, // First jump by default
        }
    }

    pub fn with_count(jump_count: u8) -> Self {
        Self {
            tick: 0,
            release_tick: None,
            jump_count,
        }
    }
}

impl Default for Jumping {
    fn default() -> Self {
        Self::new()
    }
}

impl GentState for Jumping {}

#[derive(Component, Debug)]
#[component(storage = "SparseSet")]
pub struct Falling {
    pub coyote_ticks: u32,
    pub from_ground: bool, // true if fell from ground (gets coyote time), false if from jump
    pub jump_count: u8,    // Number of jumps used (for double/triple jump)
    pub wall_slide: Option<WallSide>, // Some when wall sliding, None when falling normally
    pub fall_ticks: u32, // Total ticks spent falling (for velocity curve)
}

impl Falling {
    pub fn new() -> Self {
        Self {
            coyote_ticks: 0,
            from_ground: true, // Default assumes falling from ground
            jump_count: 0,     // Reset jump count when falling from ground
            wall_slide: None,  // Not wall sliding by default
            fall_ticks: 0,     // Start fall tick counter
        }
    }

    pub fn from_jump(jump_count: u8) -> Self {
        Self {
            coyote_ticks: 0,
            from_ground: false, // No coyote time when falling from jump
            jump_count,         // Preserve jump count for double jump
            wall_slide: None,   // Not wall sliding by default
            fall_ticks: 0,      // Start fall tick counter
        }
    }
}

impl Default for Falling {
    fn default() -> Self {
        Self::new()
    }
}

impl GentState for Falling {}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum WallSide {
    #[default]
    Left,
    Right,
}

// Action states (mutually exclusive)
#[derive(Component, Default, Debug)]
#[component(storage = "SparseSet")]
pub struct Ready;
impl GentState for Ready {}

#[derive(Component, Debug)]
#[component(storage = "SparseSet")]
pub struct Attacking {
    pub tick: u32,
    pub variant: AttackVariant,
    pub weapon_type: WeaponType,
    pub followup: bool,
    pub auto_aim_flipped: bool, // Track if auto-aim flipped the facing
    pub max_ticks: Option<u32>, // Optional lifetime provided by skills
}

impl Default for Attacking {
    fn default() -> Self {
        Self {
            tick: 0,
            variant: AttackVariant::Forward,
            weapon_type: WeaponType::Sword,
            followup: false,
            auto_aim_flipped: false,
            max_ticks: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AttackVariant {
    Forward,
    Up,
    Down,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WeaponType {
    Sword,
    Hammer,
    Bow,
}

impl WeaponType {
    pub fn is_melee(&self) -> bool {
        matches!(
            self,
            WeaponType::Sword | WeaponType::Hammer
        )
    }
}

impl From<SkillWeaponKind> for WeaponType {
    fn from(value: SkillWeaponKind) -> Self {
        match value {
            SkillWeaponKind::Sword => WeaponType::Sword,
            SkillWeaponKind::Hammer => WeaponType::Hammer,
            SkillWeaponKind::Bow => WeaponType::Bow,
        }
    }
}

impl From<WeaponType> for SkillWeaponKind {
    fn from(value: WeaponType) -> Self {
        match value {
            WeaponType::Sword => SkillWeaponKind::Sword,
            WeaponType::Hammer => SkillWeaponKind::Hammer,
            WeaponType::Bow => SkillWeaponKind::Bow,
        }
    }
}

impl Attacking {
    pub fn new(variant: AttackVariant, weapon_type: WeaponType) -> Self {
        Self {
            tick: 0,
            variant,
            weapon_type,
            followup: false,
            auto_aim_flipped: false,
            max_ticks: None,
        }
    }

    pub fn with_max_ticks(mut self, max_ticks: u32) -> Self {
        self.max_ticks = Some(max_ticks);
        self
    }
}

impl GentState for Attacking {}

#[derive(Component, Debug)]
#[component(storage = "SparseSet")]
pub struct Dashing {
    pub tick: u32,
    pub duration: f32,
    pub hit: bool,
    pub hit_ground: bool,
    /// Horizontal direction chosen at dash start (-1.0 or 1.0)
    pub dir: f32,
    /// Lifetime in ticks provided by skills; dash ends when `tick >= max_ticks`
    pub max_ticks: u32,
    /// Speed modifier snapshot from PlayerStatMod at dash initiation
    pub speed_mod: f32,
}

impl Default for Dashing {
    fn default() -> Self {
        Self {
            tick: 0,
            duration: 0.0,
            hit: false,
            hit_ground: false,
            dir: 0.0,
            max_ticks: 0,
            speed_mod: 1.0,
        }
    }
}

impl Dashing {
    pub fn new() -> Self {
        Self {
            tick: 0,
            duration: 0.0,
            hit: false,
            hit_ground: false,
            dir: 0.0,
            max_ticks: 0,
            speed_mod: 1.0,
        }
    }

    pub fn with_max_ticks(mut self, max_ticks: u32) -> Self {
        self.max_ticks = max_ticks;
        self
    }

    pub fn with_speed_mod(mut self, speed_mod: f32) -> Self {
        self.speed_mod = speed_mod;
        self
    }

    /// Dash duration in seconds (based on `max_ticks` @ 96 Hz)
    pub fn dash_duration(&self) -> f32 {
        self.max_ticks as f32 / 96.0
    }
}

impl GentState for Dashing {}

#[derive(Component, Debug)]
#[component(storage = "SparseSet")]
pub struct Whirling {
    pub tick: u32,
    pub weapon_type: WeaponType,
    pub damage_entity: Option<Entity>,
    pub last_damage_frame: u32,
    pub slot_action: Option<crate::game::player::PlayerAction>, // Which slot action started this whirl
}

impl Default for Whirling {
    fn default() -> Self {
        Self {
            tick: 0,
            weapon_type: WeaponType::Sword,
            damage_entity: None,
            last_damage_frame: 0,
            slot_action: None,
        }
    }
}

impl Whirling {
    pub fn new(weapon_type: WeaponType) -> Self {
        Self {
            tick: 0,
            weapon_type,
            damage_entity: None,
            last_damage_frame: 0,
            slot_action: None,
        }
    }
}

impl GentState for Whirling {}

#[derive(Component, Debug)]
#[component(storage = "SparseSet")]
pub struct BurningDashing {
    pub tick: u32,
    pub damage_entity: Option<Entity>,
    pub slot_action: Option<crate::game::player::PlayerAction>,
    pub speed_mod: f32,
}

impl Default for BurningDashing {
    fn default() -> Self {
        Self {
            tick: 0,
            damage_entity: None,
            slot_action: None,
            speed_mod: 1.0,
        }
    }
}

impl BurningDashing {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_speed_mod(mut self, speed_mod: f32) -> Self {
        self.speed_mod = speed_mod;
        self
    }
}

impl GentState for BurningDashing {}

// Flicker Strike skill state - rapid dashing between enemies
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlickerPhase {
    Intro,   // Playing intro frames
    Dashing, // Moving toward target
    Damage,  // At target, dealing damage
    Outro,   // Playing outro frames
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlickerVariant {
    Forward,
    Upward,
    Downward,
    FrontUpward,
    FrontDownward,
}

#[derive(Component, Debug)]
#[component(storage = "SparseSet")]
pub struct FlickerStriking {
    pub tick: u32,
    pub weapon_type: WeaponType,
    pub current_target: Option<Entity>,
    pub last_target: Option<Entity>,
    pub damaged_entities: HashSet<Entity>,
    pub slot_action: Option<crate::game::player::PlayerAction>,
    pub phase: FlickerPhase,
    pub current_variant: FlickerVariant,
    pub ticks_per_frame: u32,    // 6 or 8
    pub phase_start_tick: u32,   // When current phase started
    pub damage_applied: bool, // Whether damage was applied for current target
    pub pending_outro: bool,  // If true, end after finishing current dash
    pub defense_only_mode: bool, // True if we fell back to Defense-only targeting
}

impl FlickerStriking {
    pub fn new(weapon_type: WeaponType) -> Self {
        Self {
            tick: 0,
            weapon_type,
            current_target: None,
            last_target: None,
            damaged_entities: HashSet::new(),
            slot_action: None,
            phase: FlickerPhase::Intro,
            current_variant: FlickerVariant::Forward,
            ticks_per_frame: 8,
            phase_start_tick: 0,
            damage_applied: false,
            pending_outro: false,
            defense_only_mode: false,
        }
    }

    pub fn with_ticks_per_frame(mut self, tpf: u32) -> Self {
        self.ticks_per_frame = tpf;
        self
    }
}

impl GentState for FlickerStriking {}

// Stance marker
#[derive(Component, Default, Debug)]
#[component(storage = "SparseSet")]
pub struct Grounded;
impl GentState for Grounded {}

// InAir marker reflects actual airborne state
// Added in Jumping/Falling and when a skill overrides locomotion; removed in Idle/Running
#[derive(Component, Default, Debug)]
#[component(storage = "SparseSet")]
pub struct InAir;
impl GentState for InAir {}

// Additional states retained for compatibility during refactor

#[derive(Component, Debug)]
#[component(storage = "SparseSet")]
pub struct DashStrike {
    pub tick: u32,
    pub max_ticks: u32,
    pub variant: crate::game::player::skills::types::Variant,
    /// Horizontal direction chosen at start (-1.0 or 1.0)
    pub dir: f32,
    /// True once we collide with ground/wall/ceiling according to variant rules
    pub collided: bool,
    /// Counts ticks of the post-collision lock (outro); while < 8, no transitions allowed
    pub lock_ticks: u32,
    /// Track which enemies were damaged during this strike to prevent multiple hits
    pub damaged_entities: std::collections::HashSet<Entity>,
}
impl Default for DashStrike {
    fn default() -> Self {
        Self {
            tick: 0,
            max_ticks: 16,
            variant: crate::game::player::skills::types::Variant::Down,
            dir: 0.0,
            collided: false,
            lock_ticks: 0,
            damaged_entities: Default::default(),
        }
    }
}
impl DashStrike {
    pub fn new(variant: crate::game::player::skills::types::Variant) -> Self {
        Self {
            variant,
            ..Default::default()
        }
    }
}
impl GentState for DashStrike {}

/// Tracks the player's last non-zero horizontal move input direction (-1.0 or 1.0).
/// Not affected by auto-aim; used for movement skills to decide direction when input is neutral.
#[derive(Component, Debug, Default, Clone, Copy)]
pub struct LastMoveDir(pub f32);

/// Marker component indicating a skill state that fully overrides locomotion control.
/// When present, locomotion states should skip movement input processing and state transitions.
#[derive(Component, Default, Debug)]
#[component(storage = "SparseSet")]
pub struct OverridesLocomotion;

define_locomotion_meta!(
    (Idle, IDLE_LOCOMOTION_META, "Idle", None, None),
    (Running, RUNNING_LOCOMOTION_META, "Running", None, None),
    (Jumping, JUMPING_LOCOMOTION_META, "Jumping", None, None),
    (Falling, FALLING_LOCOMOTION_META, "Falling", None, None),
);

define_action_meta!(
    (Ready, READY_ACTION_META, "Ready", false, None, None),
    (Attacking, ATTACKING_ACTION_META, "Attacking", false, None, None),
    (Dashing, DASHING_ACTION_META, "Dashing", true, Some(apply_dash_fx), Some(cleanup_dash)),
    (Whirling, WHIRLING_ACTION_META, "Whirling", false, Some(apply_whirling_fx), Some(cleanup_whirling)),
    (BurningDashing, BURNING_DASHING_ACTION_META, "BurningDashing", true, Some(apply_burning_dash_fx), Some(cleanup_burning_dash)),
    (FlickerStriking, FLICKER_STRIKING_ACTION_META, "FlickerStriking", true, Some(apply_flicker_strike_fx), Some(cleanup_flicker_strike)),
    (DashStrike, DASH_STRIKE_ACTION_META, "DashStrike", true, Some(apply_dash_strike_fx), Some(cleanup_dash_strike)),
);

struct TransitionLocomotionCommand<T: Component + PlayerLocomotionState> {
    entity: Entity,
    new_state: T,
    new_meta: StateMetadata,
}

impl<T> Command for TransitionLocomotionCommand<T>
where
    T: Component + PlayerLocomotionState,
{
    fn apply(self, world: &mut World) {
        let entity_id = self.entity;
        let Ok(mut entity_mut) = world.get_entity_mut(entity_id) else {
            return;
        };

        let mut removed_meta: Vec<StateMetadata> = Vec::new();
        for &meta in PLAYER_LOCOMOTION_STATE_REGISTRY {
            if (meta.remove)(&mut entity_mut) {
                removed_meta.push(meta);
            }
        }

        entity_mut.insert(self.new_state);

        let new_meta = self.new_meta;
        let removed_meta = removed_meta;
        entity_mut.world_scope(move |world| {
            for meta in removed_meta {
                if let Some(on_exit) = meta.on_exit {
                    on_exit(world, entity_id);
                }
            }

            if let Some(on_enter) = new_meta.on_enter {
                on_enter(world, entity_id);
            }
        });
    }
}

struct TransitionActionCommand<T: Component + PlayerActionState + std::fmt::Debug> {
    entity: Entity,
    new_state: T,
    new_meta: ActionStateMetadata,
}

impl<T> Command for TransitionActionCommand<T>
where
    T: Component + PlayerActionState + std::fmt::Debug,
{
    fn apply(self, world: &mut World) {
        let entity_id = self.entity;
        let Ok(mut entity_mut) = world.get_entity_mut(entity_id) else {
            return;
        };

        let mut removed_meta: Vec<ActionStateMetadata> = Vec::new();
        for &meta in PLAYER_ACTION_STATE_REGISTRY {
            if (meta.state.remove)(&mut entity_mut) {
                removed_meta.push(meta);
            }
        }

        entity_mut.remove::<OverridesLocomotion>();
        entity_mut.insert(self.new_state);

        if self.new_meta.overrides_locomotion {
            entity_mut.insert(OverridesLocomotion);
        }

        let new_meta = self.new_meta;
        let removed_meta = removed_meta;
        entity_mut.world_scope(move |world| {
            for meta in removed_meta {
                if let Some(on_exit) = meta.state.on_exit {
                    on_exit(world, entity_id);
                }
            }

            if let Some(on_enter) = new_meta.state.on_enter {
                on_enter(world, entity_id);
            }
        });
    }
}

/// Helper to transition between locomotion states
pub fn transition_locomotion<T: Component + PlayerLocomotionState>(
    commands: &mut Commands,
    entity: Entity,
    new_state: T,
) {
    let new_meta = T::metadata();
    debug_assert!(
        PLAYER_LOCOMOTION_STATE_REGISTRY
            .iter()
            .any(|meta| meta.name == new_meta.name),
        "Player locomotion state '{}' is not registered",
        new_meta.name
    );

    commands.queue(TransitionLocomotionCommand {
        entity,
        new_state,
        new_meta: *new_meta,
    });
}

/// Helper to transition between action states
pub fn transition_action<T: Component + std::fmt::Debug + PlayerActionState>(
    commands: &mut Commands,
    entity: Entity,
    new_state: T,
) {
    let new_meta = T::metadata();
    debug_assert!(
        PLAYER_ACTION_STATE_REGISTRY
            .iter()
            .any(|meta| meta.state.name == new_meta.state.name),
        "Player action state '{}' is not registered",
        new_meta.state.name
    );

    commands.queue(TransitionActionCommand {
        entity,
        new_state,
        new_meta: *new_meta,
    });
}
