use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use bevy::asset::UntypedAssetId;
use bevy::audio::AudioSource;
use bevy::prelude::*;
use bevy::tasks::{block_on, futures_lite::future, AsyncComputeTaskPool, Task};
use crossbeam_channel::{bounded, Receiver, Sender};
use symphonium::{ResampleQuality, SymphoniumLoader};

use super::mixer::{AudioBus, Command, CommandKind, Control, Mapping, MixerDiagnostics, Play, SampleData};
const CLICK_HANDLE: Handle<AudioSource> = bevy::asset::uuid_handle!("2f4ad04e-4e9b-4b28-91cb-3e294be7e7c1");

use super::transport::{duration_sample, tick_sample, TICK_RATE_HZ};

#[derive(Resource, Debug, Clone)]
pub struct AudioEngineConfig {
    /// Disable device access for deterministic tests or intentionally silent play.
    pub output_enabled: bool,
    pub tick_rate_hz: u32,
    pub desired_sample_rate: u32,
    pub desired_block_frames: u32,
    pub lead_ticks: u32,
    pub max_sfx_voices: usize,
    pub max_background_voices: usize,
    pub command_capacity: usize,
}

impl Default for AudioEngineConfig {
    fn default() -> Self {
        Self {
            output_enabled: std::env::var_os("SEEKER_SILENT_AUDIO").is_none(),
            tick_rate_hz: TICK_RATE_HZ, desired_sample_rate: 48_000,
            desired_block_frames: 256, lead_ticks: 4,
            max_sfx_voices: 48, max_background_voices: 4, command_capacity: 1024,
        }
    }
}

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AudioSet { Update, Flush }

#[derive(Debug, Clone, Copy)]
pub(super) struct ClockSnapshot {
    pub device_end: i64,
    pub playback_end: Instant,
    pub observed: Instant,
    pub mapping: Mapping,
    pub epoch: u64,
    pub block_frames: u32,
    pub stats: MixerDiagnostics,
    pub latency: Duration,
}

#[derive(Resource, Debug, Clone)]
pub struct AudioClock {
    pub running: bool,
    pub sample_rate: u32,
    pub block_frames: u32,
    pub generation: u64,
    snapshot: Option<ClockSnapshot>,
}

impl Default for AudioClock {
    fn default() -> Self {
        Self { running: false, sample_rate: 48_000, block_frames: 256, generation: 1, snapshot: None }
    }
}

impl AudioClock {
    #[cfg(test)]
    pub(crate) fn test_running(now: Instant, frame: i64, generation: u64) -> Self {
        Self { running: true, generation, snapshot: Some(ClockSnapshot {
            device_end: frame, playback_end: now, observed: now, mapping: Mapping::default(),
            epoch: generation, block_frames: 256, stats: MixerDiagnostics::default(), latency: Duration::ZERO,
        }), ..Default::default() }
    }
    pub fn healthy(&self, now: Instant) -> bool {
        self.running && self.snapshot.is_some_and(|snapshot| {
            let deadline = Duration::from_secs_f64((f64::from(self.block_frames) / f64::from(self.sample_rate) * 8.0).max(0.25));
            now.saturating_duration_since(snapshot.observed) < deadline
        })
    }

    pub fn device_position(&self, at: Instant) -> f64 {
        self.snapshot.map_or(0.0, |snapshot| {
            snapshot.device_end as f64 + signed_seconds(at, snapshot.playback_end) * f64::from(self.sample_rate)
        })
    }

    pub fn transport_position(&self, at: Instant) -> f64 {
        self.snapshot.map_or(0.0, |snapshot| snapshot.mapping.position(self.device_position(at)))
    }

    pub fn latency(&self) -> Duration { self.snapshot.map_or(Duration::ZERO, |snapshot| snapshot.latency) }
    pub fn paused(&self) -> bool { self.snapshot.is_some_and(|snapshot| snapshot.mapping.paused) }
}

fn signed_seconds(a: Instant, b: Instant) -> f64 {
    if a >= b { a.duration_since(b).as_secs_f64() } else { -b.duration_since(a).as_secs_f64() }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct AudioInstanceId(pub(super) u64);

struct AudioInstance {
    bus: AudioBus,
    label: Option<String>,
    asset: UntypedAssetId,
    _handle: Handle<AudioSource>,
    ended: Arc<AtomicU64>,
}

#[derive(Debug, Clone, Copy)]
pub enum AudioStartTime { Immediate, Tick(u64), Time(Duration) }

#[derive(Debug, Clone, Copy)]
pub struct AudioPlaybackConfig {
    pub timing: AudioStartTime,
    pub volume: f32,
    pub pan: f32,
    pub looped: bool,
    /// Source-file sample-frame bounds and their sample rate.
    pub loop_region: Option<(u64, u64, u32)>,
    pub position: Duration,
    pub fade_in: Duration,
}

impl Default for AudioPlaybackConfig {
    fn default() -> Self {
        Self { timing: AudioStartTime::Immediate, volume: 1.0, pan: 0.0,
            looped: false, loop_region: None, position: Duration::ZERO, fade_in: Duration::ZERO }
    }
}

struct PendingPlay {
    id: AudioInstanceId,
    asset: UntypedAssetId,
    config: AudioPlaybackConfig,
    at: i64,
    order: u64,
}

struct PreparedSample { sample: Arc<SampleData>, deferred: bool }
type DecodeTask = Task<Result<Arc<SampleData>, String>>;

#[derive(Resource)]
pub struct AudioEngine {
    pub(super) command_tx: Sender<Command>,
    pub(super) control_tx: Sender<Control>,
    pub(super) clock_rx: Receiver<ClockSnapshot>,
    pub(super) running: Arc<AtomicBool>,
    config: AudioEngineConfig,
    clock: AudioClock,
    instances: HashMap<AudioInstanceId, AudioInstance>,
    samples: HashMap<UntypedAssetId, PreparedSample>,
    pins: Arc<Mutex<AudioPins>>,
    tasks: HashMap<UntypedAssetId, (DecodeTask, bool)>,
    failed: HashSet<UntypedAssetId>,
    requested: HashSet<UntypedAssetId>,
    retained_music: HashSet<UntypedAssetId>,
    pending: Vec<PendingPlay>,
    controls: VecDeque<Control>,
    next_id: u64,
    next_order: u64,
    epoch: u64,
    command_time: AudioStartTime,
    pub diagnostics: MixerDiagnostics,
    pub dropped_commands: u64,
    resync_requested: bool,
    reconnect_at: Instant,
    pub(super) session_generation: u64,
}

#[derive(Default)]
struct AudioPins {
    samples: Vec<Arc<SampleData>>,
    completions: Vec<Arc<AtomicU64>>,
}

// Fields drop in declaration order: stop/join the stream before releasing its pins.
pub struct AudioStream {
    pub(super) stream: Option<cpal::Stream>,
    _pins: Arc<Mutex<AudioPins>>,
}

impl AudioEngine {
    pub(crate) fn build(config: &AudioEngineConfig) -> (Self, AudioStream) {
        let (command_tx, command_rx) = bounded(config.command_capacity.max(1));
        let (control_tx, control_rx) = bounded(config.command_capacity.max(1));
        let (clock_tx, clock_rx) = bounded(8);
        let running = Arc::new(AtomicBool::new(false));
        let started = if config.output_enabled {
            super::device::start(config, command_rx, control_rx, clock_tx, clock_rx.clone(), running.clone())
        } else { Err("Output disabled".to_owned()) };
        let (stream, rate, block) = match started {
            Ok((stream, rate, block)) => (Some(stream), rate, block),
            Err(error) => {
                warn!("Custom audio unavailable; using silent transport: {error}");
                (None, config.desired_sample_rate, config.desired_block_frames)
            },
        };
        let pins = Arc::new(Mutex::new(AudioPins::default()));
        (Self {
            command_tx, control_tx, clock_rx, running, config: config.clone(),
            clock: AudioClock { sample_rate: rate, block_frames: block, ..Default::default() },
            instances: HashMap::new(), samples: HashMap::new(), pins: pins.clone(),
            tasks: HashMap::new(), failed: HashSet::new(), requested: HashSet::new(),
            retained_music: HashSet::new(), pending: Vec::new(), controls: VecDeque::new(),
            next_id: 1, next_order: 1, epoch: 1, command_time: AudioStartTime::Immediate,
            diagnostics: MixerDiagnostics::default(), dropped_commands: 0, resync_requested: false,
            reconnect_at: Instant::now() + Duration::from_secs(2), session_generation: 0,
        }, AudioStream { stream, _pins: pins })
    }

    pub fn sample_rate(&self) -> u32 { self.clock.sample_rate }
    pub fn block_frames(&self) -> u32 { self.clock.block_frames }
    pub fn running(&self) -> bool { self.running.load(Ordering::Acquire) }
    pub fn clock(&self) -> &AudioClock { &self.clock }
    pub fn prepare(&mut self, handle: &Handle<AudioSource>) { self.requested.insert(handle.id().untyped()); }
    pub fn is_prepared(&self, handle: &Handle<AudioSource>) -> bool { self.samples.contains_key(&handle.id().untyped()) }
    pub fn preparation_failed(&self, handle: &Handle<AudioSource>) -> bool { self.failed.contains(&handle.id().untyped()) }
    pub fn preparation_ready(&self, preloaded: &crate::assets::PreloadedAssets, server: &AssetServer) -> bool {
        preloaded.handles().all(|handle| self.preloaded_ready(handle, server))
    }

    fn preloaded_ready(&self, handle: &UntypedHandle, server: &AssetServer) -> bool {
        if handle.type_id() != std::any::TypeId::of::<AudioSource>() { return true; }
        let id = handle.id();
        let deferred = server.get_path(id).is_some_and(|path| path.path().starts_with("audio/music"));
        deferred || self.samples.contains_key(&id) || self.failed.contains(&id)
            || matches!(server.get_load_state(id), Some(bevy::asset::LoadState::Failed(_)))
    }
    pub fn set_command_time(&mut self, timing: AudioStartTime) { self.command_time = timing; }
    pub fn contains(&self, id: AudioInstanceId) -> bool { self.instances.contains_key(&id) }
    pub fn cleanup_ids(&self, ids: &mut Vec<AudioInstanceId>) { ids.retain(|id| self.contains(*id)); }

    pub fn retain_music(&mut self, handles: &[Handle<AudioSource>]) {
        self.retained_music = handles.iter().map(|handle| handle.id().untyped()).collect();
    }

    pub fn play_sfx(&mut self, label: Option<&str>, handle: &Handle<AudioSource>, _server: &AssetServer, config: AudioPlaybackConfig) -> Option<AudioInstanceId> {
        self.play(AudioBus::Sfx, label, handle, AudioPlaybackConfig { looped: false, loop_region: None, ..config })
    }

    pub fn play_background(&mut self, label: Option<&str>, handle: &Handle<AudioSource>, _server: &AssetServer, config: AudioPlaybackConfig) -> Option<AudioInstanceId> {
        self.play(AudioBus::Ambience, label, handle, config)
    }

    pub fn play_music(&mut self, handle: &Handle<AudioSource>, config: AudioPlaybackConfig) -> Option<AudioInstanceId> {
        self.play(AudioBus::Music, None, handle, config)
    }

    fn play(&mut self, bus: AudioBus, label: Option<&str>, handle: &Handle<AudioSource>, config: AudioPlaybackConfig) -> Option<AudioInstanceId> {
        if !self.running() { return None; }
        if !config.volume.is_finite() || !config.pan.is_finite() || self.pending.len() >= self.config.command_capacity {
            self.dropped_commands += 1;
            return None;
        }
        if bus != AudioBus::Sfx && self.instances.values().filter(|instance| instance.bus != AudioBus::Sfx).count() >= self.config.max_background_voices.max(2) {
            return None;
        }
        let asset = handle.id().untyped();
        if self.failed.contains(&asset) { return None; }
        self.prepare(handle);
        let id = AudioInstanceId(self.next_id);
        self.next_id += 1;
        let at = self.resolve_time(config.timing);
        let order = self.order();
        let ended = self.completion();
        self.instances.insert(id, AudioInstance { bus, label: label.map(str::to_owned), asset, _handle: handle.clone(), ended });
        self.pending.push(PendingPlay { id, asset, config, at, order });
        Some(id)
    }

    pub fn play_tick_click(&mut self, tick: u64) {
        if !self.running() { return; }
        let id = AudioInstanceId(self.next_id);
        self.next_id += 1;
        let pins = &self.pins;
        let sample = self.samples.entry(CLICK_HANDLE.id().untyped()).or_insert_with(|| {
            let sample = Arc::new(SampleData { channels: 1, frames: 1, data: vec![0.1] });
            pins.lock().expect("Audio pins lock poisoned").samples.push(sample.clone());
            PreparedSample { sample, deferred: false }
        }).sample.clone();
        let ended = self.completion();
        let asset = CLICK_HANDLE.id().untyped();
        self.instances.insert(id, AudioInstance { bus: AudioBus::Sfx, label: None, asset, _handle: CLICK_HANDLE.clone(), ended: ended.clone() });
        let command = Command { at: tick_sample(tick, self.sample_rate(), self.config.tick_rate_hz), epoch: self.epoch, order: self.order(), kind: CommandKind::Play(Play {
            id: id.0, sample, ended, bus: AudioBus::Sfx, volume: std::f32::consts::SQRT_2, pan: 0.0,
            loop_range: None, position: 0, fade_frames: 0,
        }) };
        if self.command_tx.try_send(command).is_err() { self.instances.remove(&id); self.dropped_commands += 1; }
    }

    pub fn reference_clicks(&mut self, enabled: bool) { self.queue_control(Control::Reference(enabled)); }

    pub fn stop_one(&mut self, id: AudioInstanceId) {
        if self.pending.iter().any(|play| play.id == id) {
            self.pending.retain(|play| play.id != id);
            self.instances.remove(&id);
            return;
        }
        let command = Command { at: self.resolve_time(self.command_time), epoch: self.epoch, order: self.order(), kind: CommandKind::Stop(id.0) };
        self.queue_control(Control::Event(command));
    }
    pub fn stop_many(&mut self, ids: &[AudioInstanceId]) { for &id in ids { self.stop_one(id); } }
    pub fn stop_all(&mut self) {
        self.cancel_pending();
        let command = Command { at: self.resolve_time(self.command_time), epoch: self.epoch, order: self.order(),
            kind: CommandKind::StopAll(self.next_id.saturating_sub(1)) };
        self.queue_control(Control::Event(command));
    }

    fn cancel_pending(&mut self) {
        for play in self.pending.drain(..) { self.instances.remove(&play.id); }
    }
    pub fn stop_sfx_all(&mut self) { self.stop_bus(AudioBus::Sfx, None); }
    pub fn stop_sfx_label(&mut self, label: &str) { self.stop_bus(AudioBus::Sfx, Some(label)); }
    pub fn stop_backgrounds(&mut self, label: Option<&str>) { self.stop_bus(AudioBus::Ambience, label); }
    pub fn matches_label(&self, id: AudioInstanceId, label: &str) -> bool { self.instances.get(&id).is_some_and(|instance| instance.label.as_deref() == Some(label)) }

    fn stop_bus(&mut self, bus: AudioBus, label: Option<&str>) {
        let ids: Vec<_> = self.instances.iter().filter(|(_, instance)| instance.bus == bus && label.is_none_or(|label| instance.label.as_deref() == Some(label))).map(|(&id, _)| id).collect();
        self.stop_many(&ids);
    }

    pub fn set_bus_volume(&mut self, bus: AudioBus, volume: f32) {
        if !volume.is_finite() { return; }
        let command = Command { at: self.resolve_time(AudioStartTime::Immediate), epoch: self.epoch,
            order: self.order(), kind: CommandKind::BusGain { bus, volume: volume.max(0.0) } };
        self.queue_control(Control::Event(command));
    }

    pub fn fade(&mut self, id: AudioInstanceId, volume: f32, timing: AudioStartTime, duration: Duration, stop_after: bool) {
        if !volume.is_finite() { return; }
        let command = Command { at: self.resolve_time(timing), epoch: self.epoch, order: self.order(), kind: CommandKind::Gain {
            id: id.0, volume: volume.max(0.0), frames: duration_sample(duration, self.sample_rate()) as usize, stop_after,
        } };
        self.queue_control(Control::Event(command));
    }

    pub fn pause_at_tick(&mut self, tick: u64) {
        self.queue_control(Control::Pause(tick_sample(tick, self.sample_rate(), self.config.tick_rate_hz)));
    }
    pub fn resume_after(&mut self, delay: Duration) {
        self.queue_control(Control::ResumeAfter(duration_sample(delay, self.sample_rate())));
    }
    pub fn resynchronize(&mut self, tick: u64) -> u64 {
        self.epoch += 1;
        self.cancel_pending();
        self.controls.clear();
        self.controls.push_back(Control::Reset { frame: tick_sample(tick, self.sample_rate(), self.config.tick_rate_hz), epoch: self.epoch });
        self.epoch + self.session_generation
    }

    pub(crate) fn take_resync_request(&mut self) -> bool { std::mem::take(&mut self.resync_requested) }

    fn queue_control(&mut self, control: Control) {
        if self.controls.len() >= self.config.command_capacity.max(1) {
            self.controls.clear();
            self.resync_requested = true;
            self.dropped_commands += 1;
        }
        self.controls.push_back(control);
    }

    fn completion(&self) -> Arc<AtomicU64> {
        let ended = Arc::new(AtomicU64::new(u64::MAX));
        self.pins.lock().expect("Audio pins lock poisoned").completions.push(ended.clone());
        ended
    }

    fn order(&mut self) -> u64 { let order = self.next_order; self.next_order += 1; order }
    fn resolve_time(&self, timing: AudioStartTime) -> i64 {
        match timing {
            AudioStartTime::Tick(tick) => tick_sample(tick, self.sample_rate(), self.config.tick_rate_hz),
            AudioStartTime::Time(duration) => duration_sample(duration, self.sample_rate()),
            AudioStartTime::Immediate => self.clock.transport_position(Instant::now() + self.clock.latency() + Duration::from_secs_f64(f64::from(self.block_frames()) / f64::from(self.sample_rate()))).ceil() as i64,
        }
    }

    pub(super) fn update(&mut self) {
        while let Ok(snapshot) = self.clock_rx.try_recv() {
            self.resync_requested |= snapshot.stats.resync_requests > self.diagnostics.resync_requests;
            self.diagnostics = snapshot.stats;
            self.clock.block_frames = snapshot.block_frames;
            self.clock.generation = snapshot.epoch + self.session_generation;
            self.clock.snapshot = Some(snapshot);
        }
        self.clock.running = self.running();
        let completed = self.diagnostics.callbacks;
        self.instances.retain(|_, instance| instance.ended.load(Ordering::Acquire) > completed);
        {
            let mut pins = self.pins.lock().expect("Audio pins lock poisoned");
            pins.samples.retain(|sample| Arc::strong_count(sample) > 1);
            pins.completions.retain(|ended| Arc::strong_count(ended) > 1);
        }
        self.flush();
    }

    pub(crate) fn flush(&mut self) {
        let pending = std::mem::take(&mut self.pending);
        for play in pending { self.submit_play(play); }
        flush_controls(&self.control_tx, &mut self.controls);
    }

    fn submit_play(&mut self, pending: PendingPlay) {
        let Some(instance) = self.instances.get(&pending.id) else { return; };
        if self.failed.contains(&pending.asset) { self.instances.remove(&pending.id); return; }
        let Some(prepared) = self.samples.get(&pending.asset) else { self.pending.push(pending); return; };
        let sample = &prepared.sample;
        let region = resolve_loop(&pending.config, sample.frames, self.sample_rate());
        if pending.config.looped && region.is_none() { self.instances.remove(&pending.id); return; }
        let command = Command { at: pending.at, epoch: self.epoch, order: pending.order, kind: CommandKind::Play(Play {
            id: pending.id.0, sample: sample.clone(), ended: instance.ended.clone(), bus: instance.bus,
            volume: pending.config.volume.max(0.0), pan: pending.config.pan,
            loop_range: region, position: duration_sample(pending.config.position, self.sample_rate()) as usize,
            fade_frames: duration_sample(pending.config.fade_in, self.sample_rate()) as usize,
        }) };
        if self.command_tx.try_send(command).is_err() { self.pending.push(pending); self.dropped_commands += 1; }
    }
}

fn resolve_loop(config: &AudioPlaybackConfig, frames: usize, rate: u32) -> Option<(usize, usize)> {
    if !config.looped || frames == 0 { return None; }
    let Some((start, end, source_rate)) = config.loop_region else { return Some((0, frames)); };
    if source_rate == 0 || end <= start { return None; }
    let start = tick_sample(start, rate, source_rate) as usize;
    let end = (tick_sample(end, rate, source_rate) as usize).min(frames);
    (end > start).then_some((start, end))
}

fn decode(bytes: Arc<[u8]>, rate: u32) -> Result<Arc<SampleData>, String> {
    let decoded = SymphoniumLoader::new().load_f32_from_source(
        Box::new(std::io::Cursor::new(bytes)), None, Some(rate), ResampleQuality::High, None,
    ).map_err(|error| error.to_string())?;
    let channels = decoded.channels().min(2);
    let frames = decoded.frames();
    if frames == 0 || channels == 0 { return Err("Audio contains no frames".to_owned()); }
    let mut data = Vec::with_capacity(frames * channels);
    for frame in 0..frames { for channel in 0..channels { data.push(decoded.data[channel][frame]); } }
    Ok(Arc::new(SampleData { channels, frames, data }))
}

pub(super) fn prepare_audio(
    mut engine: ResMut<AudioEngine>, assets: Res<Assets<AudioSource>>, server: Res<AssetServer>,
    mut events: MessageReader<AssetEvent<AudioSource>>,
) {
    for event in events.read() { invalidate_prepared(&mut engine, event); }
    let mut completed = Vec::new();
    for (&id, (task, deferred)) in &mut engine.tasks {
        if let Some(result) = block_on(future::poll_once(task)) { completed.push((id, *deferred, result)); }
    }
    for (id, deferred, result) in completed {
        engine.tasks.remove(&id);
        match result {
            Ok(sample) => {
                engine.pins.lock().expect("Audio pins lock poisoned").samples.push(sample.clone());
                engine.samples.insert(id, PreparedSample { sample, deferred });
            },
            Err(error) => { warn!("Audio preparation failed for {id:?}: {error}"); engine.failed.insert(id); },
        }
    }
    for (id, source) in assets.iter() {
        let id = id.untyped();
        let deferred = server.get_path(id).is_some_and(|path| path.path().starts_with("audio/music"));
        let wanted = !deferred || engine.requested.contains(&id);
        if wanted && !engine.samples.contains_key(&id) && !engine.tasks.contains_key(&id) && !engine.failed.contains(&id) {
            let bytes = source.bytes.clone();
            let rate = engine.sample_rate();
            engine.tasks.insert(id, (spawn_decode(bytes, rate), deferred));
        }
    }
    let evict: Vec<_> = engine.samples.iter().filter(|(id, sample)| sample.deferred
        && !engine.retained_music.contains(id) && !engine.requested.contains(id)
        && !engine.instances.values().any(|instance| &instance.asset == *id)).map(|(&id, _)| id).collect();
    for id in evict { engine.samples.remove(&id); }
    engine.requested.clear();
}

fn invalidate_prepared(engine: &mut AudioEngine, event: &AssetEvent<AudioSource>) {
    let (AssetEvent::Modified { id } | AssetEvent::Removed { id }) = event else { return; };
    let id = id.untyped();
    engine.samples.remove(&id);
    engine.failed.remove(&id);
    engine.tasks.remove(&id);
}

pub(super) fn update_audio_engine(mut engine: ResMut<AudioEngine>, mut clock: ResMut<AudioClock>, mut stream: NonSendMut<AudioStream>) {
    engine.update();
    let now = Instant::now();
    let disconnected = !engine.running() || engine.clock.snapshot.is_some_and(|_| !engine.clock.healthy(now));
    if engine.config.output_enabled && disconnected && now >= engine.reconnect_at {
        stream.stream = None;
        let config = engine.config.clone();
        let generation = engine.session_generation + 1_000_000;
        let (mut replacement, replacement_stream) = AudioEngine::build(&config);
        replacement.session_generation = generation;
        replacement.next_id = engine.next_id;
        replacement.next_order = engine.next_order;
        *engine = replacement;
        *stream = replacement_stream;
    }
    *clock = engine.clock.clone();
}

pub(super) fn flush_audio(mut engine: ResMut<AudioEngine>) { engine.flush(); }

fn flush_controls(sender: &Sender<Control>, controls: &mut VecDeque<Control>) {
    while let Some(control) = controls.pop_front() {
        if let Err(error) = sender.try_send(control) {
            controls.push_front(error.into_inner());
            break;
        }
    }
}

fn spawn_decode(bytes: Arc<[u8]>, rate: u32) -> DecodeTask {
    AsyncComputeTaskPool::get().spawn(async move { decode(bytes, rate) })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn playback_estimate_uses_sample_timestamps_and_health_uses_elapsed_time() {
        let now = Instant::now();
        let clock = AudioClock::test_running(now, 48_000, 1);
        assert_eq!(clock.transport_position(now + Duration::from_millis(125)), 54_000.0);
        assert_eq!(clock.transport_position(now - Duration::from_millis(125)), 42_000.0);
        assert!(clock.healthy(now + Duration::from_millis(249)));
        assert!(!clock.healthy(now + Duration::from_millis(251)));
        let jittered = AudioClock::test_running(now + Duration::from_millis(11), 48_480, 1);
        // Arrival jitter does not change a valid output timestamp.
        let mut jittered = jittered;
        jittered.snapshot.as_mut().unwrap().playback_end = now + Duration::from_millis(10);
        assert_eq!(clock.transport_position(now + Duration::from_millis(20)), jittered.transport_position(now + Duration::from_millis(20)));
    }

    #[test]
    fn cancellation_before_preparation_releases_instances_and_pending_work() {
        let config = AudioEngineConfig { output_enabled: false, ..Default::default() };
        let (mut engine, _stream) = AudioEngine::build(&config);
        engine.running.store(true, Ordering::Release);
        let id = engine.play_music(&Handle::default(), AudioPlaybackConfig::default()).unwrap();
        engine.stop_one(id);
        assert!(!engine.contains(id));
        assert!(engine.pending.is_empty());
        engine.play_music(&Handle::default(), AudioPlaybackConfig::default()).unwrap();
        engine.stop_all();
        assert!(engine.instances.is_empty());
        assert!(engine.pending.is_empty());
    }
}
