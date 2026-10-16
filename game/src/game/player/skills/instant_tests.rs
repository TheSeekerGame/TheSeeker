use super::*;
use bevy::ecs::system::SystemState;
use crate::game::player::CraftMaterial;
use theseeker_engine::physics::EntityCollisionWorld;

fn fixture() -> (World, Entity) {
    let mut world = World::new();
    world.init_resource::<Cooldowns>();
    world.init_resource::<EntityCollisionWorld>();
    let entity = world.spawn(MaterialInventory::empty()).id();
    (world, entity)
}

fn activate(world: &mut World, entity: Entity, skill: SkillId, origin: ActivationOrigin) -> InstantResult {
    let mut state = SystemState::<(InstantWorld, Query<&mut MaterialInventory>)>::new(world);
    let (mut effects, mut inventories) = state.get_mut(world).expect("fixture system parameters are valid");
    let mut materials = inventories.get_mut(entity).unwrap();
    let user = InstantUser { entity, transform: &Transform::default(), facing: &Facing::Right,
        ground: &GroundSensor { is_grounded: true, distance: 0.0 }, tick: 1, cdr: 1.0, origin };
    let result = effects.activate(skill, &user, Some(&mut materials));
    state.apply(world);
    result
}

#[test]
fn pulse_drive_stealth_ignores_admission_and_retains_damage_source() {
    let (mut world, owner) = fixture();
    world.resource_mut::<Cooldowns>().start(owner, SkillId::Stealth, stealth_metadata().cooldown, 1.0, 1);
    assert!(activate(&mut world, owner, SkillId::Stealth, ActivationOrigin::Input) == InstantResult::Rejected);
    let source = world.spawn_empty().id();
    let origin = ActivationOrigin::PulseDrive { source_damage: Some(source) };
    assert!(activate(&mut world, owner, SkillId::Stealth, origin) == InstantResult::Used);
    assert_eq!(world.get::<StealthEffect>(owner).unwrap().ignore_damage_from, Some(source));
}

#[test]
fn failed_placement_and_missing_stock_do_not_pay_or_start_cooldown() {
    let (mut world, owner) = fixture();
    world.get_mut::<MaterialInventory>(owner).unwrap().add_to_backpack(CraftMaterial::AmplifiedBell, 2);
    assert!(activate(&mut world, owner, SkillId::AmplifiedBell, ActivationOrigin::Input) == InstantResult::InvalidPlacement);
    assert_eq!(world.get::<MaterialInventory>(owner).unwrap().backpack_count(CraftMaterial::AmplifiedBell), 2);
    assert!(activate(&mut world, owner, SkillId::Spinner, ActivationOrigin::Input) == InstantResult::Rejected);
    assert!(world.resource::<Cooldowns>().is_ready(owner, SkillId::AmplifiedBell, 1));
    assert!(world.resource::<Cooldowns>().is_ready(owner, SkillId::Spinner, 1));
}

#[test]
fn pylon_pair_starts_cooldown_after_second_committed_spawn() {
    let (mut world, owner) = fixture();
    world.get_mut::<MaterialInventory>(owner).unwrap().add_to_backpack(CraftMaterial::QuarkPylon, 3);
    assert!(activate(&mut world, owner, SkillId::QuarkPylon, ActivationOrigin::Input) == InstantResult::Used);
    assert!(world.resource::<Cooldowns>().is_ready(owner, SkillId::QuarkPylon, 1));
    let origin = ActivationOrigin::PulseDrive { source_damage: None };
    assert!(activate(&mut world, owner, SkillId::QuarkPylon, origin) == InstantResult::Used);
    assert!(!world.resource::<Cooldowns>().is_ready(owner, SkillId::QuarkPylon, 1));
    assert!(activate(&mut world, owner, SkillId::QuarkPylon, origin) == InstantResult::Rejected);
    assert_eq!(world.get::<MaterialInventory>(owner).unwrap().backpack_count(CraftMaterial::QuarkPylon), 1);
    assert_eq!(world.query::<&spawns::quark_pylon::QuarkPylon>().iter(&world).count(), 2);
}
