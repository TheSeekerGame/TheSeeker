//! Reproducible fresh-generation measurements and optional screenshot capture.

use super::*;
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

static STAGES: Mutex<Vec<(&'static str, f64)>> = Mutex::new(Vec::new());

pub(crate) fn enabled() -> bool {
    *OnceLock::get_or_init(&ENABLED, || {
        std::env::var_os("SEEKER_LEVELGEN_BENCHMARK").is_some()
    })
}

static ENABLED: OnceLock<bool> = OnceLock::new();
static ENGINE_START: OnceLock<Instant> = OnceLock::new();

pub(crate) struct StageTimer {
    name: &'static str,
    start: Option<Instant>,
}

impl StageTimer {
    pub(crate) fn new(name: &'static str) -> Self {
        Self { name, start: enabled().then(Instant::now) }
    }
}

impl Drop for StageTimer {
    fn drop(&mut self) {
        if let Some(start) = self.start {
            STAGES.lock().expect("generation timings").push((
                self.name,
                start.elapsed().as_secs_f64() * 1000.0,
            ));
        }
    }
}

#[derive(Resource)]
pub(crate) struct GenerationMeasurement {
    start: Instant,
    finished: Option<Instant>,
    frames_after_ready: u32,
}

pub(crate) fn begin(commands: &mut Commands) {
    if !enabled() {
        return;
    }
    if let Some(start) = ENGINE_START.get() {
        info!("levelgen_benchmark: {}", serde_json::json!({"scope": "engine_start_to_request", "elapsed_ms": start.elapsed().as_secs_f64() * 1000.0}));
    }
    STAGES.lock().expect("generation timings").clear();
    commands.insert_resource(GenerationMeasurement {
        start: Instant::now(),
        finished: None,
        frames_after_ready: 0,
    });
}

pub(crate) struct GenerationDiagnosticsPlugin;

impl Plugin for GenerationDiagnosticsPlugin {
    fn build(&self, app: &mut App) {
        if enabled() {
            ENGINE_START.get_or_init(Instant::now);
            app.add_systems(PostUpdate, finish_benchmark);
        }
    }
}

fn finish_benchmark(world: &mut World) {
    let ready = world.get_resource::<pipeline::GenerationBuild>().is_some_and(|r| r.phase == pipeline::BuildPhase::Ready)
        && crate::graphics::post_processing::darkness::bake_ready(world)
        && crate::game::enemy::spawn_catalog_ready(world);
    if !ready {
        return;
    }
    let Some(mut measurement) = world.get_resource_mut::<GenerationMeasurement>() else {
        return;
    };
    if measurement.finished.is_none() {
        measurement.finished = Some(Instant::now());
        let elapsed_ms = measurement.start.elapsed().as_secs_f64() * 1000.0;
        let stages = STAGES.lock().expect("generation timings").clone();
        info!("levelgen_benchmark: {}", serde_json::json!({
            "scope": "request_to_published_frame", "elapsed_ms": elapsed_ms, "stages_ms": stages,
        }));
    }
    measurement.frames_after_ready += 1;
    if measurement.frames_after_ready == 8 {
        if let Ok(path) = std::env::var("SEEKER_LEVELGEN_SCREENSHOT") {
            world.spawn(bevy::render::view::screenshot::Screenshot::primary_window())
                .observe(bevy::render::view::screenshot::save_to_disk(path));
        }
    }
    let measurement = world.resource::<GenerationMeasurement>();
    if measurement.finished.is_some_and(|finished| finished.elapsed().as_secs() >= 3)
        && std::env::var_os("SEEKER_LEVELGEN_KEEP_OPEN").is_none()
    {
        world.write_message(bevy::app::AppExit::Success);
    }
}

pub(super) fn record_cpu_ready(world: &World) {
    let Some(measurement) = world.get_resource::<GenerationMeasurement>() else { return; };
    let retained_bytes: usize = world.resource::<Assets<Image>>().iter().filter_map(|(_, image)| image.data.as_ref()).map(Vec::len).sum();
    info!("levelgen_benchmark: {}", serde_json::json!({
        "scope": "request_to_cpu_ready", "elapsed_ms": measurement.start.elapsed().as_secs_f64() * 1000.0,
        "all_image_cpu_bytes": retained_bytes,
    }));
}
