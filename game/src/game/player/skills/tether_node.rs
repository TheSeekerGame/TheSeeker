use bevy::prelude::*;
use theseeker_engine::physics::{geometry::ProbeDirection, Collider, TileCollisionWorld};

use crate::game::gentstate::Facing;
use crate::game::player::sensors::GroundSensor;
use crate::game::player::spawns::tether_node::TETHER_HALF_EXTENTS;

const TETHER_FORWARD_OFFSET: u8 = 18;
const TETHER_FALLBACK_SEARCH_RADIUS: u8 = TETHER_FORWARD_OFFSET * 2;
const TETHER_GROUND_PROBE_DISTANCE: f32 = 1.0;
const TETHER_GROUND_CONTACT_TOLERANCE: f32 = 0.01;
const TETHER_SUPPORT_SAMPLE_OFFSET: f32 = 0.5;

#[derive(Resource, Debug, Default, Clone, Copy)]
pub(crate) struct TetherConnectionStatus {
    pub(crate) available: bool,
}

pub(super) fn compute_tether_placement(
    player_transform: &Transform,
    facing: &Facing,
    player_collider: &Collider,
    ground_sensor: &GroundSensor,
    collision_world: Option<&TileCollisionWorld>,
) -> super::placement_preview::Placement {
    let player_pos = player_transform.translation.truncate();
    let center_x = (player_pos.x
        + facing.direction() * f32::from(TETHER_FORWARD_OFFSET))
    .round();
    let player_half_extents =
        player_collider.cuboid_half_extents();
    let ground_surface_y = player_half_extents.and_then(|half_extents| {
        player_ground_surface_y(
            player_pos,
            half_extents,
            ground_sensor,
            collision_world,
        )
    });
    let center_y = ground_surface_y
        .map_or(player_pos.y, |surface_y| surface_y + TETHER_HALF_EXTENTS.y);

    let preferred_center = Vec2::new(center_x, center_y);
    let valid_center =
        match (ground_surface_y, player_half_extents, collision_world) {
            (Some(_), Some(half_extents), Some(world)) => {
                nearest_valid_tether_center(
                    world,
                    preferred_center,
                    player_pos,
                    half_extents,
                    facing,
                )
            },
            _ => None,
        };

    super::placement_preview::Placement {
        center: valid_center.unwrap_or(preferred_center),
        valid: valid_center.is_some(),
    }
}

fn player_ground_surface_y(
    player_pos: Vec2,
    player_half_extents: Vec2,
    ground_sensor: &GroundSensor,
    collision_world: Option<&TileCollisionWorld>,
) -> Option<f32> {
    if !ground_sensor.is_grounded {
        return None;
    }

    let collision_world = collision_world.filter(|world| world.is_ready())?;
    let distance = collision_world.distance(
        ProbeDirection::Down,
        player_pos,
        player_half_extents,
        TETHER_GROUND_PROBE_DISTANCE,
    );

    (distance <= TETHER_GROUND_CONTACT_TOLERANCE)
        .then_some(player_pos.y - player_half_extents.y - distance)
}

fn nearest_valid_tether_center(
    collision_world: &TileCollisionWorld,
    preferred_center: Vec2,
    player_pos: Vec2,
    player_half_extents: Vec2,
    facing: &Facing,
) -> Option<Vec2> {
    nearest_valid_tether_center_with_clearance(
        collision_world,
        preferred_center,
        player_pos,
        player_half_extents,
        facing,
        true,
    )
    .or_else(|| {
        nearest_valid_tether_center_with_clearance(
            collision_world,
            preferred_center,
            player_pos,
            player_half_extents,
            facing,
            false,
        )
    })
}

fn nearest_valid_tether_center_with_clearance(
    collision_world: &TileCollisionWorld,
    preferred_center: Vec2,
    player_pos: Vec2,
    player_half_extents: Vec2,
    facing: &Facing,
    require_player_clearance: bool,
) -> Option<Vec2> {
    if is_valid_tether_candidate(
        collision_world,
        preferred_center,
        player_pos,
        player_half_extents,
        require_player_clearance,
    ) {
        return Some(preferred_center);
    }

    let forward = facing.direction();
    for distance in 1..=TETHER_FALLBACK_SEARCH_RADIUS {
        let offset = forward * f32::from(distance);
        let inward =
            Vec2::new(preferred_center.x - offset, preferred_center.y);
        if is_valid_tether_candidate(
            collision_world,
            inward,
            player_pos,
            player_half_extents,
            require_player_clearance,
        ) {
            return Some(inward);
        }

        let outward =
            Vec2::new(preferred_center.x + offset, preferred_center.y);
        if is_valid_tether_candidate(
            collision_world,
            outward,
            player_pos,
            player_half_extents,
            require_player_clearance,
        ) {
            return Some(outward);
        }
    }

    None
}

fn is_valid_tether_candidate(
    collision_world: &TileCollisionWorld,
    center: Vec2,
    player_pos: Vec2,
    player_half_extents: Vec2,
    require_player_clearance: bool,
) -> bool {
    (!require_player_clearance
        || !aabbs_overlap(
            center,
            TETHER_HALF_EXTENTS,
            player_pos,
            player_half_extents,
        ))
        && is_valid_ground_placement(collision_world, center)
}

fn aabbs_overlap(
    first_center: Vec2,
    first_half_extents: Vec2,
    second_center: Vec2,
    second_half_extents: Vec2,
) -> bool {
    let center_delta = (first_center - second_center).abs();
    center_delta.x < first_half_extents.x + second_half_extents.x
        && center_delta.y < first_half_extents.y + second_half_extents.y
}

fn is_valid_ground_placement(
    collision_world: &TileCollisionWorld,
    center: Vec2,
) -> bool {
    if !collision_world.is_ready()
        || collision_world.overlaps_solid(center, TETHER_HALF_EXTENTS)
    {
        return false;
    }

    let sample_y =
        center.y - TETHER_HALF_EXTENTS.y - TETHER_SUPPORT_SAMPLE_OFFSET;
    collision_world
        .is_solid_at_world_point(Vec2::new(center.x, sample_y))
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_TILE_SIZE: f32 = 6.0;
    const TEST_WORLD_WIDTH: u32 = 12;
    const TEST_WORLD_HEIGHT: u32 = 10;
    const GROUND_ROW: i32 = 8;
    const GROUND_SURFACE_Y: f32 = 12.0;
    const PLAYER_HALF_EXTENTS: Vec2 = Vec2::new(2.0, 5.0);

    fn collision_world(solids: &[(i32, i32)]) -> TileCollisionWorld {
        let mut terrain =
            vec![false; (TEST_WORLD_WIDTH * TEST_WORLD_HEIGHT) as usize];
        for &(x, y) in solids {
            terrain[(y as u32 * TEST_WORLD_WIDTH + x as u32) as usize] = true;
        }

        let mut world = TileCollisionWorld::default();
        world.rebuild(
            TEST_WORLD_WIDTH,
            TEST_WORLD_HEIGHT,
            TEST_TILE_SIZE,
            &terrain,
        );
        world
    }

    fn grounded_player_transform() -> Transform {
        Transform::from_xyz(15.0, GROUND_SURFACE_Y + PLAYER_HALF_EXTENTS.y, 0.0)
    }

    fn grounded_sensor() -> GroundSensor {
        GroundSensor {
            is_grounded: true,
            distance: 0.0,
        }
    }

    #[test]
    fn tether_placement_uses_the_players_ground_plane() {
        let world = collision_world(&[
            (2, GROUND_ROW),
            (4, GROUND_ROW),
            (5, GROUND_ROW),
            (6, GROUND_ROW),
        ]);
        let placement = compute_tether_placement(
            &grounded_player_transform(),
            &Facing::Right,
            &Collider::cuboid(PLAYER_HALF_EXTENTS.x, PLAYER_HALF_EXTENTS.y),
            &grounded_sensor(),
            Some(&world),
        );

        assert_eq!(
            placement.center.y,
            GROUND_SURFACE_Y + TETHER_HALF_EXTENTS.y
        );
        assert!(placement.valid);
    }

    #[test]
    fn blocked_preferred_location_uses_the_nearest_valid_fallback() {
        let mut solids =
            (2..=10).map(|x| (x, GROUND_ROW)).collect::<Vec<_>>();
        solids.push((5, GROUND_ROW - 1));
        let world = collision_world(&solids);
        let placement = compute_tether_placement(
            &grounded_player_transform(),
            &Facing::Right,
            &Collider::cuboid(PLAYER_HALF_EXTENTS.x, PLAYER_HALF_EXTENTS.y),
            &grounded_sensor(),
            Some(&world),
        );

        assert_eq!(placement.center.x, 46.0);
        assert!(placement.valid);
    }

    #[test]
    fn tether_placement_prefers_player_platform_over_lower_ground() {
        let lower_ground_row = GROUND_ROW + 1;
        let world = collision_world(&[
            (2, GROUND_ROW),
            (4, lower_ground_row),
            (5, lower_ground_row),
            (6, lower_ground_row),
        ]);
        let placement = compute_tether_placement(
            &grounded_player_transform(),
            &Facing::Right,
            &Collider::cuboid(PLAYER_HALF_EXTENTS.x, PLAYER_HALF_EXTENTS.y),
            &grounded_sensor(),
            Some(&world),
        );

        assert_eq!(
            placement.center.y,
            GROUND_SURFACE_Y + TETHER_HALF_EXTENTS.y
        );
        assert!(placement.valid);
        let (tile_min, tile_max) = world.tile_bounds(2, GROUND_ROW);
        assert!(placement.center.x >= tile_min.x);
        assert!(placement.center.x < tile_max.x);
    }

    #[test]
    fn tether_placement_prefers_player_platform_over_higher_ground() {
        let higher_ground_row = GROUND_ROW - 1;
        let world = collision_world(&[
            (2, GROUND_ROW),
            (4, higher_ground_row),
            (5, higher_ground_row),
            (6, higher_ground_row),
        ]);
        let placement = compute_tether_placement(
            &grounded_player_transform(),
            &Facing::Right,
            &Collider::cuboid(PLAYER_HALF_EXTENTS.x, PLAYER_HALF_EXTENTS.y),
            &grounded_sensor(),
            Some(&world),
        );

        assert_eq!(
            placement.center.y,
            GROUND_SURFACE_Y + TETHER_HALF_EXTENTS.y
        );
        assert!(placement.valid);
        let (tile_min, tile_max) = world.tile_bounds(2, GROUND_ROW);
        assert!(placement.center.x >= tile_min.x);
        assert!(placement.center.x < tile_max.x);
    }

    #[test]
    fn central_base_support_allows_controlled_edge_overhang() {
        let world =
            collision_world(&[(5, GROUND_ROW), (6, GROUND_ROW)]);
        let center = Vec2::new(
            35.0,
            GROUND_SURFACE_Y + TETHER_HALF_EXTENTS.y,
        );

        assert!(is_valid_ground_placement(&world, center));
        assert!(
            center.x - TETHER_HALF_EXTENTS.x
                < world.tile_bounds(5, GROUND_ROW).0.x
        );
    }

    #[test]
    fn unsupported_tether_placement_is_invalid() {
        let world =
            collision_world(&[(5, GROUND_ROW), (6, GROUND_ROW)]);
        let center = Vec2::new(
            29.0,
            GROUND_SURFACE_Y + TETHER_HALF_EXTENTS.y,
        );

        assert!(!is_valid_ground_placement(&world, center));
    }

    #[test]
    fn one_tile_platform_uses_overlap_fallback_when_clearance_is_impossible() {
        let world = collision_world(&[(2, GROUND_ROW)]);
        let player_transform = grounded_player_transform();
        let placement = compute_tether_placement(
            &player_transform,
            &Facing::Right,
            &Collider::cuboid(PLAYER_HALF_EXTENTS.x, PLAYER_HALF_EXTENTS.y),
            &grounded_sensor(),
            Some(&world),
        );

        assert!(placement.valid);
        assert!(aabbs_overlap(
            placement.center,
            TETHER_HALF_EXTENTS,
            player_transform.translation.truncate(),
            PLAYER_HALF_EXTENTS,
        ));
        let (tile_min, tile_max) = world.tile_bounds(2, GROUND_ROW);
        assert!(placement.center.x >= tile_min.x);
        assert!(placement.center.x < tile_max.x);
    }
}
