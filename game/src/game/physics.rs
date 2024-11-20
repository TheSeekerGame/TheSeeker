use theseeker_engine::physics::LinearVelocity;

use crate::prelude::*;

use super::enemy::Defense;

/// Knockback that can be applied to a gent. Velocity is applied Once and then blocks horizontal movement.
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
    )>,
    mut commands: Commands,
) {
    for (entity, mut knockback, mut velocity) in query.iter_mut() {
        knockback.ticks += 1;
        if knockback.is_added() {
            **velocity = knockback.strength;
        }
        if knockback.ticks > knockback.max_ticks {
            velocity.x = 0.;
            commands.entity(entity).remove::<Knockback>();
        }
    }
}
