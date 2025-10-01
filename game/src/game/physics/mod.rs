use theseeker_engine::physics::LinearVelocity;

use super::enemy::Defense;
pub mod projectile;

use crate::prelude::*;

pub struct PhysicsPlugin;

impl Plugin for PhysicsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            GameTickUpdate,
            (
                knockback,
                projectile::arc_projectile,
                projectile::arrow_continuous_collision_detection,
                projectile::despawn_projectile,
            )
                .chain()
                .after(crate::game::combat::damage_source::apply_damage)
                .after(theseeker_engine::physics::update_sprite_colliders),
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
    mut query: Query<(
        Entity,
        &mut Knockback,
        &mut LinearVelocity,
        Has<Defense>,
    )>,
    mut commands: Commands,
) {
    for (entity, mut knockback, mut velocity, is_defending) in query.iter_mut()
    {
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
