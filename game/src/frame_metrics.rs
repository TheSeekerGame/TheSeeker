//! Aggregate main-world work separately from whole-frame pacing and render waits.
use crate::prelude::*;
use bevy::render::{Render, RenderApp, RenderSystems};
use std::sync::{Arc, Mutex};
use theseeker_engine::time::SimulationWork;

pub const DEFAULT_FRAME_LATENCY: u32 = 2;

pub struct FrameMetricsPlugin;
impl Plugin for FrameMetricsPlugin {
    fn build(&self, app: &mut App) {
        let render_metrics = RenderMetrics::default();
        app.insert_resource(render_metrics.clone()).init_resource::<FrameMetrics>();
        app.add_systems(First, begin_frame).add_systems(Last, finish_frame);
        let Some(render) = app.get_sub_app_mut(RenderApp) else { return; };
        render.insert_resource(render_metrics).init_resource::<RenderTimer>();
        render.add_systems(Render, (
            begin_render.before(RenderSystems::ExtractCommands),
            begin_submit.after(RenderSystems::Prepare).before(RenderSystems::Render),
            finish_render.after(RenderSystems::PostCleanup),
        ));
    }
}

/// Wall time on the render thread, including driver calls and surface waits.
/// It overlaps main-world work and must not be added to it as CPU utilization.
#[derive(Default)]
pub struct RenderSamples {
    pub frames: u32,
    pub prepare_seconds: f64,
    pub submit_seconds: f64,
    pub max_seconds: f64,
}

#[derive(Resource, Clone, Default)]
pub struct RenderMetrics(Arc<Mutex<RenderSamples>>);
impl RenderMetrics {
    pub fn take(&self) -> RenderSamples {
        self.0.lock().map(|mut samples| std::mem::take(&mut *samples)).unwrap_or_default()
    }
}

#[derive(Resource, Default)]
struct RenderTimer { start: Option<Instant>, submit: Option<Instant> }

fn begin_render(mut timer: ResMut<RenderTimer>) { timer.start = Some(Instant::now()); }
fn begin_submit(mut timer: ResMut<RenderTimer>) { timer.submit = Some(Instant::now()); }
fn finish_render(timer: Res<RenderTimer>, metrics: Res<RenderMetrics>) {
    let (Some(start), Some(submit)) = (timer.start, timer.submit) else { return; };
    let finished = Instant::now();
    if let Ok(mut samples) = metrics.0.lock() {
        samples.frames += 1;
        samples.prepare_seconds += submit.saturating_duration_since(start).as_secs_f64();
        samples.submit_seconds += finished.saturating_duration_since(submit).as_secs_f64();
        samples.max_seconds = samples.max_seconds.max(finished.saturating_duration_since(start).as_secs_f64());
    }
}

#[derive(Resource, Default)]
pub struct FrameMetrics {
    started: Option<Instant>,
    pub frames: u32,
    pub frame_seconds: f64,
    pub max_frame_seconds: f64,
    pub main_seconds: f64,
    pub max_main_seconds: f64,
    pub simulation_seconds: f64,
    pub max_simulation_seconds: f64,
    pub ticks: u64,
    pub focused_frames: u32,
    pub window_size: UVec2,
}

pub fn begin_frame(mut metrics: ResMut<FrameMetrics>) { metrics.started = Some(Instant::now()); }

pub fn finish_frame(
    mut metrics: ResMut<FrameMetrics>, time: Res<Time<Real>>, simulation: Res<SimulationWork>,
    windows: Query<&Window, With<bevy::window::PrimaryWindow>>,
) {
    let main = metrics.started.map_or(0.0, |started| started.elapsed().as_secs_f64());
    metrics.frames += 1;
    metrics.frame_seconds += time.delta_secs_f64();
    metrics.max_frame_seconds = metrics.max_frame_seconds.max(time.delta_secs_f64());
    metrics.main_seconds += main;
    metrics.max_main_seconds = metrics.max_main_seconds.max(main);
    metrics.simulation_seconds += simulation.seconds;
    metrics.max_simulation_seconds = metrics.max_simulation_seconds.max(simulation.seconds);
    metrics.ticks += simulation.ticks;
    if let Ok(window) = windows.single() {
        metrics.focused_frames += u32::from(window.focused);
        metrics.window_size = window.physical_size();
    }
}
