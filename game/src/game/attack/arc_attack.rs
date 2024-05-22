use theseeker_engine::physics::{Collider, LinearVelocity, PhysicsWorld};

use crate::game::attack::Attack;
use crate::game::player::PlayerConfig;
use crate::prelude::*;

/// Attach this to an [`Attack`] entity to make it move with a fixed initial velocity
/// (in pixels/s) and despawn on collision
#[derive(Component, Debug)]
pub struct Projectile {
    pub vel: LinearVelocity,
}

impl Projectile {
    /// Creates a projectile component with a starting [`LinearVelocity`]
    /// of magnitude vel, such that the projectile will intersect target.
    /// Use Playerconfig.fall_accel for gravity
    pub fn with_vel(target: Vec2, start: Vec2, angle: f32, gravity: f32) -> Self {
        let diff = target - start;

        Some(Self {
            vel: LinearVelocity(Vec2::new(vx, vy)),
        })
    }
}

pub fn arc_projectile(
    spatial_query: Res<PhysicsWorld>,
    mut query: Query<
        (
            &mut Transform,
            &Collider,
            &mut Projectile,
        ),
        With<Attack>,
    >,
    mut commands: Commands,
    config: Res<PlayerConfig>,
    time: Res<GameTime>,
) {
    let fall_accel = config.fall_accel;
    for (mut transform, collider, mut projectile) in query.iter_mut() {
        projectile.vel.0.y -= fall_accel;
        let z = transform.translation.z;
        transform.translation =
            (transform.translation.xy() + *projectile.vel * (1.0 / time.hz as f32)).extend(z);
    }
}
