use super::*;
use crate::game::actors::action::TickDisplacement;

fn tile_world(solids: &[(usize, usize)]) -> TileCollisionWorld {
    let mut terrain = vec![false; 16];
    for &(x, y) in solids { terrain[y * 4 + x] = true; }
    let mut tiles = TileCollisionWorld::default();
    tiles.rebuild(4, 4, 10.0, &terrain);
    tiles
}

fn fixture(tiles: TileCollisionWorld, navigation: Navigation) -> (World, Schedule, Entity) {
    let mut world = World::new();
    world.insert_resource(GameTime::default());
    world.insert_resource(tiles);
    let entity = world.spawn((Enemy, GroundMotor, MovementState::default(), LinearVelocity::default(),
        Transform::from_xyz(15.0, 15.0, 0.0), Collider::cuboid(5.0, 5.0), navigation)).id();
    let mut schedule = Schedule::default();
    schedule.add_systems(collision::move_collide);
    (world, schedule, entity)
}

#[test]
fn frame_steps_use_tile_collision_and_discard_blocked_distance() {
    let floor = [(0, 3), (1, 3), (2, 3), (3, 3)];
    let mut solids = floor.to_vec();
    solids.push((2, 2));
    let (mut world, mut schedule, entity) = fixture(tile_world(&solids), Navigation::Grounded);
    world.get_mut::<MovementState>(entity).unwrap().frame_steps = vec![TickDisplacement(Vec2::X); 2];
    schedule.run(&mut world);
    assert_eq!(world.get::<Transform>(entity).unwrap().translation.x, 15.0);
    assert!(world.get::<MovementState>(entity).unwrap().frame_steps.is_empty());
    world.insert_resource(tile_world(&floor));
    schedule.run(&mut world);
    assert_eq!(world.get::<Transform>(entity).unwrap().translation.x, 15.0);
    world.get_mut::<MovementState>(entity).unwrap().frame_steps.push(TickDisplacement(Vec2::X));
    schedule.run(&mut world);
    assert_eq!(world.get::<Transform>(entity).unwrap().translation.x, 16.0);
}

#[test]
fn frame_steps_respect_ledges_and_ceilings() {
    let (mut world, mut schedule, entity) = fixture(tile_world(&[(1, 3)]), Navigation::Grounded);
    world.get_mut::<MovementState>(entity).unwrap().frame_steps.push(TickDisplacement(Vec2::X));
    schedule.run(&mut world);
    assert_eq!(world.get::<Transform>(entity).unwrap().translation.xy(), Vec2::splat(15.0));
    world.insert_resource(tile_world(&[(1, 1)]));
    *world.get_mut::<Navigation>(entity).unwrap() = Navigation::Falling { fall_ticks: 0, jumping: true };
    world.get_mut::<MovementState>(entity).unwrap().frame_steps.push(TickDisplacement(Vec2::Y));
    schedule.run(&mut world);
    assert_eq!(world.get::<Transform>(entity).unwrap().translation.y, 15.0);
}
