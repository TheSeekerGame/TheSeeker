//! The tick phase profile: with `SEEKER_TICK_PROFILE` set, marker systems around each phase's set
//! time it, and every second of ticks the slowest phases' average milliseconds are logged.
use bevy::ecs::schedule::InternedSystemSet;
use bevy::transform::TransformSystems;
use theseeker_engine::physics::PhysicsSet;
use theseeker_engine::profile::profiling_enabled;
use theseeker_engine::script::ScriptSet;
use theseeker_engine::time::GameTickPost;

use crate::game::enemy::EnemyStateSet;
use crate::game::player::orchestration::PlayerStateSet;
use crate::prelude::*;

const PHASE_COUNT: usize = 16;

/// Each profiled phase's log label and system set.
fn phases() -> [(&'static str, InternedSystemSet); PHASE_COUNT] {
    [
        ("script_init", ScriptSet::Init.intern()),
        ("script_run", ScriptSet::Run.intern()),
        ("transform", TransformSystems::Propagate.intern()),
        ("physics", PhysicsSet.intern()),
        ("player_sensors", PlayerStateSet::Sensors.intern()),
        ("player_input", PlayerStateSet::Input.intern()),
        ("player_behavior", PlayerStateSet::Behavior.intern()),
        ("player_collisions", PlayerStateSet::Collisions.intern()),
        ("player_anim", PlayerStateSet::Animation.intern()),
        ("enemy_sensors", EnemyStateSet::Sensors.intern()),
        ("enemy_decisions", EnemyStateSet::Decisions.intern()),
        ("enemy_actuator", EnemyStateSet::Actuator.intern()),
        ("enemy_death", EnemyStateSet::Death.intern()),
        ("enemy_collisions", EnemyStateSet::Collisions.intern()),
        ("enemy_transition", EnemyStateSet::Transition.intern()),
        ("enemy_anim", EnemyStateSet::Animation.intern()),
    ]
}

pub struct TickProfilePlugin;

impl Plugin for TickProfilePlugin {
    fn build(&self, app: &mut App) {
        if !profiling_enabled() {
            return;
        }
        info!(target: "theseeker_game::tick_profile", "Tick profiler enabled");
        app.init_resource::<TickPhaseProfiler>().add_systems(GameTickPost, finish_tick_profile);
        for (phase, (_, set)) in phases().into_iter().enumerate() {
            let begin = move |mut profiler: ResMut<TickPhaseProfiler>| profiler.begin(phase);
            let end = move |mut profiler: ResMut<TickPhaseProfiler>| profiler.end(phase);
            app.configure_sets(GameTickUpdate, (PhaseMarker::Begin(phase), set, PhaseMarker::End(phase)).chain())
                .add_systems(
                    GameTickUpdate,
                    (begin.in_set(PhaseMarker::Begin(phase)), end.in_set(PhaseMarker::End(phase)))
                        .run_if(in_state(AppState::InGame)),
                );
        }
    }
}

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum PhaseMarker {
    Begin(usize),
    End(usize),
}

#[derive(Resource, Default)]
struct TickPhaseProfiler {
    starts: [Option<Instant>; PHASE_COUNT],
    totals: [Duration; PHASE_COUNT],
    ticks: u32,
}

impl TickPhaseProfiler {
    fn begin(&mut self, phase: usize) {
        self.starts[phase] = Some(Instant::now());
    }

    fn end(&mut self, phase: usize) {
        if let Some(start) = self.starts[phase].take() {
            self.totals[phase] += start.elapsed();
        }
    }
}

fn finish_tick_profile(time: Res<GameTime>, mut profiler: ResMut<TickPhaseProfiler>) {
    profiler.ticks += 1;
    if profiler.ticks < time.hz.round().max(1.0) as u32 {
        return;
    }
    let ticks = f64::from(profiler.ticks);
    let mut phases: Vec<(f64, &str)> = phases()
        .into_iter()
        .zip(profiler.totals)
        .map(|((label, _), total)| (total.as_secs_f64() * 1000.0 / ticks, label))
        .collect();
    phases.sort_by(|a, b| b.0.total_cmp(&a.0));
    let total_ms: f64 = phases.iter().map(|(ms, _)| ms).sum();
    let slowest: Vec<String> =
        phases.iter().filter(|(ms, _)| *ms >= 0.05).take(8).map(|(ms, label)| format!("{label}={ms:.2}")).collect();
    info!(target: "theseeker_game::tick_profile", "Tick profile avg_ms: total={total_ms:.2} {}", slowest.join(", "));
    *profiler = TickPhaseProfiler::default();
}
