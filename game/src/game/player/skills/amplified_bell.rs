use bevy::prelude::*;

use super::types::{
    AmplifiedBellMetadata, CooldownMode, CooldownSpec, 
};
use crate::game::gentstate::Facing;
use theseeker_engine::physics::{
    geometry::ProbeDirection,
    Collider, CollisionGroups, Group,
    PhysicsWorld, TileCollisionWorld, ENEMY, GROUNDED_THRESHOLD,
};

const BELL_HALF_WIDTH: f32 = 6.0;
const BELL_HALF_HEIGHT: f32 = 8.0;

pub(crate) const AMPLIFIED_BELL_METADATA: AmplifiedBellMetadata =
    AmplifiedBellMetadata {
        cooldown: CooldownSpec {
            min_ticks: 0,
            max_ticks: 1152,
            mode: CooldownMode::RateBased,
        },
    };

/// Determine a valid placement point for the amplified bell given the current player pose.
pub fn find_amplified_bell_placement(
    player_transform: &Transform,
    facing: &Facing,
    spatial_query: &PhysicsWorld,
    collision_world: Option<&TileCollisionWorld>,
    owner: Entity,
) -> Option<Vec2> {
    let collision_world = collision_world.filter(|world| world.is_ready())?;
    let player_pos = player_transform.translation.truncate();
    let dir = facing.direction();
    let candidate_offsets = [16.0, 12.0, 8.0, 4.0, 0.0];

    for offset in candidate_offsets.into_iter() {
        if let Some(candidate) = amplified_bell_candidate(
            player_pos,
            dir,
            offset,
            spatial_query,
            collision_world,
            owner,
        ) {
            return Some(candidate);
        }
    }

    None
}

fn amplified_bell_candidate(
    player_pos: Vec2,
    dir: f32,
    offset: f32,
    spatial_query: &PhysicsWorld,
    collision_world: &TileCollisionWorld,
    owner: Entity,
) -> Option<Vec2> {
    let x = player_pos.x + dir * offset;
    let start_y = player_pos.y + 4.0;
    let distance =
        collision_world.distance(ProbeDirection::Down, Vec2::new(x, start_y), Vec2::ZERO, 64.0);
    if distance >= 64.0 {
        return None;
    }

    let ground_y = start_y - distance;
    if player_pos.y - ground_y > 50.0 {
        return None;
    }

    let clearance = GROUNDED_THRESHOLD + 0.1;
    let bell_center_y = ground_y + BELL_HALF_HEIGHT + clearance;
    let candidate_center = Vec2::new(x, bell_center_y);
    if collision_world.overlaps_solid(
        candidate_center,
        Vec2::new(BELL_HALF_WIDTH, BELL_HALF_HEIGHT),
    ) {
        return None;
    }

    let overlaps_enemy = spatial_query.intersect(
        candidate_center,
        &Collider::cuboid(BELL_HALF_WIDTH, BELL_HALF_HEIGHT),
        CollisionGroups::new(Group::all(), ENEMY),
        Some(owner),
    );
    overlaps_enemy.is_empty().then_some(candidate_center)
}
