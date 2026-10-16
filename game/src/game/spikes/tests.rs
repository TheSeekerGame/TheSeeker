use super::*;
use crate::game::gentstate::Facing;
use crate::game::player::foreground::{ForegroundAction, start_action};
use crate::game::player::passives::PassiveEvent;
use crate::game::player::states::{Whirling, Attacking, AttackVariant};
use crate::game::player::weapon::{PlayerMeleeWeapon, WeaponType};
use theseeker_engine::assets::animation::SpriteAnimation;
use theseeker_engine::physics::{EntityCollisionWorld, StaticCollider, rebuild_entity_collision_world};
use theseeker_engine::script::ScriptPlayer;

fn fixture() -> (World, Entity) {
    let mut world = World::new();
    world.insert_resource(GameTime::default());
    world.insert_resource(EntityCollisionWorld::default());
    world.insert_resource(Messages::<DamageInfo>::default());
    world.insert_resource(Messages::<PassiveEvent>::default());
    let mut terrain = vec![false; 64 * 32];
    terrain[64 * 31..].fill(true);
    let mut tiles = theseeker_engine::physics::TileCollisionWorld::default();
    tiles.rebuild(64, 32, 1.0, &terrain);
    world.insert_resource(tiles);
    let player = add_player(&mut world);
    (world, player)
}

fn add_player(world: &mut World) -> Entity {
    let gfx = world.spawn((Visibility::Visible, ScriptPlayer::<SpriteAnimation>::default())).id();
    let player = world.spawn((Player, Gent { e_gfx: gfx, e_effects_gfx: gfx },
        Transform::from_xyz(6.0, 3.0, 0.0), GlobalTransform::from_xyz(6.0, 3.0, 0.0),
        Collider::cuboid(1.0, 2.0), LinearVelocity::default(), groups::player_body(),
        Health { current: 100, max: 100 }, Facing::Right, CapsuleDropPlatformState::default(),
        SafeGroundedPosition { position: Vec2::new(6.0, 3.0), space: None }, InputBuffer::default())).id();
    start_action(world, player, Ready);
    player
}

fn spike(world: &mut World, position: Vec2) -> Entity {
    let entity = world.spawn((SpikeHazard, StaticCollider, Transform::from_translation(position.extend(0.0)),
        GlobalTransform::from_translation(position.extend(0.0)), Collider::cuboid(0.5, 0.5),
        groups::enemy_attack())).id();
    world.entity_mut(entity).insert(DamageSource::new(u32::MAX, entity, SPIKE_DAMAGE).with_max_targets(1));
    entity
}

fn damage_schedule() -> Schedule {
    let mut schedule = Schedule::default();
    schedule.add_systems((rebuild_entity_collision_world, prepare_spike_contacts,
        combat::damage_source::determine_damage_targets, combat::damage_source::apply_damage).chain());
    schedule
}

fn hit(world: &mut World, source: Entity, target: Entity, amount: f32) {
    world.resource_mut::<Messages<DamageInfo>>().write(DamageInfo {
        owner: source, source, target, amount, crit: false, stealthed: false, backstab: false,
    });
}

#[test]
fn persistent_spikes_share_immunity_and_hit_again_after_its_window() {
    let (mut world, player) = fixture();
    spike(&mut world, Vec2::new(6.0, 1.5));
    spike(&mut world, Vec2::new(6.5, 1.5));
    let mut schedule = damage_schedule();
    schedule.run(&mut world);
    assert_eq!(world.get::<Health>(player).unwrap().current, 80);
    for _ in 1..SpikeDamageCooldown::DURATION_TICKS { schedule.run(&mut world); }
    assert_eq!(world.get::<Health>(player).unwrap().current, 80);
    schedule.run(&mut world);
    assert_eq!(world.get::<Health>(player).unwrap().current, 60);
    world.despawn(player);
    let replacement = add_player(&mut world);
    schedule.run(&mut world);
    assert_eq!(world.get::<Health>(replacement).unwrap().current, 80);
}

#[test]
fn accepted_spike_hit_interrupts_owned_action_and_recovers_after_the_delay() {
    let (mut world, player) = fixture();
    let source = spike(&mut world, Vec2::new(20.0, 1.5));
    start_action(&mut world, player, Whirling::new(PlayerMeleeWeapon::Sword));
    let hitbox = world.spawn_empty().id();
    world.get_mut::<ForegroundAction>(player).unwrap().own(hitbox);
    world.get_mut::<Transform>(player).unwrap().translation = Vec3::new(20.0, 5.0, 0.0);
    hit(&mut world, source, player, 20.0);
    let mut process = Schedule::default();
    process.add_systems(process_spike_hits);
    process.run(&mut world);
    assert!(world.get_entity(hitbox).is_err());
    start_action(&mut world, player, Attacking::new(AttackVariant::Forward, WeaponType::Sword));
    assert!(world.get::<ForegroundAction>(player).unwrap().state::<Ready>().is_some());
    let mut advance = Schedule::default();
    advance.add_systems(advance_spike_respawns);
    for _ in 1..SPIKE_RESPAWN_DELAY_TICKS { advance.run(&mut world); }
    assert!(world.get::<SpikeRespawnPending>(player).is_some());
    assert_eq!(world.get::<Transform>(player).unwrap().translation.xy(), Vec2::new(20.0, 5.0));
    advance.run(&mut world);
    assert!(world.get::<SpikeRespawnPending>(player).is_none());
    assert_eq!(world.get::<Transform>(player).unwrap().translation.xy(), Vec2::new(6.0, 3.0));
    assert_eq!(*world.get::<CollisionGroups>(player).unwrap(), groups::player_body());
    assert!(world.get::<PresentationJump>(player).is_some());
}

#[test]
fn zero_damage_does_not_start_recovery_and_cancellation_restores_overrides() {
    let (mut world, player) = fixture();
    let source = spike(&mut world, Vec2::new(20.0, 1.5));
    hit(&mut world, source, player, 0.0);
    let mut process = Schedule::default();
    process.add_systems(process_spike_hits);
    process.run(&mut world);
    assert!(world.get::<SpikeRespawnPending>(player).is_none());
    hit(&mut world, source, player, 20.0);
    process.run(&mut world);
    let gfx = world.get::<Gent>(player).unwrap().e_gfx;
    assert_eq!(*world.get::<Visibility>(gfx).unwrap(), Visibility::Hidden);
    world.entity_mut(player).remove::<SpikeRespawnPending>();
    world.flush();
    assert_eq!(*world.get::<Visibility>(gfx).unwrap(), Visibility::Visible);
    assert_eq!(*world.get::<CollisionGroups>(player).unwrap(), groups::player_body());
}

#[test]
fn recovery_revalidates_newly_activated_hazards() {
    let (mut world, player) = fixture();
    let source = spike(&mut world, Vec2::new(20.0, 1.5));
    hit(&mut world, source, player, 20.0);
    let mut process = Schedule::default();
    process.add_systems(process_spike_hits);
    process.run(&mut world);
    spike(&mut world, Vec2::new(6.0, 1.5));
    let mut index = Schedule::default();
    index.add_systems(rebuild_entity_collision_world);
    index.run(&mut world);
    world.get_mut::<SpikeRespawnPending>(player).unwrap().ticks_remaining = 0;
    let mut advance = Schedule::default();
    advance.add_systems(advance_spike_respawns);
    advance.run(&mut world);
    let destination = world.get::<Transform>(player).unwrap().translation.xy();
    assert_ne!(destination, Vec2::new(6.0, 3.0));
    assert_eq!(destination.y, 3.0);
    assert!(world.get::<SpikeRespawnPending>(player).is_none());
}

#[test]
fn first_recovery_resolves_support_below_an_airborne_spawn() {
    let (mut world, player) = fixture();
    world.entity_mut(player).remove::<SafeGroundedPosition>();
    world.insert_resource(CompiledLevel { spawns: vec![crate::levelgen::SpawnSpec {
        kind: SpawnKind::Player, position: Vec2::new(8.0, 12.0), chunk: IVec2::ZERO,
    }], ..default() });
    let source = spike(&mut world, Vec2::new(20.0, 1.5));
    hit(&mut world, source, player, 20.0);
    let mut process = Schedule::default();
    process.add_systems(process_spike_hits);
    process.run(&mut world);
    world.get_mut::<SpikeRespawnPending>(player).unwrap().ticks_remaining = 0;
    let mut advance = Schedule::default();
    advance.add_systems(advance_spike_respawns);
    advance.run(&mut world);
    assert_eq!(world.get::<Transform>(player).unwrap().translation.xy(), Vec2::new(8.0, 3.0));
}

#[test]
fn recovery_reconciles_capsule_occupancy_on_both_decks_and_safe_fallbacks() {
    use crate::game::world_objects::{Capsule, CapsuleCollisionMask};

    for deck in 0..2 {
        for anchor_height in [0.0, 10.0] {
            let (mut world, player) = fixture();
            let capsule_position = Vec2::new(32.0, 40.0);
            let mask = CapsuleCollisionMask::default();
            let platform = mask.platforms[deck].rect;
            let destination = mask.world_origin(capsule_position)
                + Vec2::new(platform.center().x, platform.max.y + 2.0);
            let shelter = world.spawn((Capsule, mask,
                Transform::from_translation(capsule_position.extend(0.0)))).id();
            world.get_mut::<SafeGroundedPosition>(player).unwrap().position = destination + Vec2::Y * anchor_height;
            let source = spike(&mut world, Vec2::new(20.0, 1.5));
            hit(&mut world, source, player, SPIKE_DAMAGE);
            let mut process = Schedule::default();
            process.add_systems(process_spike_hits);
            process.run(&mut world);
            world.get_mut::<SpikeRespawnPending>(player).unwrap().ticks_remaining = 0;
            let mut advance = Schedule::default();
            advance.add_systems(advance_spike_respawns);
            advance.run(&mut world);
            assert!(world.get::<SpikeRespawnPending>(player).is_none());
            assert_eq!(world.get::<Transform>(player).unwrap().translation.xy(), destination);
            assert_eq!(world.get::<CapsuleOccupant>(player).unwrap().shelter, shelter,
                "a recovered cabin anchor restores ownership without crossing the entry trigger");

            world.get_mut::<SafeGroundedPosition>(player).unwrap().position = Vec2::new(6.0, 3.0);
            hit(&mut world, source, player, SPIKE_DAMAGE);
            process.run(&mut world);
            world.get_mut::<SpikeRespawnPending>(player).unwrap().ticks_remaining = 0;
            advance.run(&mut world);
            assert_eq!(world.get::<Transform>(player).unwrap().translation.xy(), Vec2::new(6.0, 3.0));
            assert!(world.get::<CapsuleOccupant>(player).is_none(),
                "recovering to exterior support clears cabin ownership");
        }
    }
}
