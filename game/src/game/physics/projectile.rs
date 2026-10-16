use crate::game::combat::{DamageSource, Hit};
use bevy::prelude::*;
use theseeker_engine::ballistics_math::solve_ballistic_arc;
use theseeker_engine::physics::{
    Collider, CollisionGroups, LinearVelocity,
    PhysicsWorld, ShapeCast, CastHit, TileCollisionWorld, ENEMY,
    PLAYER_ATTACK,
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
        if result.2 == 0 {
            return None;
        }

        Some(Self {
            vel: LinearVelocity(higher_ballistic_arc(result.0, result.1)),
        })
    }
}

fn higher_ballistic_arc(first: Vec2, second: Vec2) -> Vec2 {
    if first.y > second.y {
        first
    } else {
        second
    }
}

#[derive(Component)]
pub struct Arrow;

/// Marker for projectiles that should ignore ground/wall blocking during CCD.
#[derive(Component)]
pub struct WallPiercingProjectile;

/// Marker for arc projectiles that despawn on ground collision.
/// Used for both enemy spider balls and player spit projectiles.
#[derive(Component)]
pub struct GroundDespawnProjectile;

/// Tracks projectiles that have hit the ground and are awaiting despawn.
/// The projectile stops moving and despawns after the timer expires.
#[derive(Component)]
pub struct GroundedProjectile {
    pub ticks_remaining: u32,
}

impl GroundedProjectile {
    /// Delay before grounded projectiles despawn (16 ticks at 96Hz ≈ 167ms)
    pub const DESPAWN_DELAY_TICKS: u32 = 16;
}

#[derive(Component)]
pub struct PreviousPosition(pub Vec2);

/// Applies gravity to projectiles (except player arrows and grounded projectiles)
pub fn arc_projectile(
    mut query: Query<
        (
            &mut Transform,
            &mut Projectile,
            Option<&mut PreviousPosition>,
            Has<Arrow>,
        ),
        (
            With<DamageSource>,
            Without<GroundedProjectile>,
        ),
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
            Has<WallPiercingProjectile>,
        ),
        With<Arrow>,
    >,
    spatial_query: PhysicsWorld,
    collision_world: Option<Res<TileCollisionWorld>>,
    mut commands: Commands,
) {
    for (
        arrow_entity,
        arrow_transform,
        prev_pos,
        mut attack,
        arrow_collider,
        wall_piercing,
    ) in arrow_query.iter_mut()
    {
        let current_pos = arrow_transform.translation.xy();
        let previous_pos = prev_pos.0;

        if current_pos.distance_squared(previous_pos) < 1.0 {
            continue;
        }

        let direction = current_pos - previous_pos;
        let max_distance = direction.length();
        if max_distance < 0.001 {
            continue;
        }
        let normalized_dir = direction / max_distance;

        let wall_hit = if wall_piercing {
            None
        } else {
            ray_cast_ready_tile_world(
                collision_world.as_deref(),
                previous_pos,
                normalized_dir,
                max_distance,
            )
        };

        let enemy_hit = spatial_query.cast_shape(ShapeCast {
            origin: previous_pos,
            dir: normalized_dir,
            shape: arrow_collider,
            max_toi: max_distance,
            interaction: CollisionGroups::new(PLAYER_ATTACK, ENEMY),
            exclude: Some(arrow_entity),
        }, |_| true);

        let wall_distance = wall_hit
            .map(|hit| hit.time_of_impact)
            .unwrap_or(f32::MAX);

        if record_arrow_enemy_hit_before_wall(
            &mut attack,
            enemy_hit,
            wall_distance,
        ) {
            continue;
        }

        if !wall_piercing && wall_distance <= max_distance {
            commands.entity(arrow_entity).try_despawn();
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
            commands.entity(entity).try_despawn();
        }
    }
}

/// Detects ground collision for arc projectiles and stops them.
/// Uses surface normals to distinguish ground (upward-facing) from ceilings/walls.
/// A surface is considered "ground" if its normal.y > 0.7 (roughly horizontal, facing up).
pub fn arc_projectile_ground_collision(
    mut query: Query<
        (Entity, &Transform, &mut Projectile),
        (
            With<GroundDespawnProjectile>,
            Without<GroundedProjectile>,
        ),
    >,
    collision_world: Option<Res<TileCollisionWorld>>,
    mut commands: Commands,
) {
    // Threshold for "upward-facing" surface (cos(45°) ≈ 0.707)
    // Surfaces tilted less than ~45° from horizontal are considered ground-like.
    const GROUND_NORMAL_THRESHOLD: f32 = 0.7;

    for (entity, transform, mut projectile) in query.iter_mut() {
        // Only check when falling - optimization to avoid unnecessary raycasts when rising.
        // The normal check is the authoritative ground detection; velocity is just a filter.
        if projectile.vel.0.y >= 0.0 {
            continue;
        }

        // Cast distance proportional to fall speed to prevent tunneling.
        let fall_speed = -projectile.vel.0.y;
        let check_distance = (fall_speed / 96.0).max(2.0);

        let hit = ray_cast_ready_tile_world(
            collision_world.as_deref(),
            transform.translation.xy(),
            Vec2::NEG_Y,
            check_distance,
        );

        // If we hit ground (upward-facing surface), stop the projectile and start despawn timer.
        let Some(intersection) = hit else {
            continue;
        };
        if intersection.normal.y <= GROUND_NORMAL_THRESHOLD {
            continue;
        }

        ground_projectile(entity, &mut projectile, &mut commands);
    }
}

fn ray_cast_ready_tile_world(
    collision_world: Option<&TileCollisionWorld>,
    origin: Vec2,
    direction: Vec2,
    max_distance: f32,
) -> Option<CastHit> {
    let world = collision_world?;
    if !world.is_ready() {
        return None;
    }

    world.ray_cast_solid(origin, direction, max_distance)
}

fn record_arrow_enemy_hit_before_wall(
    attack: &mut DamageSource,
    enemy_hit: Option<(Entity, CastHit)>,
    wall_distance: f32,
) -> bool {
    let Some((hit_entity, hit_info)) = enemy_hit else {
        return false;
    };
    if hit_info.time_of_impact >= wall_distance {
        return false;
    }

    if damage_source_has_target_capacity(attack) {
        attack.target_set.insert(hit_entity);
    }
    true
}

fn damage_source_has_target_capacity(attack: &DamageSource) -> bool {
    let total_targets = attack.damaged_set.len() + attack.target_set.len();
    total_targets < attack.max_targets as usize
}

fn ground_projectile(
    entity: Entity,
    projectile: &mut Projectile,
    commands: &mut Commands,
) {
    projectile.vel.0 = Vec2::ZERO;
    commands.entity(entity).insert(GroundedProjectile {
        ticks_remaining: GroundedProjectile::DESPAWN_DELAY_TICKS,
    });
}

/// Counts down grounded projectiles and despawns them when the timer expires.
pub fn grounded_projectile_despawn(
    mut query: Query<(Entity, &mut GroundedProjectile)>,
    mut commands: Commands,
) {
    for (entity, mut grounded) in query.iter_mut() {
        if grounded.ticks_remaining == 0 {
            commands.entity(entity).try_despawn();
        } else {
            grounded.ticks_remaining -= 1;
        }
    }
}
