use crate::game::combat::{DamageSource, Hit};
use bevy::prelude::*;
use theseeker_engine::ballistics_math::solve_ballistic_arc;
use theseeker_engine::physics::{
    Collider, CollisionGroups as InteractionGroups, LinearVelocity,
    PhysicsWorld, ENEMY, GROUND, PLAYER_ATTACK,
};

/// Gravity for projectiles in px/tick^2 at 96Hz (independent of player fall accel)
const PROJECTILE_GRAVITY: f32 = 4.5;

/// Component for entities with ballistic physics (initial velocity in px/s)
#[derive(Component, Debug)]
pub struct Projectile {
    pub vel: LinearVelocity,
}

impl Projectile {
    /// Create a projectile with a ballistic trajectory toward the target.
    /// Gravity is in px/s² (e.g., 432 px/s² = 4.5 px/tick² at 96 Hz)
    pub fn with_vel(
        target: Vec2,
        start: Vec2,
        max_speed: f32,
        gravity: f32,
    ) -> Option<Self> {
        let result = solve_ballistic_arc(start, max_speed, target, gravity);
        if result.2 != 0 {
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

#[derive(Component)]
pub struct Arrow;

#[derive(Component)]
pub struct PreviousPosition(pub Vec2);

/// Applies gravity to projectiles (except player arrows)
pub fn arc_projectile(
    mut query: Query<
        (
            &mut Transform,
            &mut Projectile,
            Option<&mut PreviousPosition>,
            Has<Arrow>,
        ),
        With<DamageSource>,
    >,
    time: Res<theseeker_engine::time::GameTime>,
) {
    for (mut transform, mut projectile, prev_pos, arrow) in query.iter_mut() {
        if let Some(mut prev_pos) = prev_pos {
            prev_pos.0 = transform.translation.xy();
        }

        if !arrow {
            projectile.vel.0.y -= PROJECTILE_GRAVITY;
        }
        let z = transform.translation.z;
        transform.translation = (transform.translation.xy()
            + projectile.vel.0 * (1.0 / time.hz as f32))
            .extend(z);
    }
}

/// Continuous collision detection for arrows to prevent tunneling
pub fn arrow_continuous_collision_detection(
    mut arrow_query: Query<
        (
            Entity,
            &Transform,
            &PreviousPosition,
            &mut DamageSource,
            &Collider,
        ),
        With<Arrow>,
    >,
    spatial_query: PhysicsWorld,
    mut commands: Commands,
) {
    for (arrow_entity, arrow_transform, prev_pos, mut attack, arrow_collider) in
        arrow_query.iter_mut()
    {
        let current_pos = arrow_transform.translation.xy();
        let previous_pos = prev_pos.0;

        if current_pos.distance_squared(previous_pos) < 1.0 {
            continue;
        }

        let rapier_context = spatial_query.context();

        let direction = current_pos - previous_pos;
        let max_distance = direction.length();
        if max_distance < 0.001 {
            continue;
        }
        let normalized_dir = direction / max_distance;

        let wall_hit = rapier_context.cast_ray(
            previous_pos,
            normalized_dir,
            max_distance,
            true,
            bevy_rapier2d::prelude::QueryFilter::new().groups(
                InteractionGroups::new(PLAYER_ATTACK, GROUND),
            ),
        );

        let enemy_hit = rapier_context.cast_shape(
            previous_pos,
            0.0,
            normalized_dir,
            arrow_collider,
            bevy_rapier2d::prelude::ShapeCastOptions {
                max_time_of_impact: max_distance,
                ..Default::default()
            },
            bevy_rapier2d::prelude::QueryFilter::new()
                .groups(InteractionGroups::new(
                    PLAYER_ATTACK,
                    ENEMY,
                ))
                .exclude_collider(arrow_entity),
        );

        let wall_distance = wall_hit.map(|(_, toi)| toi).unwrap_or(f32::MAX);

        if let Some((hit_entity, hit_info)) = enemy_hit {
            if hit_info.time_of_impact < wall_distance {
                let total_targets =
                    attack.damaged_set.len() + attack.target_set.len();
                if total_targets < attack.max_targets as usize {
                    attack.target_set.insert(hit_entity);
                }
                continue;
            }
        }

        if wall_distance <= max_distance {
            commands.entity(arrow_entity).despawn();
        }
    }
}

pub fn despawn_projectile(
    query: Query<(Entity, &DamageSource), (With<Projectile>, With<Hit>)>,
    mut commands: Commands,
) {
    for (entity, damage_source) in query.iter() {
        if damage_source.damaged_set.len() == damage_source.max_targets as usize
        {
            commands.entity(entity).despawn();
        }
    }
}
