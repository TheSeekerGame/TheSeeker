use theseeker_engine::ballistics_math::solve_ballistic_arc;
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
    pub fn with_vel(target: Vec2, start: Vec2, max_speed: f32, gravity: f32) -> Option<Self> {
        let diff = target - start;
        let result = solve_ballistic_arc(
            start,
            max_speed,
            target,
            Vec2::new(0.0, 0.0),
            gravity * 60.0,
        );
        println!("{result:?}");
        if result.2 != 0 {
            Some(Self {
                vel: LinearVelocity(result.0),
            })
        } else {
            warn!("no trajectory found!");
            None
        }
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
