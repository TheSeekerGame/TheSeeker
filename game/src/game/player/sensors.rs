//! Player sensors updated at 96 Hz.
//!
//! Provides ground/wall/ceiling contact data and enemy proximity hints for
//! state machines. Uses the custom entity collision world for detection.
use super::collision_view::{PlayerCollisionView, CapsuleCollisionQuery};
use theseeker_engine::physics::{
    geometry::ProbeDirection, Collider, CollisionGroups, PhysicsWorld, ShapeCast, TileCollisionWorld,
    ENEMY, PLAYER,
};
use crate::game::enemy::Enemy;
use crate::game::gentstate::{Dead, Facing};
use crate::game::player::skills::portable_tent::{
    PendingTentEntry, ShelterInteriorKind, TentInteriorAssets, TentInteriorOccupant,
};
use crate::game::player::states::{Falling, Jumping};
use crate::game::player::{JumpCount, Player, PlayerStateSet};
use crate::game::world_objects::{
    Capsule,
    CapsuleDropPlatformState,
};
use bevy::{ecs::system::SystemParam, prelude::*};
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
        app.add_systems(GameTickUpdate, record_safe_grounded_position
            .after(PlayerStateSet::Collisions)
            .before(crate::game::spikes::process_spike_hits));
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

/// A physical recovery anchor belongs to the collision space where it was recorded.
#[derive(Component, Clone, Copy, Debug)]
pub(crate) struct SafeGroundedPosition {
    pub position: Vec2,
    pub space: Option<ShelterInteriorKind>,
}

#[derive(SystemParam)]
pub(crate) struct RecoveryCollisionWorld<'w, 's> {
    terrain: Option<Res<'w, TileCollisionWorld>>,
    interiors: Option<Res<'w, TentInteriorAssets>>,
    capsules: CapsuleCollisionQuery<'w, 's, (With<Capsule>, Without<Player>)>,
    spatial: PhysicsWorld<'w>,
    spikes: Query<'w, 's, (), With<crate::game::spikes::SpikeHazard>>,
}

impl RecoveryCollisionWorld<'_, '_> {
    pub fn capsule_occupant(
        &self,
        position: Vec2,
        occupant: Option<&TentInteriorOccupant>,
    ) -> Option<crate::game::world_objects::CapsuleOccupant> {
        if occupant.is_some() { return None; }
        crate::game::world_objects::capsule_occupant_at_world_position(position, &self.capsules)
    }

    pub fn is_safe(
        &self,
        position: Vec2,
        collider: &Collider,
        occupant: Option<&TentInteriorOccupant>,
        platforms: &CapsuleDropPlatformState,
    ) -> bool {
        let Some(half) = collider.cuboid_half_extents() else { return false; };
        let Some(view) = PlayerCollisionView::new(occupant, self.interiors.as_deref(),
            self.terrain.as_deref(), &self.capsules) else { return false; };
        view.distance(ProbeDirection::Down, position, half,
            MAX_GROUND_CHECK_DISTANCE, Some(platforms)) <= GROUND_CONTACT_EPS
            && !view.overlaps_solid(position, half)
            && !self.spatial.intersect(position, collider,
                theseeker_engine::physics::groups::player_body(), None)
                .into_iter().any(|entity| self.spikes.contains(entity))
    }

    /// Resolve a safe supported fallback near an invalidated anchor or initial spawn.
    pub fn supported_destination(
        &self,
        reference: Vec2,
        collider: &Collider,
        occupant: Option<&TentInteriorOccupant>,
        platforms: &CapsuleDropPlatformState,
    ) -> Option<Vec2> {
        if self.is_safe(reference, collider, occupant, platforms) { return Some(reference); }
        let half = collider.cuboid_half_extents()?;
        let view = PlayerCollisionView::new(occupant, self.interiors.as_deref(),
            self.terrain.as_deref(), &self.capsules)?;
        let tile_size = view.tile_size();
        let reach = crate::levelgen::mapgen::CHUNK_IMAGE_SIZE as f32 * tile_size;
        let radius = crate::levelgen::mapgen::CHUNK_IMAGE_SIZE as i32;
        (-radius..=radius).filter_map(|x| {
            let start = reference + Vec2::new(x as f32 * tile_size, half.y * 2.0);
            let distance = view.distance(ProbeDirection::Down, start, half, reach, Some(platforms));
            let candidate = start - Vec2::Y * distance;
            (distance < reach && self.is_safe(candidate, collider, occupant, platforms)).then_some(candidate)
        }).min_by(|a, b| a.distance_squared(reference).total_cmp(&b.distance_squared(reference)))
    }
}

fn record_safe_grounded_position(
    mut players: Query<(Entity, &Transform, &Collider, Option<&TentInteriorOccupant>,
        &CapsuleDropPlatformState, &super::foreground::ForegroundAction,
        Option<&mut SafeGroundedPosition>),
        (With<Player>, Without<Dead>, Without<PendingTentEntry>,
            Without<crate::game::spikes::SpikeRespawnPending>)>,
    world: RecoveryCollisionWorld,
    mut commands: Commands,
) {
    for (entity, transform, collider, occupant, platforms, foreground, checkpoint) in &mut players {
        if foreground.owns_motion() { continue; }
        let position = transform.translation.truncate();
        if !world.is_safe(position, collider, occupant, platforms) { continue; }
        let safe = SafeGroundedPosition { position, space: occupant.map(|occupant| occupant.kind) };
        if let Some(mut checkpoint) = checkpoint { *checkpoint = safe; }
        else { commands.entity(entity).insert(safe); }
    }
}

const WALL_CAST_DISTANCE: f32 = 8.0;
const CEILING_CAST_DISTANCE: f32 = 8.0;
const MELEE_RANGE: f32 = 32.0;
const RANGED_RANGE: f32 = 200.0;
const MAX_GROUND_CHECK_DISTANCE: f32 = 50.0; // Cap for distance when no hit is found
const GROUND_CONTACT_EPS: f32 = 0.01;

fn update_ground_sensor(
    mut query: Query<
        (
            Entity,
            &Transform,
            &mut GroundSensor,
            &Collider,
            Option<&mut LastGroundedPosition>,
            Has<crate::game::player::states::InAir>,
            Option<&TentInteriorOccupant>,
            &CapsuleDropPlatformState,
        ),
        With<Player>,
    >,
    collision_world: Option<Res<TileCollisionWorld>>,
    tent_interior: Option<Res<TentInteriorAssets>>,
    capsule_q: CapsuleCollisionQuery<With<Capsule>>,
    _time: Res<GameTime>,
    mut commands: Commands,
) {
    for (
        entity,
        transform,
        mut sensor,
        collider,
        mut last_grounded_pos,
        is_in_air,
        interior_occupant,
        drop_platform_state,
    ) in query.iter_mut()
    {
        let Some(view) = PlayerCollisionView::new(interior_occupant,
            tent_interior.as_deref(), collision_world.as_deref(), &capsule_q) else {
            sensor.is_grounded = false;
            sensor.distance = MAX_GROUND_CHECK_DISTANCE;
            continue;
        };

        let Some(half_extents) = collider.cuboid_half_extents() else {
            sensor.is_grounded = false;
            sensor.distance = MAX_GROUND_CHECK_DISTANCE;
            continue;
        };
        let distance = view.distance(
            ProbeDirection::Down,
            transform.translation.truncate(),
            half_extents,
            MAX_GROUND_CHECK_DISTANCE,
            Some(drop_platform_state),
        );
        sensor.distance = distance;
        sensor.is_grounded = distance <= GROUND_CONTACT_EPS;

        if !is_in_air {
            update_last_grounded_position(
                &mut commands,
                entity,
                transform.translation.truncate(),
                last_grounded_pos.as_deref_mut(),
            );
        }
    }
}

fn update_last_grounded_position(
    commands: &mut Commands,
    entity: Entity,
    position: Vec2,
    last_grounded_pos: Option<&mut LastGroundedPosition>,
) {
    let Some(last_pos) = last_grounded_pos else {
        commands.entity(entity).insert(LastGroundedPosition { position });
        return;
    };

    last_pos.position = position;
}


fn update_wall_sensor(
    mut query: Query<
        (
            &Transform,
            &mut WallSensor,
            &Collider,
            Option<&TentInteriorOccupant>,
        ),
        With<Player>,
    >,
    collision_world: Option<Res<TileCollisionWorld>>,
    tent_interior: Option<Res<TentInteriorAssets>>,
    capsule_q: CapsuleCollisionQuery<With<Capsule>>,
) {
    for (transform, mut sensor, collider, interior_occupant) in query.iter_mut() {
        let Some(view) = PlayerCollisionView::new(interior_occupant,
            tent_interior.as_deref(), collision_world.as_deref(), &capsule_q) else {
            sensor.left_contact = false;
            sensor.right_contact = false;
            sensor.left_distance = WALL_CAST_DISTANCE;
            sensor.right_distance = WALL_CAST_DISTANCE;
            continue;
        };

        let Some(half_extents) = collider.cuboid_half_extents() else {
            sensor.left_contact = false;
            sensor.right_contact = false;
            sensor.left_distance = WALL_CAST_DISTANCE;
            sensor.right_distance = WALL_CAST_DISTANCE;
            continue;
        };

        let left_distance = view.distance(
            ProbeDirection::Left,
            transform.translation.truncate(),
            half_extents,
            WALL_CAST_DISTANCE,
            None,
        );
        sensor.left_distance = left_distance;
        sensor.left_contact = left_distance <= half_extents.x + 0.05;

        let right_distance = view.distance(
            ProbeDirection::Right,
            transform.translation.truncate(),
            half_extents,
            WALL_CAST_DISTANCE,
            None,
        );
        sensor.right_distance = right_distance;
        sensor.right_contact = right_distance <= half_extents.x + 0.05;
    }
}

fn update_ceiling_sensor(
    mut query: Query<
        (
            &Transform,
            &mut CeilingSensor,
            &Collider,
            Option<&TentInteriorOccupant>,
        ),
        With<Player>,
    >,
    collision_world: Option<Res<TileCollisionWorld>>,
    tent_interior: Option<Res<TentInteriorAssets>>,
    capsule_q: CapsuleCollisionQuery<With<Capsule>>,
) {
    for (transform, mut sensor, collider, interior_occupant) in query.iter_mut() {
        let Some(view) = PlayerCollisionView::new(interior_occupant,
            tent_interior.as_deref(), collision_world.as_deref(), &capsule_q) else {
            sensor.is_touching_ceiling = false;
            sensor.distance = CEILING_CAST_DISTANCE;
            continue;
        };

        let Some(half_extents) = collider.cuboid_half_extents() else {
            sensor.is_touching_ceiling = false;
            sensor.distance = CEILING_CAST_DISTANCE;
            continue;
        };

        let distance = view.distance(
            ProbeDirection::Up,
            transform.translation.truncate(),
            half_extents,
            CEILING_CAST_DISTANCE,
            None,
        );
        sensor.distance = distance;
        sensor.is_touching_ceiling = distance <= half_extents.y + 2.0;
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
    enemy_query: Query<(Entity, &Transform), (With<Enemy>, Without<Dead>)>,
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
            record_enemy_sensor_target(
                &mut sensor,
                enemy_entity,
                player_pos,
                enemy_pos,
                facing,
            );
        }

        let sweep_shape = Collider::cuboid(60.0, 1.0);
        sensor.has_enemy_below = spatial_query
            .cast_shape(ShapeCast {
                origin: player_pos,
                dir: Vec2::NEG_Y,
                shape: &sweep_shape,
                max_toi: 180.0,
                interaction: CollisionGroups::new(PLAYER, ENEMY),
                exclude: Some(player_entity),
            }, |_| true)
            .is_some();

        let dir = facing.direction();
        let cast_dir = if dir >= 0.0 { Vec2::X } else { Vec2::NEG_X };
        sensor.has_blocking_enemy_ahead = spatial_query
            .cast_shape(ShapeCast {
                origin: player_pos,
                dir: cast_dir,
                shape: player_collider,
                max_toi: 2.0,
                interaction: CollisionGroups::new(PLAYER, ENEMY),
                exclude: Some(player_entity),
            }, |_| true)
            .is_some();
    }
}

fn record_enemy_sensor_target(
    sensor: &mut EnemyProximitySensor,
    enemy_entity: Entity,
    player_pos: Vec2,
    enemy_pos: Vec2,
    facing: &Facing,
) {
    let distance = player_pos.distance(enemy_pos);
    if distance < sensor.closest_distance {
        sensor.closest_enemy = Some(enemy_entity);
        sensor.closest_distance = distance;
    }

    if distance <= MELEE_RANGE {
        sensor.enemies_in_melee_range.push(enemy_entity);
    }
    if distance <= RANGED_RANGE {
        sensor.enemies_in_ranged_range.push(enemy_entity);
    }

    sensor.has_forward_enemy |=
        enemy_in_forward_sensor_window(player_pos, enemy_pos, facing);
}

fn enemy_in_forward_sensor_window(
    player_pos: Vec2,
    enemy_pos: Vec2,
    facing: &Facing,
) -> bool {
    let forward_len = 60.0;
    let half_width = 10.0;
    let dir = facing.direction();
    let start_x = player_pos.x;
    let end_x = player_pos.x + forward_len * dir;
    let within_x = if dir > 0.0 {
        enemy_pos.x >= start_x && enemy_pos.x <= end_x
    } else {
        enemy_pos.x <= start_x && enemy_pos.x >= end_x
    };
    let within_y = (enemy_pos.y - player_pos.y).abs() <= half_width;
    within_x && within_y
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
        let half_width = if let Some(cuboid_half) = collider.cuboid_half_extents() {
            cuboid_half.x
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
        if !is_near_wall {
            continue;
        }

        reset_wall_touch_jump_count(
            &mut jump_count,
            falling.as_deref_mut(),
            jumping.as_deref_mut(),
        );
    }
}

fn reset_wall_touch_jump_count(
    jump_count: &mut JumpCount,
    falling: Option<&mut Falling>,
    jumping: Option<&mut Jumping>,
) {
    if let Some(falling_state) = falling {
        // Treat wall jumps as fresh jumps for forgiveness.
        falling_state.jump_count = 0;
        jump_count.reset();
    }

    if let Some(jumping_state) = jumping {
        // Maintain consistency when transitioning from jump to wall.
        jumping_state.jump_count = 0;
        jump_count.reset();
    }
}

#[cfg(test)]
mod recovery_tests {
    use super::*;
    use crate::game::player::foreground::start_action;
    use theseeker_engine::gent::Gent;
    use theseeker_engine::physics::{EntityCollisionWorld, Group, StaticCollider,
        groups, rebuild_entity_collision_world};

    #[test]
    fn recovery_anchor_requires_support_and_survives_air_and_activated_spikes() {
        let mut world = World::new();
        world.insert_resource(GameTime::default());
        world.insert_resource(EntityCollisionWorld::default());
        let mut terrain = vec![false; 32 * 16];
        terrain[32 * 15..].fill(true);
        let mut tiles = TileCollisionWorld::default();
        tiles.rebuild(32, 16, 1.0, &terrain);
        world.insert_resource(tiles);
        let gfx = world.spawn_empty().id();
        let player = world.spawn((Player, Gent { e_gfx: gfx, e_effects_gfx: gfx },
            Transform::from_xyz(6.0, 5.0, 0.0), Collider::cuboid(1.0, 2.0),
            CapsuleDropPlatformState::default())).id();
        start_action(&mut world, player, crate::game::player::Ready);
        let mut record = Schedule::default();
        record.add_systems(record_safe_grounded_position);
        record.run(&mut world);
        assert!(world.get::<SafeGroundedPosition>(player).is_none());
        world.get_mut::<Transform>(player).unwrap().translation.y = 3.0;
        record.run(&mut world);
        assert_eq!(world.get::<SafeGroundedPosition>(player).unwrap().position, Vec2::new(6.0, 3.0));
        world.get_mut::<Transform>(player).unwrap().translation.y = 5.0;
        record.run(&mut world);
        assert_eq!(world.get::<SafeGroundedPosition>(player).unwrap().position, Vec2::new(6.0, 3.0));
        let spike = world.spawn((crate::game::spikes::SpikeHazard, StaticCollider,
            GlobalTransform::from_xyz(12.0, 1.5, 0.0), Collider::cuboid(0.5, 0.5),
            CollisionGroups::new(Group::NONE, Group::NONE))).id();
        let mut index = Schedule::default();
        index.add_systems(rebuild_entity_collision_world);
        index.run(&mut world);
        world.get_mut::<Transform>(player).unwrap().translation = Vec3::new(12.0, 3.0, 0.0);
        record.run(&mut world);
        assert_eq!(world.get::<SafeGroundedPosition>(player).unwrap().position, Vec2::new(12.0, 3.0));
        world.entity_mut(spike).insert(groups::enemy_attack());
        index.run(&mut world);
        world.get_mut::<SafeGroundedPosition>(player).unwrap().position = Vec2::new(6.0, 3.0);
        record.run(&mut world);
        assert_eq!(world.get::<SafeGroundedPosition>(player).unwrap().position, Vec2::new(6.0, 3.0));
    }
}
