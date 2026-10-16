use super::*;

fn fixture() -> (World, Entity, Schedule) {
    let mut world = World::new();
    world.init_resource::<tether_node::TetherConnectionStatus>();
    world.init_resource::<Messages<PassiveEvent>>();
    let mut tiles = TileCollisionWorld::default();
    let mut terrain = vec![false; 128 * 10];
    terrain[128 * 8..128 * 9].fill(true);
    tiles.rebuild(128, 10, 6.0, &terrain);
    world.insert_resource(tiles);
    let mut materials = MaterialInventory::empty();
    materials.add_to_backpack(CraftMaterial::Tent, 2);
    materials.add_to_backpack(CraftMaterial::Tether, 2);
    let player = world.spawn((Player, ActionState::<PlayerAction>::default(),
        Transform::from_xyz(30.0, 17.0, 0.0), Facing::Right, Collider::cuboid(2.0, 5.0),
        GroundSensor { is_grounded: true, distance: 0.0 }, materials,
        SkillInventory { equipped: vec![SkillId::PortableTent, SkillId::TetherNode], inventory: Vec::new() })).id();
    let mut schedule = Schedule::default();
    schedule.add_systems((update_placement_previews, cleanup_orphaned_previews).chain());
    (world, player, schedule)
}

fn set_buttons(world: &mut World, player: Entity, pressed: bool) {
    let mut input = world.get_mut::<ActionState<PlayerAction>>(player).unwrap();
    for action in [PlayerAction::Skill1, PlayerAction::Skill2] {
        if pressed { input.press(&action); } else { input.release(&action); }
    }
}

#[test]
fn held_previews_keep_stock_and_release_consumes_each_item_once() {
    let (mut world, player, mut schedule) = fixture();
    set_buttons(&mut world, player, true);
    schedule.run(&mut world);
    schedule.run(&mut world);
    assert_eq!(world.query::<&PlacementPreview>().iter(&world).count(), 2);
    assert_eq!(world.get::<MaterialInventory>(player).unwrap().backpack_count(CraftMaterial::Tent), 2);
    set_buttons(&mut world, player, false);
    schedule.run(&mut world);
    assert_eq!(world.query::<&PlacementPreview>().iter(&world).count(), 0);
    assert_eq!(world.query::<&portable_tent::PortableTent>().iter(&world).count(), 1);
    assert_eq!(world.query::<&TetherNode>().iter(&world).count(), 1);
    assert_eq!(world.resource::<Messages<PassiveEvent>>().len(), 2);
    *world.get_mut::<ActionState<PlayerAction>>(player).unwrap() = ActionState::default();
    schedule.run(&mut world);
    for material in [CraftMaterial::Tent, CraftMaterial::Tether] {
        assert_eq!(world.get::<MaterialInventory>(player).unwrap().backpack_count(material), 1);
    }
    assert_eq!(world.resource::<Messages<PassiveEvent>>().len(), 2);
}

#[test]
fn rejected_placement_and_orphan_cleanup_do_not_pay_or_emit_skill_used() {
    let (mut world, player, mut schedule) = fixture();
    world.remove_resource::<TileCollisionWorld>();
    set_buttons(&mut world, player, true);
    schedule.run(&mut world);
    set_buttons(&mut world, player, false);
    schedule.run(&mut world);
    for material in [CraftMaterial::Tent, CraftMaterial::Tether] {
        assert_eq!(world.get::<MaterialInventory>(player).unwrap().backpack_count(material), 2);
    }
    assert!(world.resource::<Messages<PassiveEvent>>().is_empty());
    set_buttons(&mut world, player, true);
    schedule.run(&mut world);
    assert_eq!(world.query::<&PlacementPreview>().iter(&world).count(), 2);
    world.despawn(player);
    schedule.run(&mut world);
    assert_eq!(world.query::<&PlacementPreview>().iter(&world).count(), 0);
}
