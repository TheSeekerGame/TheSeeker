use bevy::prelude::*;
use std::collections::HashSet;

use crate::game::player::weapon::{PlayerMeleeWeapon, WeaponType};

// Organization: locomotion vs skill states
pub mod elevator;
pub mod locomotion;
pub mod skill;

// Shared movement tuning used by locomotion states (per-tick displacements)
pub const WALL_PUSHBACK: f32 = 0.729;
pub const WALL_PUSHBACK_TICKS: u32 = 24;

// Re-export state plugins
pub use elevator::ElevatorStatePlugin;
pub use locomotion::falling::FallingStatePlugin;
pub use locomotion::idle::IdleStatePlugin;
pub use locomotion::jumping::JumpingStatePlugin;
pub use locomotion::running::RunStatePlugin;
pub use skill::attacking::AttackingStatePlugin;
pub use skill::burning_dashing::BurningDashingStatePlugin;
pub use skill::dashing::DashingStatePlugin;
pub use skill::flicker_striking::FlickerStrikingStatePlugin;
pub use skill::gathering::GatheringStatePlugin;
pub use skill::whirling::WhirlingStatePlugin;

// Locomotion states (mutually exclusive)
#[derive(Component, Default, Debug)]
#[component(storage = "SparseSet")]
pub struct Idle;

#[derive(Component, Default, Debug)]
#[component(storage = "SparseSet")]
pub struct Running;

#[derive(Component, Debug)]
#[component(storage = "SparseSet")]
pub struct Jumping {
    pub tick: u32,
    pub jump_count: u8, // Which jump is this (1 for first jump, 2 for double jump)
}

impl Jumping {
    pub fn new() -> Self {
        Self {
            tick: 0,
            jump_count: 1, // First jump by default
        }
    }

    pub fn with_count(jump_count: u8) -> Self {
        Self {
            tick: 0,
            jump_count,
        }
    }
}

impl Default for Jumping {
    fn default() -> Self {
        Self::new()
    }
}

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

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum WallSide {
    #[default]
    Left,
    Right,
}

// Action states (mutually exclusive)
#[derive(Default, Debug)]
pub struct Ready;

#[derive(Debug, Clone, Copy)]
pub struct Elevator {
    shelter: Entity,
    direction: ElevatorDirection,
}

impl Elevator {
    pub fn new(shelter: Entity, direction: ElevatorDirection) -> Self {
        Self { shelter, direction }
    }

    pub fn shelter(self) -> Entity {
        self.shelter
    }

    pub fn direction(self) -> ElevatorDirection {
        self.direction
    }

    pub fn velocity(self) -> Vec2 {
        Vec2::Y * self.direction.sign() * ELEVATOR_SPEED
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ElevatorDirection {
    Up,
    Down,
}

impl ElevatorDirection {
    pub fn from_vertical_velocity(velocity: f32) -> Option<Self> {
        if velocity > 0.0 {
            Some(Self::Up)
        } else if velocity < 0.0 {
            Some(Self::Down)
        } else {
            None
        }
    }

    fn sign(self) -> f32 {
        match self {
            Self::Up => 1.0,
            Self::Down => -1.0,
        }
    }
}

const ELEVATOR_SPEED: f32 = 3.0;

#[derive(Debug, Default)]
pub struct Gathering {
    pub tick: u32,
}

#[derive(Debug)]
pub struct Attacking {
    pub tick: u32,
    pub variant: AttackVariant,
    pub weapon: WeaponType,
    /// Ticks the attack lasts, fixed by the requested swing.
    pub duration: u32,
}

impl Attacking {
    pub fn new(variant: AttackVariant, weapon: WeaponType) -> Self {
        Self {
            tick: 0,
            variant,
            weapon,
            duration: weapon.attack().swing(variant).state_ticks,
        }
    }
}

/// The direction of a basic attack.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AttackVariant {
    Forward,
    Up,
    Down,
}

#[derive(Debug)]
pub struct Dashing {
    pub tick: u32,
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
}

#[derive(Debug)]
pub struct Whirling {
    pub tick: u32,
    pub weapon: PlayerMeleeWeapon,
    pub damage_entity: Option<Entity>,
    pub slot_action: Option<crate::game::player::PlayerAction>, // Which slot action started this whirl
}

impl Whirling {
    pub fn new(weapon: PlayerMeleeWeapon) -> Self {
        Self {
            tick: 0,
            weapon,
            damage_entity: None,
            slot_action: None,
        }
    }
}

#[derive(Debug)]
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

#[derive(Debug, Clone, Copy)]
pub enum FlickerTarget {
    Enemy(Entity),
    Detour { position: Vec2, return_to: Entity },
}

#[derive(Debug)]
pub struct FlickerStriking {
    pub tick: u32,
    pub weapon: PlayerMeleeWeapon,
    pub current_target: Option<FlickerTarget>,
    pub last_target: Option<Entity>,
    pub damaged_entities: HashSet<Entity>,
    pub slot_action: Option<crate::game::player::PlayerAction>,
    pub phase: FlickerPhase,
    pub current_variant: FlickerVariant,
    pub damage_applied: bool, // Whether damage was applied for current target
    pub pending_outro: bool,  // If true, end after finishing current dash
    pub invulnerable_only_mode: bool,
}

impl FlickerStriking {
    pub fn new(weapon: PlayerMeleeWeapon) -> Self {
        Self {
            tick: 0,
            weapon,
            current_target: None,
            last_target: None,
            damaged_entities: HashSet::new(),
            slot_action: None,
            phase: FlickerPhase::Intro,
            current_variant: FlickerVariant::Forward,
            damage_applied: false,
            pending_outro: false,
            invulnerable_only_mode: false,
        }
    }
}

// Stance marker
#[derive(Component, Default, Debug)]
#[component(storage = "SparseSet")]
pub struct Grounded;

// Airborne combat classification, including foreground movement phases
// Added in Jumping/Falling and when a skill overrides locomotion; removed in Idle/Running
#[derive(Component, Default, Debug)]
#[component(storage = "SparseSet")]
pub struct InAir;

#[derive(Debug)]
pub struct DashStrike {
    pub dir: f32,
    pub action: Option<crate::game::actors::action::ActiveAction>,
}

impl Default for DashStrike {
    fn default() -> Self { Self { dir: 0.0, action: None } }
}

impl DashStrike {
    pub fn new() -> Self { Self::default() }
}

/// Spit skill state - player transforms into spider form and fires a projectile
#[derive(Debug)]
pub struct Spitting {
    pub facing: crate::game::gentstate::Facing,
    pub action: Option<crate::game::actors::action::ActiveAction>,
}

impl Spitting {
    pub fn new(facing: crate::game::gentstate::Facing) -> Self {
        Self { facing, action: None }
    }
}

/// Tracks the player's last non-zero horizontal move input direction (-1.0 or 1.0).
/// Used for movement skills to decide direction when input is neutral.
#[derive(Component, Debug, Default, Clone, Copy)]
pub struct LastMoveDir(pub f32);

/// Resolved horizontal input for the current tick.
///
/// `movement` drives locomotion and preserves the original held direction even when
/// the player presses the opposite key to manually reverse facing.
#[derive(Component, Debug, Default, Clone, Copy)]
pub struct HorizontalInputIntent {
    pub movement: f32,
    pub facing: Option<crate::game::gentstate::Facing>,
}


/// Locomotion components retain their own movement curves and entry systems.
pub trait PlayerLocomotionState: Component {}
impl PlayerLocomotionState for Idle {}
impl PlayerLocomotionState for Running {}
impl PlayerLocomotionState for Jumping {}
impl PlayerLocomotionState for Falling {}

pub fn transition_locomotion<T: PlayerLocomotionState>(
    commands: &mut Commands,
    entity: Entity,
    state: T,
) {
    commands.queue(move |world: &mut World| {
        let Ok(mut actor) = world.get_entity_mut(entity) else { return; };
        actor.remove::<(Idle, Running, Jumping, Falling)>();
        actor.insert(state);
    });
}

pub use super::foreground::transition_action;
