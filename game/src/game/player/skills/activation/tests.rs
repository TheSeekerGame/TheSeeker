use crate::game::player::skills::cooldowns::Cooldowns;
use super::*;
use crate::game::player::{foreground::{self, ActionTag}, input_buffer::InputVariant,
    material_inventory::CraftMaterial, states::{Attacking, Dashing, Ready}, spawns::mine::Mine};
use theseeker_engine::physics::{EntityCollisionWorld, LinearVelocity};

fn fixture(skills: Vec<SkillId>) -> (World, Entity, Schedule) {
    let mut world = World::new();
    world.init_resource::<Cooldowns>();
    world.init_resource::<PlayerMeleeWeapon>();
    world.init_resource::<GameTime>();
    world.init_resource::<EntityCollisionWorld>();
    world.init_resource::<Messages<crate::game::player::passives::PassiveEvent>>();
    let mut materials = MaterialInventory::empty();
    materials.add_to_backpack(CraftMaterial::ExplosiveMine, 3);
    let player = world.spawn((Player, ActionState::<PlayerAction>::default(), InputBuffer::default(),
        SkillInventory { equipped: skills, inventory: Vec::new() }, materials, WeaponInventory::default(),
        WhirlAbility::default(), FlickerAbility::default(), Transform::default(), Facing::Right,
        HorizontalInputIntent::default(), Health { current: 10, max: 10 },
        GroundSensor { is_grounded: true, distance: 0.0 }, LinearVelocity(Vec2::new(2.0, 1.0)))).id();
    foreground::start_action(&mut world, player, Ready);
    let mut schedule = Schedule::default();
    schedule.add_systems(skill_dispatch_system);
    (world, player, schedule)
}

fn press(world: &mut World, player: Entity, actions: &[PlayerAction]) {
    let mut input = world.get_mut::<ActionState<PlayerAction>>(player).unwrap();
    for action in actions { input.press(action); }
}

#[test]
fn one_foreground_request_and_instant_items_work_in_either_slot_order() {
    assert_slot_order(vec![SkillId::Attack, SkillId::ExplosiveMine, SkillId::Dash]);
    assert_slot_order(vec![SkillId::ExplosiveMine, SkillId::Attack, SkillId::Dash]);
}

fn assert_slot_order(skills: Vec<SkillId>) {
    let (mut world, player, mut schedule) = fixture(skills);
    press(&mut world, player, &[PlayerAction::Skill1, PlayerAction::Skill2, PlayerAction::Skill3]);
    schedule.run(&mut world);
    assert!(world.get::<ActionTag<Attacking>>(player).is_some());
    assert!(world.get::<ActionTag<Dashing>>(player).is_none());
    assert!(world.resource::<Cooldowns>().is_ready(player, SkillId::Dash, world.resource::<GameTime>().tick()));
    assert_eq!(world.query_filtered::<Entity, With<Mine>>().iter(&world).count(), 1);
    assert_eq!(world.get::<MaterialInventory>(player).unwrap().backpack_count(CraftMaterial::ExplosiveMine), 2);
}

#[test]
fn failed_foreground_request_does_not_block_another_valid_request() {
    let (mut world, player, mut schedule) = fixture(vec![SkillId::Whirl, SkillId::Dash]);
    press(&mut world, player, &[PlayerAction::Skill1, PlayerAction::Skill2]);
    schedule.run(&mut world);
    assert!(world.get::<ActionTag<Dashing>>(player).is_some());
}

#[test]
fn buffered_skill_can_activate_after_the_button_is_released() {
    let (mut world, player, mut schedule) = fixture(vec![SkillId::Dash]);
    let tick = world.resource::<GameTime>().tick();
    world.get_mut::<InputBuffer>(player).unwrap().push(PlayerAction::Skill1, tick, InputVariant::Normal);
    schedule.run(&mut world);
    assert!(world.get::<ActionTag<Dashing>>(player).is_some());
    assert!(world.get::<InputBuffer>(player).unwrap().check_buffered(PlayerAction::Skill1, tick).is_none());
}

#[test]
fn instant_use_keeps_the_active_ability_and_its_motion() {
    let (mut world, player, mut schedule) = fixture(vec![SkillId::ExplosiveMine]);
    foreground::start_action(&mut world, player, Dashing::new());
    let execution = world.get::<ForegroundAction>(player).unwrap().id;
    press(&mut world, player, &[PlayerAction::Skill1]);
    schedule.run(&mut world);
    assert_eq!(world.get::<ForegroundAction>(player).unwrap().id, execution);
    assert_eq!(world.get::<LinearVelocity>(player).unwrap().0, Vec2::new(2.0, 1.0));
    assert_eq!(world.query_filtered::<Entity, With<Mine>>().iter(&world).count(), 1);
}
