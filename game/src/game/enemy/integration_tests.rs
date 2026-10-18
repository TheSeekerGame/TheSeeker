//! Exercise normal actor assembly, decisions, actuation, and body motors together.

use super::*;
use crate::game::actors::{self, ActorAction, LevelSeed};
use bevy::ecs::system::{RunSystemOnce, SystemState};
use theseeker_engine::time::{GameTickPre, GameTickPost, MasterClock, run_gametickupdate_schedule};

fn actor_world() -> (World, Vec<Entity>) {
    let mut world = actors::fixtures::fixture_world();
    let catalog = actors::fixtures::fixture_catalog(&mut world).unwrap();
    let mut definitions: Vec<_> = catalog.actors.values().cloned().collect();
    definitions.sort_by(|a, b| a.archetype.id.cmp(&b.archetype.id));
    world.insert_resource(catalog);
    world.insert_resource(LevelSeed(123));
    world.insert_resource(GameTime::default());
    world.insert_resource(ArcParticleEffectHandle(Handle::default()));
    world.init_resource::<Messages<DamageInfo>>();
    world.spawn((Player, Transform::from_xyz(50.0, 10.0, 0.0), GlobalTransform::default(), Health { current: 10_000, max: 10_000 }));
    let mut state = SystemState::<Commands>::new(&mut world);
    let mut commands = state.get_mut(&mut world).expect("fixture system parameters are valid");
    let entities = definitions.iter().enumerate().map(|(index, definition)| {
        spawn_enemy_actor_blueprint(&mut commands, &definition.archetype.id,
            Transform::from_xyz(index as f32 * 80.0, 40.0, 0.0))
    }).collect();
    state.apply(&mut world);
    world.run_system_once(setup_enemy_actor).unwrap();
    (world, entities)
}

fn actor_schedule(world: &mut World) {
    world.add_schedule(Schedule::new(GameTickPre));
    world.add_schedule(Schedule::new(GameTickPost));
    let mut schedule = Schedule::new(GameTickUpdate);
    schedule.add_systems((actors::advance_actor_actions, actors::actor_decisions,
        enemy_actor_actuator, ground_motion_motor, flying_motor_system,
        jump_motor_system, stationary_trap_controller, actuation::finish_actor_tick).chain());
    world.add_schedule(schedule);
}

#[test]
fn every_actor_assembles_and_runs_decision_action_motor_schedule() {
    let (mut world, entities) = actor_world();
    assert_eq!(entities.len(), world.resource::<actors::catalog::ActorCatalog>().actors.len());
    for &entity in &entities {
        assert!(world.get::<Enemy>(entity).is_some());
        assert!(world.get::<ActorController>(entity).is_some());
        assert!(world.get::<ActorAction>(entity).is_some());
        assert_body_capabilities(&world, entity);
    }
    actor_schedule(&mut world);
    let clock = MasterClock { tick: 700, ..Default::default() };
    world.resource_mut::<GameTime>().update_from_master(&clock);
    run_gametickupdate_schedule(&mut world);
    for entity in entities {
        let velocity = world.get::<LinearVelocity>(entity).unwrap().0;
        assert!(velocity.is_finite(), "invalid velocity on {entity:?}");
        assert!(!world.get::<ActorController>(entity).unwrap().entered);
    }
}

fn assert_body_capabilities(world: &World, entity: Entity) {
    let actor = world.get::<ActorController>(entity).unwrap();
    let setup = actor.definition.archetype.setup.as_ref().unwrap();
    assert_eq!(world.get::<trap_controller::TrapPrey>(entity).is_some(), setup.trap_prey.unwrap_or(false));
    let multipart = setup.gfx.as_ref().is_some_and(|gfx| matches!(gfx.kind, Some(actors::definition::EnemyGfxKind::VoidSplit)));
    assert_eq!(world.get::<flying_motor::OrientToVelocity>(entity).is_some(), multipart);
}

#[test]
fn void_pursues_the_player_and_contact_damage_rearms_after_its_cooldown() {
    use crate::game::actors::PerceivedTargetSensor;
    use theseeker_engine::physics::{EntityCollisionWorld, groups, rebuild_entity_collision_world};

    let (mut world, entities) = actor_world();
    let pursuer = *entities.iter().find(|&&entity| {
        world.get::<ActorController>(entity).unwrap().definition.archetype.id == "void"
    }).unwrap();
    for entity in entities.into_iter().filter(|&entity| entity != pursuer) { world.despawn(entity); }
    let player = world.query_filtered::<Entity, With<Player>>().single(&world).unwrap();
    world.entity_mut(player).insert((Transform::from_xyz(30.0, 10.0, 0.0),
        GlobalTransform::from_xyz(30.0, 10.0, 0.0), Collider::cuboid(2.0, 5.0), groups::player_body()));
    world.entity_mut(pursuer).insert((Transform::from_xyz(0.0, 0.0, 0.0),
        GlobalTransform::from_xyz(0.0, 0.0, 0.0),
        TargetSensor { entity: Some(player), dist2: 1000.0 },
        PerceivedTargetSensor { entity: Some(player), dist2: 1000.0 }));
    actor_schedule(&mut world);
    world.resource_mut::<GameTime>().update_from_master(&MasterClock { tick: 1, ..default() });
    run_gametickupdate_schedule(&mut world);
    assert_eq!(world.get::<ActorController>(pursuer).unwrap().activity, actors::controller::Activity::Approach);
    let velocity = world.get::<LinearVelocity>(pursuer).unwrap().0;
    assert!(velocity.dot(Vec2::new(30.0, 10.0)) > 0.0);
    assert!((velocity.length() - 0.72).abs() < 0.0001);
    assert!(world.get::<ActorAction>(pursuer).unwrap().0.is_none(), "pursuit uses contact combat without a timeline attack");

    world.entity_mut(player).insert((Transform::default(), GlobalTransform::default()));
    world.insert_resource(EntityCollisionWorld::default());
    world.run_system_once(rebuild_entity_collision_world).unwrap();
    world.run_system_once(contact_damage_system).unwrap();
    assert_eq!(world.get::<Health>(player).unwrap().current, 9968);
    assert!(world.get::<Knockback>(player).is_some());
    for _ in 0..47 { world.run_system_once(contact_damage_system).unwrap(); }
    assert_eq!(world.get::<Health>(player).unwrap().current, 9968);
    world.run_system_once(contact_damage_system).unwrap();
    assert_eq!(world.get::<Health>(player).unwrap().current, 9936);
}

#[test]
fn dying_mid_action_keeps_the_resting_sprite_anchor() {
    use theseeker_engine::{animation::PlaybackId, gent::TransformGfxFromGent};

    let (mut world, entities) = actor_world();
    let spider = *entities.iter().find(|&&entity| {
        world.get::<ActorController>(entity).unwrap().definition.archetype.id == "spider_small"
    }).unwrap();
    let resting = world.get::<actors::ActorDefinition>(spider).unwrap().0.body.gfx_offset;
    assert!(resting.y > 0.0, "the small spider's sprite rests above its body origin");
    let gfx = world.get::<Gent>(spider).unwrap().e_gfx;
    let attack = world.get::<ActorController>(spider).unwrap().definition.actions["Attack"].clone();
    let action = actors::action::ActiveAction::new(attack, PlaybackId(1));
    world.get_mut::<ScriptPlayer<SpriteAnimation>>(gfx).unwrap().acquire_presentation(action.id);
    world.entity_mut(spider).insert(ActorAction(Some(action), actors::action::ActionEffects::default()));

    // Death strips the action slot while the attack still owns the sprite.
    world.entity_mut(spider).insert(Dead::default()).remove::<ActorAction>();
    world.flush();

    assert_eq!(world.get::<TransformGfxFromGent>(gfx).unwrap().offset, Some(resting));
}

#[test]
fn optional_actor_cleanup_tolerates_an_earlier_queued_hierarchy_despawn() {
    let mut app = App::new();
    app.set_error_handler(bevy::ecs::error::panic);
    let owner = app.world_mut().spawn_empty().id();
    let visual = app.world_mut().spawn(ChildOf(owner)).id();
    app.add_systems(Update, move |mut commands: Commands| {
        commands.entity(owner).despawn();
        despawn_entity_if_exists(&mut commands, visual);
        despawn_entity_if_exists(&mut commands, owner);
    });
    app.update();
    assert!(app.world().get_entity(owner).is_err());
    assert!(app.world().get_entity(visual).is_err());
}

/// A squid hopping in a walled room toward the player, angered partway by the player's hit,
/// takes the same path tick for tick: its motor's tuned hops, falls, landings, and air jumps
/// are pinned by a fingerprint of every tick's position, velocity, navigation, and observations.
#[test]
fn squid_hops_the_same_path_through_a_room() {
    const TICKS: u64 = 2000;
    const HIT_AT: u64 = 700;
    let (mut world, entities) = actor_world();
    let squid = *entities.iter().find(|&&entity| {
        world.get::<ActorController>(entity).unwrap().definition.archetype.id == "squid"
    }).expect("the fixture catalog has a squid");
    for &entity in entities.iter().filter(|&&entity| entity != squid) {
        world.despawn(entity);
    }
    let player = world.query_filtered::<Entity, With<Player>>().single(&world).unwrap();
    world.get_mut::<Transform>(player).unwrap().translation = Vec3::new(330.0, 12.0, 0.0);
    world.get_mut::<Transform>(squid).unwrap().translation = Vec3::new(150.0, 20.0, 0.0);
    // A 60 by 6 room of 8-unit tiles: floor, walls, and a ceiling low enough to cut hops short.
    let (width, height) = (60, 6);
    let terrain: Vec<bool> = (0..width * height)
        .map(|i| i / width == 0 || i / width == height - 1 || i % width == 0 || i % width == width - 1)
        .collect();
    let mut tiles = TileCollisionWorld::default();
    tiles.rebuild(width as u32, height as u32, 8.0, &terrain);
    world.insert_resource(tiles);
    world.add_schedule(Schedule::new(GameTickPre));
    world.add_schedule(Schedule::new(GameTickPost));
    let mut schedule = Schedule::new(GameTickUpdate);
    schedule.add_systems((actors::sensors::sensor_ground::<Navigation>, actors::advance_actor_actions,
        actors::actor_decisions, enemy_actor_actuator, jump_motor_system, actuation::finish_actor_tick,
        jump_motor_anger_trigger, jump_motor_move_collide).chain());
    world.add_schedule(schedule);

    // FNV-1a over each tick's state.
    let mut fingerprint: u64 = 0xcbf2_9ce4_8422_2325;
    let (mut jumped, mut landed, mut air_jumped, mut angered, mut bumped) = (false, false, false, false, false);
    // How high the squid can rise under the ceiling.
    let ceiling = (height - 1) as f32 * 8.0 - world.get::<Collider>(squid).unwrap().half_extents().y;
    for tick in 1..=TICKS {
        if tick == HIT_AT {
            world.resource_mut::<Messages<DamageInfo>>().write(DamageInfo {
                owner: player, source: player, target: squid, amount: 1.0, crit: false, stealthed: false, backstab: false,
            });
        }
        world.resource_mut::<GameTime>().update_from_master(&MasterClock { tick, ..Default::default() });
        run_gametickupdate_schedule(&mut world);
        let position = world.get::<Transform>(squid).unwrap().translation.truncate();
        let velocity = world.get::<LinearVelocity>(squid).unwrap().0;
        let navigation = world.get::<Navigation>(squid).unwrap();
        let controller = world.get::<ActorController>(squid).unwrap();
        let observations = controller.observations;
        let state = format!("{:?} {:?} {navigation:?} {:?} {} {} {} {}", position.to_array().map(f32::to_bits),
            velocity.to_array().map(f32::to_bits), controller.activity, observations.idle_ready,
            observations.jump_done, observations.air_jump, controller.hopper.anger_active);
        for byte in state.bytes() {
            fingerprint = (fingerprint ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3);
        }
        jumped |= matches!(navigation, Navigation::Falling { jumping: true, .. });
        landed |= jumped && matches!(navigation, Navigation::Grounded);
        air_jumped |= observations.air_jump;
        angered |= controller.hopper.anger_active;
        bumped |= position.y > ceiling - 2.0;
        assert!(tick >= HIT_AT || !controller.hopper.anger_active, "only the hit angers the squid");
    }
    assert!(jumped && landed && air_jumped && angered && bumped, "the squid jumps, lands, air jumps, angers, and reaches the ceiling");
    assert_eq!(fingerprint, 0x16ea_0aa1_b780_3abc);
}
