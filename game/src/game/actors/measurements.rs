//! Repeatable development-build measurements of the ECS decision/action stage.

use super::*;
use std::time::Instant;
use theseeker_engine::animation::{PlaybackId, PlaybackEvent, PlaybackEventKind, PlaybackState};
use theseeker_engine::time::{GameTime, GameTickPre, GameTickPost, MasterClock, run_gametickupdate_schedule};

#[test]
#[ignore = "Run explicitly to measure populations without concurrent test workloads"]
fn controller_action_population_cost() {
    for count in [500, 1_000] { measure_population(count); }
}

fn measure_population(count: usize) {
    let mut world = fixtures::fixture_world();
    let catalog = fixtures::fixture_catalog(&mut world).unwrap();
    let mut definitions: Vec<_> = catalog.actors.values().cloned().collect();
    definitions.sort_by(|a, b| a.archetype.id.cmp(&b.archetype.id));
    let shot = catalog.actions["player_spit"].clone();
    for index in 0..count {
        world.spawn((ActorController::new(definitions[index % definitions.len()].clone(), index as u32),
            ActorAction(Some(action::ActiveAction::new(shot.clone(), PlaybackId(1))), action::ActionEffects::default()),
            PerceivedTargetSensor { entity: Some(Entity::from_raw_u32(1).unwrap()), dist2: 10_000.0 },
            PerceivedGroundSensor { on: true }, HealthSensor::default(), PlaybackState::default()));
    }
    world.insert_resource(GameTime::default());
    world.add_schedule(Schedule::new(GameTickPre));
    world.add_schedule(Schedule::new(GameTickPost));
    let mut schedule = Schedule::new(GameTickUpdate);
    schedule.add_systems((prepare_playback_fixture, advance_actor_actions, actor_decisions, reset_activity_entry).chain());
    world.add_schedule(schedule);
    let mut clock = MasterClock { tick: 96, ..Default::default() };
    world.resource_mut::<GameTime>().update_from_master(&clock);
    run_gametickupdate_schedule(&mut world);
    let mut samples = Vec::new();
    for _ in 0..11 {
        clock.tick += 600;
        world.resource_mut::<GameTime>().update_from_master(&clock);
        let start = Instant::now();
        run_gametickupdate_schedule(&mut world);
        samples.push(start.elapsed().as_secs_f64() * 1_000_000.0 / 600.0);
    }
    samples.sort_by(f64::total_cmp);
    println!("actors={count} ticks_per_sample=600 samples=11 median_us={:.3} min_us={:.3} max_us={:.3}", samples[5], samples[0], samples[10]);
}

fn prepare_playback_fixture(mut actors: Query<(&mut ActorAction, &mut PlaybackState)>, time: Res<GameTime>) {
    for (mut slot, mut playback) in &mut actors {
        let Some(action) = &mut slot.0 else { continue; };
        if action.outcome.is_some() {
            *action = action::ActiveAction::new(action.definition.clone(), PlaybackId(action.playback.0 + 1));
        }
        playback.clear_for_tick();
        if time.tick() % 80 == 16 {
            playback.ordered.push(PlaybackEvent { playback: action.playback, kind: PlaybackEventKind::Cue(action.definition.cues.release.unwrap()) });
        }
    }
}

fn reset_activity_entry(mut actors: Query<&mut ActorController>) {
    for mut actor in &mut actors { actor.entered = false; }
}
