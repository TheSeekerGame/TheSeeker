use super::effects::frozen::Frozen;
use super::enemy::Defense;
pub mod projectile;
pub mod tile_collision_debug;

use crate::prelude::*;
use theseeker_engine::physics::LinearVelocity;

/// Shared per-tick gravity displacements (96 Hz). Negative values move downward.
pub const GRAVITY_VELOCITIES: &[f32] = &[
    0.0, 0.0, 0.0, 0.0, -0.047, -0.090, -0.132, -0.175, -0.217, -0.260, -0.302,
    -0.345, -0.387, -0.430, -0.472, -0.515, -0.557, -0.600, -0.642, -0.685,
    -0.727, -0.770, -0.812, -0.855, -0.897, -0.940, -0.982, -1.025, -1.067,
    -1.110, -1.152, -1.195, -1.237, -1.280, -1.322, -1.365, -1.407, -1.450,
    -1.492, -1.535, -1.577, -1.620, -1.662, -1.705, -1.705, -1.705, -1.705,
    -1.705,
];

/// Shared per-tick upward displacements for jumps (96 Hz).
pub const JUMP_VELOCITIES: &[f32] = &[
    0.0, 0.0, 1.515, 1.479, 1.342, 1.306, 1.369, 1.333, 1.296, 1.260, 1.223,
    1.257, 1.250, 1.214, 1.177, 1.141, 1.104, 1.068, 1.031, 0.995, 0.958,
    0.922, 0.885, 0.849, 0.812, 0.776, 0.739, 0.703, 0.666, 0.630, 0.593,
    0.557, 0.520, 0.484, 0.447, 0.411, 0.374, 0.338, 0.301, 0.265, 0.228,
    0.192, 0.155, 0.119, 0.082, 0.046, 0.010, 0.0, 0.0, 0.0, 0.0,
];

#[inline]
pub fn gravity_velocity(tick: u32) -> f32 {
    let idx = tick.min(GRAVITY_VELOCITIES.len().saturating_sub(1) as u32);
    GRAVITY_VELOCITIES[idx as usize]
}

#[inline]
pub fn jump_velocity(tick: u32) -> f32 {
    let idx = tick.min(JUMP_VELOCITIES.len().saturating_sub(1) as u32);
    JUMP_VELOCITIES[idx as usize]
}

pub struct PhysicsPlugin;

impl Plugin for PhysicsPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(tile_collision_debug::TileCollisionDebugPlugin::default().disabled());
        app.add_systems(
            GameTickUpdate,
            (
                knockback,
                projectile::arc_projectile,
                projectile::arc_projectile_ground_collision,
                projectile::grounded_projectile_despawn,
                projectile::arrow_continuous_collision_detection,
                projectile::despawn_projectile,
            )
                .chain()
                .after(crate::game::combat::damage_source::apply_damage)
                .after(theseeker_engine::physics::PhysicsSet),
        );
    }
}

/// Knockback applied to a gent. Initial velocity is set once, then horizontal movement is blocked until expiry.
#[derive(Component, Default, Clone, Copy, Debug)]
pub struct Knockback {
    pub ticks: u32,
    pub max_ticks: u32,
    pub strength: Vec2,
}

impl Knockback {
    pub fn new(strength: Vec2, max_ticks: u32) -> Self {
        Self {
            ticks: 0,
            max_ticks,
            strength,
        }
    }
}

pub fn knockback(
    mut query: Query<
        (Entity, &mut Knockback, &mut LinearVelocity, Has<Defense>),
        Without<Frozen>,
    >,
    mut commands: Commands,
) {
    for (entity, mut knockback, mut velocity, is_defending) in query.iter_mut() {
        knockback.ticks += 1;
        if knockback.is_added() && !is_defending {
            velocity.0 = knockback.strength;
        }
        if knockback.ticks > knockback.max_ticks {
            velocity.0.x = 0.;
            commands.entity(entity).remove::<Knockback>();
        }
    }
}
