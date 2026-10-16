//! Fixed-input fixtures load the authored assets through the same typed resolver.

use super::catalog::*;
use super::action::ActionDefinition;
use bevy::ecs::system::SystemState;
use bevy::prelude::*;
use serde::de::DeserializeOwned;
use std::path::{Path, PathBuf};
use theseeker_engine::assets::{animation::SpriteAnimation, attack::AttackDescriptor, projectile::ProjectileDescriptor, PreloadedAssets};

fn files_in(directory: &Path, suffix: &str) -> Vec<PathBuf> {
    let mut files = Vec::new();
    for entry in std::fs::read_dir(directory).expect("fixture asset directory") {
        let path = entry.expect("fixture directory entry").path();
        collect_asset_file(&path, suffix, &mut files);
    }
    files.sort();
    files
}

fn collect_asset_file(path: &Path, suffix: &str, files: &mut Vec<PathBuf>) {
    if path.is_dir() {
        files.extend(files_in(path, suffix));
    } else if path.to_string_lossy().ends_with(suffix) {
        files.push(path.to_owned());
    }
}

fn read_asset<T: DeserializeOwned>(path: &Path) -> T {
    let text = std::fs::read_to_string(path).expect("fixture asset");
    toml::from_str(&text).unwrap_or_else(|error| panic!("{}: {error}", path.display()))
}

fn load_assets<T: Asset + DeserializeOwned>(directory: &Path, suffix: &str, prefix: &str, preloaded: &mut PreloadedAssets) -> Assets<T> {
    let mut assets = Assets::<T>::default();
    for path in files_in(directory, suffix) {
        let key = format!("{prefix}.{}", path.file_name().unwrap().to_string_lossy().trim_end_matches(suffix));
        preloaded.register_runtime_asset(key, assets.add(read_asset::<T>(&path)));
    }
    assets
}

pub(crate) fn fixture_world() -> World {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("assets");
    let mut world = World::new();
    let mut preloaded = PreloadedAssets::default();
    let mut clips = Assets::<SpriteAnimation>::default();
    for path in files_in(&root.join("animations"), ".anim.toml") {
        load_clip(&path, &mut clips, &mut preloaded);
    }
    world.insert_resource(clips);
    world.insert_resource(load_assets::<AuthoredActor>(&root.join("enemies/archetypes"), ".arch.toml", "arch", &mut preloaded));
    world.insert_resource(load_assets::<ActionDefinition>(&root.join("actions"), ".action.toml", "action", &mut preloaded));
    world.insert_resource(load_assets::<ProjectileDescriptor>(&root.join("projectiles"), ".projectile.toml", "projectile", &mut preloaded));
    world.insert_resource(load_assets::<AttackDescriptor>(&root.join("attacks"), ".attack.toml", "attack", &mut preloaded));
    world.insert_resource(preloaded);
    world
}

fn load_clip(path: &Path, clips: &mut Assets<SpriteAnimation>, preloaded: &mut PreloadedAssets) {
    let text = std::fs::read_to_string(path).expect("animation asset");
    if !text.contains("[asset]") { return; }
    let data: toml::Value = read_asset(path);
    let Some(asset) = data.get("asset") else { return; };
    let keys: Vec<_> = asset.get("key").and_then(toml::Value::as_str).into_iter()
        .chain(asset.get("keys").and_then(toml::Value::as_array).into_iter().flatten().filter_map(toml::Value::as_str)).collect();
    let clip: SpriteAnimation = read_asset(path);
    let handle = clips.add(clip);
    for key in keys { preloaded.register_runtime_asset(key.to_owned(), handle.clone()); }
}

pub(crate) fn fixture_catalog(world: &mut World) -> Result<ActorCatalog, String> {
    let mut state = SystemState::<DefinitionAssets>::new(world);
    let assets = state.get(world).expect("fixture system parameters are valid");
    assets.resolve(world.resource::<PreloadedAssets>())
}

#[test]
fn every_authored_enemy_and_action_resolves() {
    let mut world = fixture_world();
    let catalog = fixture_catalog(&mut world).expect("valid actor catalog");
    assert!(catalog.actors.len() >= 17);
    assert!(catalog.actions.contains_key("player_spit"));
    assert!(catalog.actions.contains_key("player_dive"));
    assert!(catalog.actions.contains_key("tentacle_strike"));
}

fn action_fixture(id: &str) -> super::action::ActiveAction {
    let catalog = fixture_catalog(&mut fixture_world()).expect("valid catalog");
    super::action::ActionRequest { forward: 1.0, actor: Entity::from_raw_u32(1).unwrap(), target: None,
        damage: catalog.actions[id].damage.unwrap_or(0.0), definition: catalog.actions[id].clone() }
        .activate(theseeker_engine::animation::PlaybackId(10))
}

#[test]
fn player_dive_preserves_diagonal_displacement_and_flight_limit() {
    for direction in [-1.0, 1.0] { assert_player_dive_flight(direction); }
}

fn assert_player_dive_flight(direction: f32) {
    use super::lunge::LungeRuntime;
    use super::action::ActionOutcome;
    let mut action = action_fixture("player_dive");
    let motion = action.definition.motion.unwrap();
    action.lunge = Some(LungeRuntime::new(motion, Vec2::new(direction, 0.0), Vec2::ZERO));
    let mut distance = Vec2::ZERO;
    for tick in 0..16 {
        let step = action.lunge.as_ref().unwrap().displacement().0;
        assert_eq!(step, Vec2::new(direction * 4.167, -4.167));
        distance += step;
        let effects = action.advance(tick, None, false);
        assert_eq!(effects.outcome, (tick == 15).then_some(ActionOutcome::Completed));
    }
    assert!((distance - Vec2::new(direction * 66.672, -66.672)).length() < 0.0001);
}

#[test]
fn player_dive_impact_locks_for_eight_ticks_and_cannot_restart() {
    use super::lunge::{LungeRuntime, MotionContact};
    let mut action = action_fixture("player_dive");
    action.lunge = Some(LungeRuntime::new(action.definition.motion.unwrap(), Vec2::X, Vec2::ZERO));
    for tick in 0..14 { assert!(action.advance(tick, None, false).outcome.is_none()); }
    assert!(action.lunge.as_mut().unwrap().impact(MotionContact::Ground));
    for tick in 14..22 {
        assert!(!action.lunge.as_mut().unwrap().impact(MotionContact::Ground));
        assert_eq!(action.lunge.as_ref().unwrap().displacement().0, Vec2::ZERO);
        assert_eq!(action.advance(tick, None, false).outcome.is_some(), tick == 21);
    }
}

#[test]
fn patrol_clip_requests_match_fixed_tick_trace() {
    use super::controller::{ActorController, PatrolPhase};
    let catalog = fixture_catalog(&mut fixture_world()).unwrap();
    let mut actor = ActorController::new(catalog.actors["spider_small"].clone(), 1);
    let mut trace = Vec::new();
    for tick in 0..700 {
        actor.tick_patrol(1.0);
        if actor.patrol_changed && tick > 0 { trace.push((tick, actor.patrol_phase)); }
        actor.entered = false;
    }
    assert_eq!(trace, [(75, PatrolPhase::Walk), (151, PatrolPhase::Walk), (301, PatrolPhase::Idle),
        (377, PatrolPhase::Walk), (453, PatrolPhase::Walk), (603, PatrolPhase::Idle), (679, PatrolPhase::Walk)]);
}

#[test]
fn sprite_shots_have_distinct_release_frames_and_spit_cancellation() {
    use theseeker_engine::animation::{PlaybackState, PlaybackEvent, PlaybackEventKind, PlaybackId, cue_id};
    let catalog = fixture_catalog(&mut fixture_world()).unwrap();
    let player = &catalog.actions["player_spit"];
    let enemy = &catalog.actors["spider_big"].actions["Attack"];
    assert_eq!(player.clip.as_ref().unwrap().events.iter().find(|event| event.emit == "fire_projectile").unwrap().on.frame_enter.unwrap().0, 7);
    assert_eq!(enemy.clip.as_ref().unwrap().events.iter().find(|event| event.emit == "fire_projectile").unwrap().on.frame_enter.unwrap().0, 8);
    let mut action = action_fixture("player_spit");
    let events = |playback, cue: &str| PlaybackState { ordered: vec![PlaybackEvent { playback, kind: PlaybackEventKind::Cue(cue_id(cue)) }], ..PlaybackState::default() };
    assert_eq!(action.advance(0, Some(&events(PlaybackId(9), "fire_projectile")), false).releases, 0);
    assert_eq!(action.advance(1, Some(&events(action.playback, "fire_projectile")), false).releases, 1);
    assert_eq!(action.advance(1, Some(&events(action.playback, "fire_projectile")), false).releases, 0);
    assert!(action.advance(2, None, true).outcome.is_none());
    let cancel_cue = action.definition.timing.cancel.as_deref().unwrap();
    let cancel = events(action.playback, cancel_cue);
    assert_eq!(action.advance(3, Some(&cancel), true).outcome, Some(super::action::ActionOutcome::Interrupted));
    assert_eq!(action.advance(4, Some(&events(action.playback, "fire_projectile")), false).releases, 0);
}

#[test]
fn variants_change_data_and_share_the_mechanic() {
    use super::action::Mechanic;
    let catalog = fixture_catalog(&mut fixture_world()).unwrap();
    let dive = &catalog.actions["player_dive"];
    let glide = &catalog.actions["copied_bat_glide"];
    assert!(matches!((&dive.mechanic, &glide.mechanic), (Mechanic::Lunge, Mechanic::Lunge)));
    assert_ne!(dive.motion.unwrap().displacement(Vec2::X), glide.motion.unwrap().displacement(Vec2::X));
    assert_ne!(dive.presentation.as_ref().unwrap().animation, glide.presentation.as_ref().unwrap().animation);
    assert!(std::mem::discriminant(&catalog.actors["spider_big"].behavior) == std::mem::discriminant(&catalog.actors["spider_big_shortstep"].behavior));
}

#[test]
fn component_removal_cleans_owned_hitboxes_but_preserves_projectiles() {
    use super::ActorAction;
    let mut world = World::new();
    let owned = world.spawn_empty().id();
    let projectile = world.spawn_empty().id();
    let mut action = action_fixture("player_dive");
    action.owned_entities.push(owned);
    let entity = world.spawn(ActorAction(Some(action), super::action::ActionEffects::default())).id();
    world.entity_mut(entity).remove::<ActorAction>();
    world.flush();
    assert!(world.get_entity(owned).is_err());
    assert!(world.get_entity(projectile).is_ok());
    world.entity_mut(entity).remove::<ActorAction>();
    world.flush();
}

#[test]
fn frame_steps_repeat_on_real_entries_with_no_motion_between_them() {
    use theseeker_engine::animation::{PlaybackState, PlaybackEvent, PlaybackEventKind, PlaybackId};
    use theseeker_engine::assets::animation::FrameId;
    let catalog = fixture_catalog(&mut fixture_world()).unwrap();
    let motion = &catalog.actors["spider_big"].walking;
    let mut output = Vec::new();
    let mut distance = 0.0;
    for tick in 0..160 {
        let ordered = if tick % 8 == 0 { vec![PlaybackEvent { playback: PlaybackId(10), kind: PlaybackEventKind::FrameEntered(FrameId((tick / 8) % 10 + 1)) }] } else { Vec::new() };
        let signals = PlaybackState { ordered, ..PlaybackState::default() };
        motion.collect_steps(Some(&signals), PlaybackId(10), 1.0, &mut output);
        let expected = tick % 8 == 0 && [1, 2, 4, 6, 8, 10].contains(&((tick / 8) % 10 + 1));
        assert_eq!(output.len(), usize::from(expected));
        distance += output.iter().map(|step| step.0.x).sum::<f32>();
    }
    assert_eq!(distance, 12.0);
    motion.collect_steps(None, PlaybackId(10), 1.0, &mut output);
    assert!(output.is_empty());
}

#[test]
fn buzzer_burst_releases_three_shots_and_completes_once() {
    use super::action::{ActiveAction, ActionOutcome};
    use theseeker_engine::animation::{PlaybackState, PlaybackEvent, PlaybackEventKind, PlaybackId};
    let catalog = fixture_catalog(&mut fixture_world()).unwrap();
    let mut action = ActiveAction::new(catalog.actors["buzzer"].actions["Attack"].clone(), PlaybackId(1));
    let mut releases = 0;
    for cycle in 0..3 {
        let signals = PlaybackState { ordered: vec![
            PlaybackEvent { playback: action.playback, kind: PlaybackEventKind::Cue(action.definition.cues.release.unwrap()) },
            PlaybackEvent { playback: action.playback, kind: PlaybackEventKind::Completed },
        ], ..PlaybackState::default() };
        let effects = action.advance(cycle, Some(&signals), false);
        releases += effects.releases;
        assert_eq!(effects.restart_clip, cycle < 2);
        assert_eq!(effects.outcome, (cycle == 2).then_some(ActionOutcome::Completed));
        action.playback.0 += 1;
    }
    assert_eq!(releases, 3);
}

#[test]
fn every_controller_family_obeys_its_decision_and_commitment_rules() {
    use super::controller::{ActorController, Activity, decide, DecisionInput};
    let catalog = fixture_catalog(&mut fixture_world()).unwrap();
    for id in catalog.actors.keys() {
        let actor = ActorController::new(catalog.actors[id].clone(), 1);
        assert_eq!(decide(&actor, DecisionInput { dead: true, ..DecisionInput::default() }), Activity::Death);
    }
    let mut bat = ActorController::new(catalog.actors["bat_small"].clone(), 1);
    assert_eq!(decide(&bat, DecisionInput { in_vision: true, ..DecisionInput::default() }), Activity::Approach);
    bat.transition(Activity::Attack);
    assert_eq!(decide(&bat, DecisionInput { outside_vision: true, ..DecisionInput::default() }), Activity::Attack);
    assert_eq!(decide(&bat, DecisionInput { action_done: true, ..DecisionInput::default() }), Activity::Recover);
    let observer = ActorController::new(catalog.actors["observer_laser"].clone(), 1);
    assert_eq!(decide(&observer, DecisionInput { in_melee: true, ..DecisionInput::default() }), Activity::Patrol);
    let mut pursuer = ActorController::new(catalog.actors["void"].clone(), 1);
    assert_eq!(decide(&pursuer, DecisionInput { in_vision: true, ..DecisionInput::default() }), Activity::Approach);
    pursuer.transition(Activity::Approach);
    assert_eq!(decide(&pursuer, DecisionInput { in_melee: true, ..DecisionInput::default() }), Activity::Approach);
    assert_eq!(decide(&pursuer, DecisionInput { outside_vision: true, ..DecisionInput::default() }), Activity::Patrol);
}

#[test]
fn ambush_direction_and_confirmed_kill_select_the_followup() {
    use super::controller::{ActorController, Activity, decide, DecisionInput};
    let catalog = fixture_catalog(&mut fixture_world()).unwrap();
    let mut actor = ActorController::new(catalog.actors["opi"].clone(), 1);
    actor.observations.detected_right = true;
    assert_eq!(decide(&actor, DecisionInput::default()), Activity::Prepare);
    actor.transition(Activity::Prepare);
    assert_eq!(decide(&actor, DecisionInput { prepare_done: true, ..DecisionInput::default() }), Activity::AttackRight);
    actor.transition(Activity::AttackRight);
    assert_eq!(decide(&actor, DecisionInput { attack_done: true, ..DecisionInput::default() }), Activity::Idle);
    actor.observations.killed_target = true;
    assert_eq!(decide(&actor, DecisionInput { attack_done: true, ..DecisionInput::default() }), Activity::EatRight);
    actor.transition(Activity::EatRight);
    assert_eq!(decide(&actor, DecisionInput { eat_done: true, ..DecisionInput::default() }), Activity::Idle);
}

#[test]
fn hopper_anger_air_jump_and_landing_memory_are_explicit() {
    use super::controller::{ActorController, Activity, decide, DecisionInput};
    let catalog = fixture_catalog(&mut fixture_world()).unwrap();
    let mut actor = ActorController::new(catalog.actors["squid"].clone(), 1);
    actor.hopper.anger_active = true;
    assert!(actor.hopper.enter_idle(&mut actor.rng_state, [143, 271], Some(Vec2::new(20.0, 0.0)), Vec2::ZERO, 200.0));
    assert_eq!(actor.hopper.direction(&mut actor.rng_state, Some(-20.0)), -1.0);
    actor.transition(Activity::Fall);
    actor.observations.air_jump = true;
    assert_eq!(decide(&actor, DecisionInput { grounded: true, ..DecisionInput::default() }), Activity::AirJump);
    assert!(!actor.hopper.try_air_jump(&mut actor.rng_state, 11));
    let _ = actor.hopper.try_air_jump(&mut actor.rng_state, 12);
    assert!(!actor.hopper.try_air_jump(&mut actor.rng_state, 13));
    assert!(!actor.hopper.enter_idle(&mut actor.rng_state, [143, 271], Some(Vec2::new(201.0, 0.0)), Vec2::ZERO, 200.0));
    assert!(!actor.hopper.anger_active);
    assert!(!actor.hopper.air_jump_checked);
}

#[test]
fn active_definitions_pin_timing_and_invalid_edits_preserve_the_catalog() {
    let mut world = fixture_world();
    world.insert_resource(ActorCatalog::default());
    let mut schedule = Schedule::default();
    schedule.add_systems(super::catalog::resolve_catalog);
    schedule.run(&mut world);
    let snapshot = world.resource::<ActorCatalog>().actions["player_dive"].clone();
    let generation = world.resource::<ActorCatalog>().generation;
    let mut assets = world.resource_mut::<Assets<ActionDefinition>>();
    let (_, action) = assets.iter_mut().find(|(_, action)| action.id == "player_dive").unwrap();
    action.motion.as_mut().unwrap().speed = 6.0;
    schedule.run(&mut world);
    assert_eq!(snapshot.motion.unwrap().speed, 4.167);
    assert_eq!(world.resource::<ActorCatalog>().actions["player_dive"].motion.unwrap().speed, 6.0);
    let mut assets = world.resource_mut::<Assets<ActionDefinition>>();
    let (_, action) = assets.iter_mut().find(|(_, action)| action.id == "player_dive").unwrap();
    action.timing.as_mut().unwrap().release = Some("nonexistent_cue".to_owned());
    schedule.run(&mut world);
    assert_eq!(world.resource::<ActorCatalog>().generation, generation + 1);
    assert_eq!(world.resource::<ActorCatalog>().actions["player_dive"].motion.unwrap().speed, 6.0);
}

#[test]
fn procedural_strike_uses_tick_phases_and_an_inclusive_hit_window() {
    let action = action_fixture("tentacle_strike");
    let super::action::Mechanic::TentacleStrike { phases } = action.definition.mechanic else { panic!("procedural strike"); };
    assert_eq!(phases.duration(), 72);
    let active: Vec<_> = (0..phases.duration()).filter(|age| phases.hit_active(*age)).collect();
    assert_eq!(active, (28..=46).collect::<Vec<_>>());
    assert_eq!(phases.phase(25), super::tick_strike::StrikePhase::Windup);
    assert_eq!(phases.phase(26), super::tick_strike::StrikePhase::Strike(0.0));
    assert_eq!(phases.phase(40), super::tick_strike::StrikePhase::Recover(0.0));
}

#[test]
fn motion_ownership_only_replaces_claimed_axes() {
    let catalog = fixture_catalog(&mut fixture_world()).unwrap();
    let action = super::action::ActiveAction::new(catalog.actors["spider_big"].actions["Attack"].clone(), theseeker_engine::animation::PlaybackId(1));
    let mut velocity = theseeker_engine::physics::LinearVelocity(Vec2::new(2.0, -3.0));
    let intent = action.motion_intent(super::action::TickDisplacement::default());
    intent.apply(action.id + 1, &mut velocity);
    assert_eq!(velocity.0, Vec2::new(2.0, -3.0));
    intent.apply(action.id, &mut velocity);
    assert_eq!(velocity.0, Vec2::new(0.0, -3.0));
}

#[test]
fn projectile_timing_is_pinned_and_required_controller_bindings_are_validated() {
    let mut world = fixture_world();
    let catalog = fixture_catalog(&mut world).unwrap();
    let original = catalog.actors["buzzer"].actions["Attack"].projectile_clips.as_ref().unwrap().pop.clone();
    let projectile = catalog.actors["buzzer"].actions["Attack"].projectile.as_ref().unwrap();
    let key = &projectile.sprite.as_ref().unwrap().pop_anim_key;
    let handle = world.resource::<PreloadedAssets>().get_single_asset::<SpriteAnimation>(key).unwrap();
    let duration = original.settings.extended.ticks_per_frame;
    world.resource_mut::<Assets<SpriteAnimation>>().get_mut(&handle).unwrap().settings.extended.ticks_per_frame += 1;
    let edited = fixture_catalog(&mut world).unwrap();
    assert_eq!(original.settings.extended.ticks_per_frame, duration);
    assert_eq!(edited.actors["buzzer"].actions["Attack"].projectile_clips.as_ref().unwrap().pop.settings.extended.ticks_per_frame, duration + 1);
    let mut sources = world.resource_mut::<Assets<AuthoredActor>>();
    let (_, source) = sources.iter_mut().find(|(_, source)| source.0.id == "buzzer").unwrap();
    source.0.actions.remove("Attack");
    let error = fixture_catalog(&mut world).err().expect("missing binding rejected");
    assert!(error.contains("buzzer: controller requires action binding Attack"), "{error}");
}
