//! Opt-in development measurement of the complete loaded-game simulation tick.

use bevy::app::AppExit;
use bevy::prelude::*;
use std::time::{Duration, Instant};
use theseeker_engine::gent::Gent;
use theseeker_engine::time::{GameTickPre, GameTickUpdate, GameTickPost};
use crate::appstate::AppState;
use crate::game::combat::{Health, Invulnerable, InvulnerabilitySource};
use crate::game::enemy::{Enemy, EnemyStateSet, spawn_enemy_actor_blueprint};
use crate::game::player::Player;
use super::catalog::ActorCatalog;

pub struct PopulationProbePlugin;

#[derive(Component)]
struct ProbeProtected;

/// Presence disables the ordinary population director for a controlled workload.
#[derive(Resource)]
pub struct PopulationProbe {
    requested: usize,
    seeded: bool,
    warmup: u32,
    tick_start: Instant,
    sample_ticks: u32,
    sample_time: Duration,
    samples: Vec<f64>,
    minimum_actors: usize,
    maximum_actors: usize,
    minimum_active: usize,
    maximum_active: usize,
}

impl Plugin for PopulationProbePlugin {
    fn build(&self, app: &mut App) {
        if !cfg!(debug_assertions) { return; }
        let count = std::env::args().find_map(|arg| arg.strip_prefix("--actor-benchmark=").and_then(|count| count.parse::<usize>().ok()));
        let Some(count @ (500 | 1_000)) = count else { return; };
        app.insert_resource(PopulationProbe {
            requested: count, seeded: false, warmup: 192, tick_start: Instant::now(),
            sample_ticks: 0, sample_time: Duration::ZERO, samples: Vec::new(),
            minimum_actors: usize::MAX, maximum_actors: 0,
            minimum_active: usize::MAX, maximum_active: 0,
        });
        app.add_systems(GameTickPre, begin_tick);
        app.add_systems(GameTickUpdate, seed_population.before(EnemyStateSet::Sensors).run_if(in_state(AppState::InGame)));
        app.add_systems(GameTickUpdate, protect_population.before(EnemyStateSet::Actuator));
        app.add_systems(GameTickPost, finish_tick);
    }
}

fn protect_population(
    mut commands: Commands,
    mut actors: Query<(Entity, Option<&mut Invulnerable>), (Or<(With<Player>, With<Enemy>)>, Without<ProbeProtected>)>,
) {
    for (entity, protection) in &mut actors {
        if let Some(mut protection) = protection {
            protection.add_source(InvulnerabilitySource::PopulationProbe);
        } else {
            commands.entity(entity).insert(Invulnerable::from_source(InvulnerabilitySource::PopulationProbe));
        }
        commands.entity(entity).insert(ProbeProtected);
    }
}

fn begin_tick(mut probe: ResMut<PopulationProbe>) {
    probe.tick_start = Instant::now();
}

fn seed_population(
    mut commands: Commands,
    mut probe: ResMut<PopulationProbe>,
    catalog: Res<ActorCatalog>,
    players: Query<(Entity, &Transform), With<Player>>,
    enemies: Query<(Entity, Option<&Gent>), With<Enemy>>,
) {
    if probe.seeded { return; }
    let Some((player, transform)) = players.iter().next() else { return; };
    if catalog.actors.is_empty() { return; }
    for (entity, gent) in &enemies {
        commands.entity(entity).try_despawn();
        if let Some(gent) = gent {
            commands.entity(gent.e_gfx).try_despawn();
            commands.entity(gent.e_effects_gfx).try_despawn();
        }
    }
    commands.entity(player).insert(Health { current: u32::MAX, max: u32::MAX });
    let mut ids: Vec<_> = catalog.actors.keys().collect();
    ids.sort();
    for index in 0..probe.requested {
        let offset = Vec3::new((index % 20) as f32 * 12.0 - 120.0, (index / 20) as f32 * 8.0 + 16.0, 0.0);
        spawn_enemy_actor_blueprint(&mut commands, ids[index % ids.len()],
            Transform::from_translation(transform.translation + offset));
    }
    probe.seeded = true;
}

fn finish_tick(mut probe: ResMut<PopulationProbe>, enemies: Query<Has<crate::game::gentstate::Dead>, With<Enemy>>, mut exit: MessageWriter<AppExit>) {
    if !probe.seeded { return; }
    if probe.warmup > 0 { probe.warmup -= 1; return; }
    let duration = probe.tick_start.elapsed();
    let count = enemies.iter().len();
    let active = enemies.iter().filter(|dead| !*dead).count();
    probe.minimum_actors = probe.minimum_actors.min(count);
    probe.maximum_actors = probe.maximum_actors.max(count);
    probe.minimum_active = probe.minimum_active.min(active);
    probe.maximum_active = probe.maximum_active.max(active);
    probe.sample_time += duration;
    probe.sample_ticks += 1;
    if probe.sample_ticks < 96 { return; }
    let mean_us = probe.sample_time.as_secs_f64() * 1_000_000.0 / 96.0;
    probe.samples.push(mean_us);
    probe.sample_time = Duration::ZERO;
    probe.sample_ticks = 0;
    println!("Actor population probe: requested={} sample={} mean_us={mean_us:.3} actors={}..{}",
        probe.requested, probe.samples.len(), probe.minimum_actors, probe.maximum_actors);
    if probe.samples.len() < 7 { return; }
    probe.samples.sort_by(f64::total_cmp);
    println!("ACTOR_POPULATION_RESULT requested={} entities_min={} entities_max={} active_min={} active_max={} samples=7 ticks_per_sample=96 median_us={:.3} min_us={:.3} max_us={:.3}",
        probe.requested, probe.minimum_actors, probe.maximum_actors, probe.minimum_active, probe.maximum_active,
        probe.samples[3], probe.samples[0], probe.samples[6]);
    exit.write(AppExit::Success);
}
