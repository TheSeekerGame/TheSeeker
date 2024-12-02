use theseeker_engine::ballistics_math::solve_ballistic_arc;
use theseeker_engine::physics::{Collider, LinearVelocity};

use crate::game::attack::Attack;
use crate::game::player::PlayerConfig;
use crate::prelude::*;

/// Attach this to an [`Attack`] entity to make it move with a fixed initial velocity
/// (in pixels/s) and despawn on collision. (The despawn on collision logic is
/// handled by the [`attack_damage`] system)
#[derive(Component, Debug)]
pub struct Projectile {
    pub vel: LinearVelocity,
}

impl Projectile {
    /// Creates a projectile component with a starting [`LinearVelocity`]
    /// of magnitude vel, such that the projectile will intersect target.
    ///
    /// set gravity to Playerconfig.fall_accel * time.hz
    /// since Playerconfig.fall_accel is in pixels/tick, you need to multiply by the time.hz
    /// to convert to per/second units, like velocity is.
    pub fn with_vel(
        target: Vec2,
        start: Vec2,
        max_speed: f32,
        gravity: f32,
    ) -> Option<Self> {
        let result = solve_ballistic_arc(start, max_speed, target, gravity);
        println!("result: {result:?}");
        if result.2 != 0 {
            // use the arc that has the bigger y component
            if result.0.y > result.1.y {
                Some(Self {
                    vel: LinearVelocity(result.0),
                })
            } else {
                Some(Self {
                    vel: LinearVelocity(result.1),
                })
            }
        } else {
            None
        }
    }
}

/// Applies gravity to the projectile
pub fn arc_projectile(
    mut query: Query<
        (
            &mut Transform,
            &Collider,
            &mut Projectile,
        ),
        With<Attack>,
    >,
    config: Res<PlayerConfig>,
    time: Res<GameTime>,
) {
    let fall_accel = config.fall_accel;
    for (mut transform, collider, mut projectile) in query.iter_mut() {
        projectile.vel.0.y -= fall_accel;
        let z = transform.translation.z;
        transform.translation = (transform.translation.xy()
            + *projectile.vel * (1.0 / time.hz as f32))
            .extend(z);
    }
}
