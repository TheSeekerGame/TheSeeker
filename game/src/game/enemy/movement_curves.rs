//! Per-tick velocity curves for enemy movement.
//!
//! All values are in pixels per tick at 96 Hz.
//! A value of 1.0 means the entity moves exactly 1 pixel during that tick.
//!
//! This module supports two movement modes:
//! 1. Tick-based: Movement progresses with a tick counter (default)
//! 2. Frame-based: Movement synchronized with animation frames (for precise animation sync)

// Per-tick fall displacements (negative = downward).
// Starts slow, accelerates to a capped terminal.
pub const ENEMY_FALL_VELOCITIES: &[f32] = &[
    -0.047, -0.094, -0.141, -0.188, -0.234, -0.281, -0.328, -0.375, -0.422,
    -0.469, -0.516, -0.563, -0.609, -0.656, -0.703, -0.750, -0.797, -0.844,
    -0.891, -0.938, -0.984, -1.031, -1.078, -1.125, -1.172, -1.219, -1.266,
    -1.313, -1.359, -1.406, -1.453, -1.500, -1.547, -1.594, -1.641, -1.688,
    -1.734, -1.781, -1.828, -1.875, -1.875, -1.875, -1.875, -1.875, -1.875,
    -1.875, -1.875, -1.875, -1.875, -1.875,
];

pub const SPIDER_SMALL_WALK_VELOCITIES: &[f32] = &[0.16];

pub const SPIDER_SMALL_CHASE_VELOCITIES: &[f32] = &[0.365];

// Big spider walk - uses frame-based movement instead of velocity curves
pub const SPIDER_BIG_WALK_VELOCITIES: &[f32] = &[0.0];

pub const SPIDER_BIG_CHASE_VELOCITIES: &[f32] =
    &[0.70, 0.75, 0.833, 0.80, 0.833];

pub const DEFAULT_WALK_VELOCITIES: &[f32] = &[0.0];

pub const DEFAULT_CHASE_VELOCITIES: &[f32] = &[0.365];

// Jump velocities for enemies that can jump (future extensibility).
// Currently unused.
#[allow(dead_code)]
pub const ENEMY_JUMP_VELOCITIES: &[f32] = &[
    1.2, 1.15, 1.1, 1.05, 1.0, 0.95, 0.9, 0.85, 0.8, 0.75, 0.7, 0.65, 0.6,
    0.55, 0.5, 0.45, 0.4, 0.35, 0.3, 0.25, 0.2, 0.15, 0.1, 0.05, 0.0,
];

pub const ENEMY_WALK_LOOPS: bool = true;
pub const ENEMY_CHASE_LOOPS: bool = true;
pub const ENEMY_FALL_LOOPS: bool = false;
#[allow(dead_code)] // Reserved for future jump behavior
pub const ENEMY_JUMP_LOOPS: bool = false;

pub fn get_curve_velocity(curve: &[f32], tick: u32, should_loop: bool) -> f32 {
    if should_loop && !curve.is_empty() {
        let index = (tick as usize) % curve.len();
        curve[index]
    } else {
        let index = (tick as usize).min(curve.len() - 1);
        curve[index]
    }
}

pub fn get_walk_curve(
    variant: crate::game::enemy::EnemyVariant,
) -> &'static [f32] {
    use crate::game::enemy::EnemyVariant;
    match variant {
        EnemyVariant::SmallSpider => SPIDER_SMALL_WALK_VELOCITIES,
        EnemyVariant::BigSpider => SPIDER_BIG_WALK_VELOCITIES,
        EnemyVariant::Default => DEFAULT_WALK_VELOCITIES,
    }
}

pub fn get_chase_curve(
    variant: crate::game::enemy::EnemyVariant,
) -> &'static [f32] {
    use crate::game::enemy::EnemyVariant;
    match variant {
        EnemyVariant::SmallSpider => SPIDER_SMALL_CHASE_VELOCITIES,
        EnemyVariant::BigSpider => SPIDER_BIG_CHASE_VELOCITIES,
        EnemyVariant::Default => DEFAULT_CHASE_VELOCITIES,
    }
}

/// Movement triggered by specific animation frames
#[derive(Debug, Clone)]
pub struct FrameBasedMovement {
    /// Which animation frames trigger movement (0-indexed)
    pub trigger_frames: &'static [u32],
    /// Velocity to apply when triggered (px/tick)
    pub velocity: f32,
    #[allow(dead_code)]
    pub ticks_per_frame: u32,
}

// Big spider frame-based walk movement:
// Animation has 10 frames. Leg pushes on frames 2,4,6,8,10 (1-indexed) = 1,3,5,7,9 (0-indexed).
// Movement is applied for exactly one tick on those frames.
pub const SPIDER_BIG_FRAME_WALK: FrameBasedMovement = FrameBasedMovement {
    trigger_frames: &[0, 1, 3, 5, 7, 9],
    velocity: 1.0,
    ticks_per_frame: 8,
};

pub fn should_use_frame_based(
    variant: crate::game::enemy::EnemyVariant,
    is_walking: bool,
) -> bool {
    use crate::game::enemy::EnemyVariant;
    matches!(variant, EnemyVariant::BigSpider) && is_walking
}

pub fn get_frame_based_movement(
    variant: crate::game::enemy::EnemyVariant,
) -> Option<&'static FrameBasedMovement> {
    use crate::game::enemy::EnemyVariant;
    match variant {
        EnemyVariant::BigSpider => Some(&SPIDER_BIG_FRAME_WALK),
        _ => None,
    }
}

/// Currently unused - we detect actual sprite frame changes instead
#[allow(dead_code)]
pub fn get_frame_based_velocity(
    movement: &FrameBasedMovement,
    anim_tick: u32,
) -> Option<f32> {
    let current_frame = anim_tick / movement.ticks_per_frame;

    let frame_tick = anim_tick % movement.ticks_per_frame;
    if frame_tick == 0 && movement.trigger_frames.contains(&current_frame) {
        Some(movement.velocity)
    } else {
        None
    }
}
