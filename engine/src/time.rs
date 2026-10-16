//! Audio-derived transport, fixed simulation, and delayed presentation.
use bevy::ecs::{schedule::{SingleThreadedExecutor, ScheduleLabel}, system::SystemParam};
use crate::audio::{AudioClock, AudioEngine, AudioEngineConfig, AudioSet, AudioStartTime};
use crate::audio::transport::TICK_RATE_HZ;
use crate::prelude::*;

#[cfg(test)]
mod tests;

pub struct GameTimePlugin;

impl Plugin for GameTimePlugin {
    fn build(&self, app: &mut App) {
        for schedule in [Schedule::new(GameTickPre), Schedule::new(GameTickUpdate), Schedule::new(GameTickPost)] {
            app.add_schedule(schedule);
        }
        app.edit_schedule(GameTickPre, |schedule| { schedule.set_executor(SingleThreadedExecutor::new()); });
        app.edit_schedule(GameTickUpdate, |schedule| { schedule.set_executor(SingleThreadedExecutor::new()); });
        app.edit_schedule(GameTickPost, |schedule| { schedule.set_executor(SingleThreadedExecutor::new()); });
        app.init_resource::<MasterClock>().init_resource::<ClockTime>()
            .init_resource::<SimulationWork>()
            .init_resource::<PresentationTime>().init_resource::<GameTransport>()
            .init_resource::<RenderInterpolation>().init_resource::<GameTime>()
            .init_resource::<ClockSource>().init_resource::<TimingSettings>();
        app.configure_sets(Update, MasterClockSet::Update.after(AudioSet::Update));
        app.add_systems(Update, (
            update_master_clock.in_set(MasterClockSet::Update),
            run_gametickupdate_schedule.after(MasterClockSet::Update),
            log_diagnostics.in_set(GameTickSet::Post),
        ));
        app.configure_sets(Update, GameTickSet::Pre.after(MasterClockSet::Update).before(run_gametickupdate_schedule));
        app.configure_sets(Update, GameTickSet::Post.after(run_gametickupdate_schedule));
    }
}

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GameTickSet { Pre, Post }
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MasterClockSet { Update }
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct GameTickEventClearSet;

#[derive(Resource, Debug, Clone)]
pub struct MasterClock {
    pub tick_rate_hz: u32,
    pub sample_rate: u32,
    pub tick: u64,
    pub tick_alpha: f64,
    pub elapsed: Duration,
    pub delta: Duration,
}

impl Default for MasterClock {
    fn default() -> Self {
        Self { tick_rate_hz: TICK_RATE_HZ, sample_rate: 48_000, tick: 0,
            tick_alpha: 0.0, elapsed: Duration::ZERO, delta: Duration::ZERO }
    }
}

#[derive(Resource, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ClockSource { Audio, #[default] Time }

/// Wall time spent executing all custom simulation schedules in the last app frame.
#[derive(Resource, Default)]
pub struct SimulationWork { pub seconds: f64, pub ticks: u64 }

/// Game code requests pause; the audio callback commits it on a tick boundary.
#[derive(Resource, Debug, Default)]
pub struct GameTransport {
    pub paused: bool,
    pub generation: u64,
    pub resynchronizations: u64,
    pub lead_seconds: f64,
}

#[derive(Resource, Debug)]
pub struct TimingSettings {
    /// Expected extraction-to-display delay, independently adjustable from audio lead.
    pub display_latency_ms: f64,
    /// Positive values present a later transport moment to compensate a slow display.
    pub av_offset_ms: f64,
    pub diagnostics: bool,
}

impl Default for TimingSettings {
    fn default() -> Self {
        Self {
            display_latency_ms: env_number("SEEKER_DISPLAY_LATENCY_MS", 16.667).clamp(0.0, 400.0),
            av_offset_ms: env_number("SEEKER_AV_OFFSET_MS", 0.0).clamp(-100.0, 100.0),
            diagnostics: std::env::var("SEEKER_TIME_DIAGNOSTICS").is_ok_and(|value| value != "0"),
        }
    }
}

fn env_number(key: &str, default: f64) -> f64 {
    std::env::var(key).ok().and_then(|value| value.parse::<f64>().ok()).filter(|value| value.is_finite()).unwrap_or(default)
}

/// Simulation time; updated for each fixed tick, never from the monitor refresh rate.
#[derive(Resource, Debug, Clone, Default)]
pub struct ClockTime { elapsed: Duration, delta: Duration }
impl ClockTime {
    pub fn elapsed(&self) -> Duration { self.elapsed }
    pub fn delta(&self) -> Duration { self.delta }
    pub fn elapsed_secs(&self) -> f32 { self.elapsed.as_secs_f32() }
    pub fn delta_secs(&self) -> f32 { self.delta.as_secs_f32() }
    fn advance_to(&mut self, seconds: f64) {
        let elapsed = Duration::from_secs_f64(seconds.max(0.0));
        self.delta = elapsed.saturating_sub(self.elapsed);
        self.elapsed = elapsed;
    }
}

/// The time being presented; gameplay visual effects consume this instead of wall time.
#[derive(Resource, Debug, Clone, Default, Deref, DerefMut)]
pub struct PresentationTime(ClockTime);

#[derive(Resource, Debug, Clone, Default)]
pub struct RenderInterpolation { position: f64 }
impl RenderInterpolation {
    pub fn alpha(&self) -> f32 { self.position.fract() as f32 }
    pub fn position(&self) -> f64 { self.position }
    pub fn tick(&self) -> u64 { self.position.floor() as u64 }
    pub fn at(position: f64) -> Self { Self { position: position.max(0.0) } }
}

#[derive(Default)]
struct ClockState {
    host: Option<Instant>,
    seconds: f64,
    paused: bool,
    freeze_seconds: f64,
    audio_active: bool,
    device_generation: u64,
    awaiting_generation: Option<u64>,
    frame_budget_seconds: f64,
    frame_intervals: FrameIntervals,
}

#[derive(Default)]
struct FrameIntervals { samples: [f64; 32], count: usize, next: usize }
impl FrameIntervals {
    fn record(&mut self, seconds: f64) -> Option<f64> {
        if !(0.0..0.1).contains(&seconds) || seconds == 0.0 { return None; }
        self.samples[self.next] = seconds;
        self.next = (self.next + 1) % self.samples.len();
        self.count = (self.count + 1).min(self.samples.len());
        if self.count < self.samples.len() { return None; }
        let mut ordered = self.samples;
        ordered[..self.count].sort_unstable_by(f64::total_cmp);
        Some(ordered[(self.count - 1) * 9 / 10])
    }
}

#[derive(SystemParam)]
struct ClockInputs<'w> {
    clock: Res<'w, AudioClock>,
    config: Res<'w, AudioEngineConfig>,
    settings: Res<'w, TimingSettings>,
}

#[derive(SystemParam)]
struct ClockOutputs<'w> {
    master: ResMut<'w, MasterClock>,
    game: ResMut<'w, GameTime>,
    transport: ResMut<'w, GameTransport>,
    presentation: ResMut<'w, PresentationTime>,
    simulation_time: ResMut<'w, ClockTime>,
    interpolation: ResMut<'w, RenderInterpolation>,
    source: ResMut<'w, ClockSource>,
}

fn update_master_clock(inputs: ClockInputs, mut out: ClockOutputs, mut engine: ResMut<AudioEngine>, mut state: Local<ClockState>) {
    advance_master_clock(inputs, &mut out, &mut engine, &mut state, Instant::now());
}

fn advance_master_clock(inputs: ClockInputs, out: &mut ClockOutputs, engine: &mut AudioEngine, state: &mut ClockState, now: Instant) {
    let delta = state.host.replace(now).map_or(0.0, |previous| now.saturating_duration_since(previous).as_secs_f64());
    let rate = f64::from(inputs.config.tick_rate_hz.max(1));
    if let Some(budget) = state.frame_intervals.record(delta) { state.frame_budget_seconds = budget; }
    let healthy = inputs.clock.healthy(now);
    let recovered = healthy && (!state.audio_active || state.device_generation != inputs.clock.generation);
    // Lead controls how far ahead we simulate. Growing it does not move the
    // sample timeline or invalidate voices. Do not shrink it during playback.
    out.transport.lead_seconds = out.transport.lead_seconds.max(
        scheduling_lead(&inputs.config, &inputs.clock, &inputs.settings, state.frame_budget_seconds));
    let lead = out.transport.lead_seconds;
    if out.transport.paused != state.paused {
        state.paused = out.transport.paused;
        state.freeze_seconds = out.game.tick() as f64 / rate;
        if state.paused { engine.pause_at_tick(out.game.tick()); }
        else { engine.resume_after(Duration::from_secs_f64(lead)); }
    }
    let forced = engine.take_resync_request();
    if !healthy || state.awaiting_generation.is_some_and(|expected| inputs.clock.generation > expected) {
        state.awaiting_generation = None;
    }
    if healthy && (forced || recovered) && out.game.tick() > 0 && state.awaiting_generation.is_none() {
        resynchronize(out, state, engine);
    }
    if state.awaiting_generation.is_some_and(|generation| inputs.clock.generation == generation) {
        state.awaiting_generation = None;
    }
    state.audio_active = healthy;
    state.device_generation = inputs.clock.generation;
    let mut seconds = if healthy && state.awaiting_generation.is_none() {
        inputs.clock.transport_position(now) / f64::from(inputs.clock.sample_rate)
    } else if healthy { state.seconds }
    else { state.seconds + delta };
    if state.paused { seconds = seconds.min(state.freeze_seconds); }
    seconds = seconds.max(state.seconds);
    state.seconds = seconds;
    let mut target = if state.paused || state.awaiting_generation.is_some() { out.game.tick() }
        else { ((seconds + lead) * rate).floor() as u64 };
    if target.saturating_sub(out.game.tick()) > (lead * rate).ceil() as u64 + 8 {
        if healthy { resynchronize(out, state, engine); }
        else { state.seconds = (out.game.tick() as f64 / rate - lead).max(0.0); out.transport.resynchronizations += 1; }
        target = out.game.tick();
    }
    out.master.tick_rate_hz = inputs.config.tick_rate_hz;
    out.master.sample_rate = inputs.clock.sample_rate;
    out.master.tick = target.max(out.game.tick());
    out.master.tick_alpha = ((state.seconds + lead) * rate).fract();
    let elapsed = Duration::from_secs_f64(out.master.tick as f64 / rate);
    out.master.delta = elapsed.saturating_sub(out.master.elapsed);
    out.master.elapsed = elapsed;
    out.simulation_time.advance_to(elapsed.as_secs_f64());
    out.game.update_from_master(&out.master);
    *out.source = if healthy { ClockSource::Audio } else { ClockSource::Time };
    let offset = (inputs.settings.display_latency_ms + inputs.settings.av_offset_ms) / 1000.0;
    let present = if healthy && state.awaiting_generation.is_none() {
        let instant = if offset >= 0.0 { now + Duration::from_secs_f64(offset) }
            else { now - Duration::from_secs_f64(-offset) };
        inputs.clock.transport_position(instant) / f64::from(inputs.clock.sample_rate)
    } else { state.seconds + offset };
    let present = present.max(out.presentation.elapsed().as_secs_f64()).min(out.master.tick as f64 / rate);
    out.interpolation.position = present.max(0.0) * rate;
    out.presentation.advance_to(present);
    out.transport.generation = inputs.clock.generation;
}

fn scheduling_lead(config: &AudioEngineConfig, clock: &AudioClock, settings: &TimingSettings, frame_budget: f64) -> f64 {
    let tick = 1.0 / f64::from(config.tick_rate_hz.max(1));
    let minimum = clock.latency().as_secs_f64() + f64::from(clock.block_frames) / f64::from(clock.sample_rate) + frame_budget.max(tick) + tick;
    let display = (settings.display_latency_ms + settings.av_offset_ms).max(0.0) / 1000.0 + tick;
    ((f64::from(config.lead_ticks) * tick).max(minimum).max(display) / tick).ceil() * tick
}

fn resynchronize(out: &mut ClockOutputs, state: &mut ClockState, engine: &mut AudioEngine) {
    let generation = engine.resynchronize(out.game.tick());
    state.awaiting_generation = Some(generation);
    state.seconds = out.game.tick() as f64 / out.game.hz;
    out.transport.resynchronizations += 1;
    if out.transport.paused { engine.pause_at_tick(out.game.tick()); }
    else { engine.resume_after(Duration::from_secs_f64(out.transport.lead_seconds)); }
}
pub trait GameTimeAppExt {
    fn add_gametick_event<T: Message>(&mut self) -> &mut Self;
}

impl GameTimeAppExt for App {
    fn add_gametick_event<T: Message>(&mut self) -> &mut Self {
        if !self.world().contains_resource::<Messages<T>>() {
            self.init_resource::<Messages<T>>();
            self.add_systems(
                GameTickUpdate,
                minimal_event_update_system::<T>.in_set(GameTickEventClearSet),
            );
        } else {
            warn!("Attempted to add a Game Tick event type that had already been added as an event before!");
        }
        self
    }
}

fn minimal_event_update_system<T: Message>(mut events: ResMut<Messages<T>>) {
    events.update();
}

/// Our alternative to `FixedUpdate`
#[derive(ScheduleLabel, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct GameTickUpdate;

/// Run before `GameTickUpdate`
#[derive(ScheduleLabel, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct GameTickPre;

/// Run after `GameTickUpdate`
/// used for running systems which update input state between GameTickUpdate runs
#[derive(ScheduleLabel, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct GameTickPost;

#[derive(Resource, Debug)]
pub struct GameTime {
    /// The time base rate
    pub hz: f64,
    tick: u64,
    new_ticks: u64,
    total_ticks: u64,
    overstep: f64,
    last_update: Duration,
}

impl Default for GameTime {
    fn default() -> Self {
        Self {
            hz: f64::from(TICK_RATE_HZ),
            tick: 0,
            new_ticks: 0,
            total_ticks: 0,
            overstep: 0.0,
            last_update: Duration::new(0, 0),
        }
    }
}

impl GameTime {
    /// Create with a non-default tick rate
    pub fn new(hz: f64) -> Self {
        Self {
            hz,
            ..Default::default()
        }
    }

    /// Get the current tick number to be simulated
    ///
    /// Increments with every run of the `GameTickUpdate` schedule, until it reaches
    /// the `total_ticks` target value.
    pub fn tick(&self) -> u64 {
        self.tick
    }

    /// Get the number of new ticks to be simulated this Bevy frame update
    pub fn new_ticks(&self) -> u64 {
        self.new_ticks
    }

    /// Get the total target number of ticks as of this Bevy frame update
    pub fn total_ticks(&self) -> u64 {
        self.total_ticks
    }

    /// Get the leftover partial tick to be carried over to the next Bevy frame update
    pub fn overstep(&self) -> f64 {
        self.overstep
    }

    /// Get the "elapsed" time when we last updated
    pub fn last_update(&self) -> Duration {
        self.last_update
    }

    /// Reset tick counters to zero, set the last update to now, keep the `hz` value
    pub fn reset(&mut self, now: Duration) {
        *self = Self {
            hz: self.hz,
            last_update: now,
            ..Default::default()
        };
    }

    /// Every frame, this gets called to advance the tick counters.
    pub fn update_from_master(&mut self, master: &MasterClock) {
        self.hz = master.tick_rate_hz as f64;
        self.last_update = master.elapsed;
        self.total_ticks = master.tick;
        self.overstep = master.tick_alpha;
        self.new_ticks = self.total_ticks.saturating_sub(self.tick);
    }

    /// returns the amount of time since start; ie: ticks * tick length
    pub fn time_in_seconds(&self) -> f64 {
        self.tick() as f64 * self.seconds_per_tick()
    }

    /// Convenience function to save you the math
    pub fn seconds_per_tick(&self) -> f64 {
        1.0 / self.hz
    }
}


pub fn run_gametickupdate_schedule(world: &mut World) {
    let started = Instant::now();
    let first_tick = world.resource::<GameTime>().tick;
    let frame_clock = world.get_resource::<ClockTime>().cloned();
    while world.resource::<GameTime>().tick < world.resource::<GameTime>().total_ticks {
        let tick = world.resource::<GameTime>().tick;
        let seconds = world.resource::<GameTime>().time_in_seconds();
        let tick_duration = Duration::from_secs_f64(world.resource::<GameTime>().seconds_per_tick());
        if let Some(mut clock) = world.get_resource_mut::<ClockTime>() {
            *clock = ClockTime { elapsed: Duration::from_secs_f64(seconds), delta: tick_duration };
        }
        if let Some(mut engine) = world.get_resource_mut::<AudioEngine>() { engine.set_command_time(AudioStartTime::Tick(tick)); }
        world.run_schedule(GameTickPre);
        world.run_schedule(GameTickUpdate);
        world.run_schedule(GameTickPost);
        if let Some(mut engine) = world.get_resource_mut::<AudioEngine>() { engine.flush(); }
        world.resource_mut::<GameTime>().tick += 1;
    }
    if let Some(clock) = frame_clock { world.insert_resource(clock); }
    let ticks = world.resource::<GameTime>().tick - first_tick;
    if let Some(mut work) = world.get_resource_mut::<SimulationWork>() {
        work.seconds = started.elapsed().as_secs_f64();
        work.ticks = ticks;
    }
}

pub fn at_tick_multiples(quant: Quant) -> impl FnMut(Res<GameTime>) -> bool {
    move |time: Res<GameTime>| quant.check(time.tick() as i64)
}

fn log_diagnostics(
    settings: Res<TimingSettings>, transport: Res<GameTransport>, engine: Res<AudioEngine>,
    presentation: Res<RenderInterpolation>, time: Res<Time<Real>>, mut elapsed: Local<f64>,
) {
    if !settings.diagnostics { return; }
    *elapsed += time.delta_secs_f64();
    if *elapsed < 1.0 { return; }
    *elapsed = 0.0;
    info!("Transport: lead_ms={:.2} display_ms={:.2} av_ms={:.2} presentation_tick={:.3} resyncs={} late={} (plays={} controls={} max_frames={}) rejected={} retries={} callback_max_us={}",
        transport.lead_seconds * 1000.0, settings.display_latency_ms, settings.av_offset_ms,
        presentation.position(), transport.resynchronizations, engine.diagnostics.late_events,
        engine.diagnostics.late_plays, engine.diagnostics.late_controls, engine.diagnostics.max_lateness_frames,
        engine.diagnostics.rejected_events, engine.dropped_commands, engine.diagnostics.max_callback_micros);
}
