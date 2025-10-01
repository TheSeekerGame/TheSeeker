//! Player sensors updated at 96 Hz.
//!
//! Provides ground/wall/ceiling contact data and enemy proximity hints for
//! state machines. Uses Rapier queries only for detection (no physics forces).
use crate::game::enemy::Enemy;
use crate::game::gentstate::Facing;
use crate::game::player::states::{Falling, Jumping};
use crate::game::player::{JumpCount, Player, PlayerStateSet};
use bevy::prelude::*;
use bevy_rapier2d::prelude::{QueryFilter, ShapeCastOptions};
use theseeker_engine::physics::CollisionGroups as InteractionGroups;
use theseeker_engine::physics::{
    Collider, CollisionGroups, LinearVelocity, PhysicsWorld, ShapeCaster,
    ENEMY, GROUND, GROUNDED_THRESHOLD, PLAYER,
};
use theseeker_engine::time::{GameTickUpdate, GameTime};

pub struct SensorPlugin;

impl Plugin for SensorPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            GameTickUpdate,
            (
                update_ground_sensor,
                update_wall_sensor,
                update_ceiling_sensor,
                update_enemy_proximity_sensor,
                reset_jump_count_on_wall_touch,
            )
                .chain()
                .in_set(PlayerStateSet::Sensors),
        );
    }
}

#[derive(Component, Default, Debug)]
pub struct GroundSensor {
    pub is_grounded: bool,
    pub distance: f32,
}

#[derive(Component, Default, Debug)]
pub struct WallSensor {
    pub left_contact: bool,
    pub right_contact: bool,
    pub left_distance: f32,
    pub right_distance: f32,
}

#[derive(Component, Default, Debug)]
pub struct EnemyProximitySensor {
    pub enemies_in_melee_range: Vec<Entity>,
    pub enemies_in_ranged_range: Vec<Entity>,
    pub closest_enemy: Option<Entity>,
    pub closest_distance: f32,
    /// True if an enemy is directly in front of the player within a small forward window
    pub has_forward_enemy: bool,
    /// True if any enemy is detected directly below the player within a vertical sweep
    pub has_enemy_below: bool,
    /// True if an enemy collider is immediately blocking movement in facing direction
    pub has_blocking_enemy_ahead: bool,
}

#[derive(Component, Default, Debug)]
pub struct CeilingSensor {
    pub is_touching_ceiling: bool,
    pub distance: f32,
}

#[derive(Component, Default, Debug)]
pub struct LastGroundedPosition {
    pub position: Vec2,
}

const WALL_CAST_DISTANCE: f32 = 8.0;
const CEILING_CAST_DISTANCE: f32 = 8.0;
const MELEE_RANGE: f32 = 32.0;
const RANGED_RANGE: f32 = 200.0;
const MAX_GROUND_CHECK_DISTANCE: f32 = 50.0; // Cap for distance when no hit is found

fn update_ground_sensor(
    mut query: Query<
        (
            Entity,
            &Transform,
            &mut GroundSensor,
            &ShapeCaster,
            &LinearVelocity,
            Option<&mut LastGroundedPosition>,
            Has<crate::game::player::states::InAir>,
        ),
        With<Player>,
    >,
    spatial_query: PhysicsWorld,
    _time: Res<GameTime>,
    mut commands: Commands,
) {
    for (
        entity,
        transform,
        mut sensor,
        shape_caster,
        velocity,
        mut last_grounded_pos,
        is_in_air,
    ) in query.iter_mut()
    {
        // Detect ground via ShapeCaster under the player
        if let Some((_, toi)) = shape_caster.cast(
            &spatial_query.context(),
            &transform,
            Some(entity),
        ) {
            sensor.distance = toi.time_of_impact;

            // Predict imminent ground only when falling fast; conservative threshold
            if velocity.0.y < -0.52 {
                // ~50 px/s at 96 Hz
                // Predict landing
                let predicted_distance = toi.time_of_impact + velocity.0.y; // per-tick displacement
                sensor.is_grounded = predicted_distance < GROUNDED_THRESHOLD;
            } else {
                // Distance check with a small buffer to prevent flicker near threshold
                sensor.is_grounded =
                    toi.time_of_impact < GROUNDED_THRESHOLD + 0.1;
            }
        } else {
            sensor.is_grounded = false;
            sensor.distance = MAX_GROUND_CHECK_DISTANCE;
        }

        // Update last grounded position when NOT in air (more accurate than prediction)
        if !is_in_air {
            if let Some(ref mut last_pos) = last_grounded_pos {
                last_pos.position = transform.translation.truncate();
            } else {
                commands.entity(entity).insert(LastGroundedPosition {
                    position: transform.translation.truncate(),
                });
            }
        }
    }
}

fn update_wall_sensor(
    mut query: Query<(&Transform, &mut WallSensor, &Collider), With<Player>>,
    spatial_query: PhysicsWorld,
) {
    for (transform, mut sensor, collider) in query.iter_mut() {
        let ray_origin = transform.translation.truncate();

        // Collider half-extents for proper wall detection
        let half_width = if let Some(cuboid) = collider.as_cuboid() {
            cuboid.half_extents().x
        } else {
            2.0 // Default fallback
        };

        // Cast ray to the left
        let left_dir = Vec2::NEG_X;
        if let Some((_, toi)) = spatial_query.ray_cast(
            ray_origin,
            left_dir,
            half_width + WALL_CAST_DISTANCE,
            true,
            CollisionGroups::new(PLAYER, GROUND),
            None,
        ) {
            // Tolerance approximates collision skin width to reduce gaps
            sensor.left_contact = toi.time_of_impact <= half_width + 0.05;
            sensor.left_distance = toi.time_of_impact;
        } else {
            sensor.left_contact = false;
            sensor.left_distance = WALL_CAST_DISTANCE;
        }

        // Cast ray to the right
        let right_dir = Vec2::X;
        if let Some((_, toi)) = spatial_query.ray_cast(
            ray_origin,
            right_dir,
            half_width + WALL_CAST_DISTANCE,
            true,
            CollisionGroups::new(PLAYER, GROUND),
            None,
        ) {
            // Tolerance approximates collision skin width to reduce gaps
            sensor.right_contact = toi.time_of_impact <= half_width + 0.05;
            sensor.right_distance = toi.time_of_impact;
        } else {
            sensor.right_contact = false;
            sensor.right_distance = WALL_CAST_DISTANCE;
        }
    }
}

fn update_ceiling_sensor(
    mut query: Query<
        (
            &Transform,
            &mut CeilingSensor,
            &Collider,
        ),
        With<Player>,
    >,
    spatial_query: PhysicsWorld,
) {
    for (transform, mut sensor, collider) in query.iter_mut() {
        let ray_origin = transform.translation.truncate();

        // Collider half-height for proper ceiling detection
        let half_height = if let Some(cuboid) = collider.as_cuboid() {
            cuboid.half_extents().y
        } else {
            8.0 // Default fallback
        };

        // Cast ray upward
        let up_dir = Vec2::Y;
        if let Some((_, toi)) = spatial_query.ray_cast(
            ray_origin,
            up_dir,
            half_height + CEILING_CAST_DISTANCE,
            true,
            CollisionGroups::new(PLAYER, GROUND),
            None,
        ) {
            // Consider touching ceiling if within a small threshold
            sensor.is_touching_ceiling =
                toi.time_of_impact <= half_height + 2.0;
            sensor.distance = toi.time_of_impact;
        } else {
            sensor.is_touching_ceiling = false;
            sensor.distance = CEILING_CAST_DISTANCE;
        }
    }
}

fn update_enemy_proximity_sensor(
    mut query: Query<
        (
            Entity,
            &Transform,
            &Facing,
            &Collider,
            &mut EnemyProximitySensor,
        ),
        With<Player>,
    >,
    enemy_query: Query<(Entity, &Transform), With<Enemy>>,
    spatial_query: PhysicsWorld,
) {
    for (
        player_entity,
        player_transform,
        facing,
        player_collider,
        mut sensor,
    ) in query.iter_mut()
    {
        let player_pos = player_transform.translation.truncate();

        // Reset previous frame's data
        sensor.enemies_in_melee_range.clear();
        sensor.enemies_in_ranged_range.clear();
        sensor.closest_enemy = None;
        sensor.closest_distance = f32::MAX;
        sensor.has_forward_enemy = false;
        sensor.has_enemy_below = false;
        sensor.has_blocking_enemy_ahead = false;

        // Check all enemies
        for (enemy_entity, enemy_transform) in enemy_query.iter() {
            let enemy_pos = enemy_transform.translation.truncate();
            let distance = player_pos.distance(enemy_pos);

            // Update closest enemy
            if distance < sensor.closest_distance {
                sensor.closest_enemy = Some(enemy_entity);
                sensor.closest_distance = distance;
            }

            // Categorize by range
            if distance <= MELEE_RANGE {
                sensor.enemies_in_melee_range.push(enemy_entity);
            }
            if distance <= RANGED_RANGE {
                sensor.enemies_in_ranged_range.push(enemy_entity);
            }

            // Forward window check in a narrow band
            let forward_len = 60.0;
            let half_width = 10.0;
            let dir = facing.direction(); // 1.0 right, -1.0 left
            let start_x = player_pos.x;
            let end_x = player_pos.x + forward_len * dir;
            let within_x = if dir > 0.0 {
                enemy_pos.x >= start_x && enemy_pos.x <= end_x
            } else {
                enemy_pos.x <= start_x && enemy_pos.x >= end_x
            };
            let within_y = (enemy_pos.y - player_pos.y).abs() <= half_width;
            if within_x && within_y {
                sensor.has_forward_enemy = true;
            }
        }

        // Downward shape cast: 120px wide (half_width 60), cast down 180px from player center
        let rapier = spatial_query.context();
        let sweep_shape = Collider::cuboid(60.0, 1.0);
        let down_hit = rapier.cast_shape(
            player_pos,
            0.0,
            Vec2::NEG_Y,
            &sweep_shape,
            ShapeCastOptions {
                max_time_of_impact: 180.0,
                ..Default::default()
            },
            QueryFilter::new()
                .groups(InteractionGroups::new(PLAYER, ENEMY))
                .exclude_collider(player_entity),
        );
        if down_hit.is_some() {
            sensor.has_enemy_below = true;
        }

        // Immediate forward-block detection: cast player's collider a small distance ahead
        let dir = facing.direction(); // 1.0 right, -1.0 left
        let cast_dir = if dir >= 0.0 { Vec2::X } else { Vec2::NEG_X };
        let forward_hit = rapier.cast_shape(
            player_pos,
            0.0,
            cast_dir,
            player_collider,
            ShapeCastOptions {
                max_time_of_impact: 2.0,
                ..Default::default()
            }, // ~2px ahead
            QueryFilter::new()
                .groups(InteractionGroups::new(PLAYER, ENEMY))
                .exclude_collider(player_entity),
        );
        if forward_hit.is_some() {
            sensor.has_blocking_enemy_ahead = true;
        }
    }
}

/// Reset jump count when player touches a wall - enables wall jumping
fn reset_jump_count_on_wall_touch(
    mut query: Query<
        (
            &mut JumpCount,
            &WallSensor,
            &Collider,
            Option<&mut Falling>,
            Option<&mut Jumping>,
        ),
        With<Player>,
    >,
) {
    for (mut jump_count, wall_sensor, collider, mut falling, mut jumping) in
        query.iter_mut()
    {
        // Player half-width for accurate distance calculation
        let half_width = if let Some(cuboid) = collider.as_cuboid() {
            cuboid.half_extents().x
        } else {
            2.0 // Default fallback
        };

        // Forgiving wall detection within 2 px of the wall
        const WALL_FORGIVENESS_DISTANCE: f32 = 2.0;
        let near_left_wall =
            wall_sensor.left_distance <= half_width + WALL_FORGIVENESS_DISTANCE;
        let near_right_wall = wall_sensor.right_distance
            <= half_width + WALL_FORGIVENESS_DISTANCE;
        let is_near_wall = near_left_wall || near_right_wall;

        // Reset jump count near a wall in Falling or Jumping states
        if is_near_wall {
            if let Some(ref mut falling_state) = falling {
                // Treat wall jumps as fresh jumps for forgiveness
                falling_state.jump_count = 0;
                jump_count.reset();
            }

            if let Some(ref mut jumping_state) = jumping {
                // Maintain consistency when transitioning from jump to wall
                jumping_state.jump_count = 0;
                jump_count.reset();
            }
        }
    }
}
