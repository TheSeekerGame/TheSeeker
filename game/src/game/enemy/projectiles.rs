use bevy::prelude::*;
use crate::game::physics::projectile::Projectile;
use theseeker_engine::physics::LinearVelocity;

/// Calculate a ballistic projectile solution for an enemy.
/// Returns a `Projectile` with velocity in px/tick derived from px/s inputs.
pub fn calculate_enemy_projectile(
    start: Vec2,
    target: Vec2,
    max_speed: f32,
    gravity: f32,
) -> Option<Projectile> {
    Projectile::with_vel(target, start, max_speed, gravity)
}