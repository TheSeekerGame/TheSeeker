//! Music owns its voices independently of actors and ambience scripts.
use crate::prelude::*;
use bevy_common_assets::toml::TomlAssetPlugin;
use bevy::audio::AudioSource;
use theseeker_engine::audio::{AudioBus, AudioEngine, AudioInstanceId, AudioPlaybackConfig, AudioStartTime, AudioSet};
use theseeker_engine::audio::transport::{next_boundary, TICKS_PER_BEAT};

#[derive(Debug, Clone, Deserialize, TypePath)]
#[serde(deny_unknown_fields)]
pub struct MusicTrack {
    pub audio_asset: String,
    pub sample_rate: u32,
    pub first_downbeat: u64,
    pub loop_start: u64,
    pub loop_end: u64,
    pub gain: f32,
    pub beats_per_bar: u32,
}

impl MusicTrack {
    fn valid(&self) -> bool {
        self.sample_rate > 0 && self.loop_start <= self.first_downbeat
            && self.first_downbeat < self.loop_end && self.gain.is_finite() && self.gain >= 0.0
            && self.beats_per_bar > 0 && self.loop_end > self.loop_start
            && (u128::from(self.loop_end - self.loop_start) * 3).is_multiple_of(u128::from(self.sample_rate) * 2)
    }
}

#[derive(Asset, TypePath, Deserialize)]
pub struct MusicLibrary { pub tracks: HashMap<String, MusicTrack> }
#[derive(Resource)]
struct MusicLibraryHandle(Handle<MusicLibrary>);
impl FromWorld for MusicLibraryHandle {
    fn from_world(world: &mut World) -> Self { Self(world.resource::<AssetServer>().load("audio/music/tracks.music.toml")) }
}

#[derive(Debug, Clone, Copy, Default)]
#[cfg_attr(not(test), expect(dead_code, reason = "Adaptive music API; gameplay so far only starts a track on the next beat."))]
pub enum MusicBoundary { Immediate, #[default] NextBeat, NextBar }
#[derive(Message, Debug)]
#[expect(dead_code, reason = "Adaptive music API; gameplay so far only plays one track.")]
pub enum MusicCommand {
    Play { track: String, boundary: MusicBoundary },
    Transition { track: String, boundary: MusicBoundary, fade_bars: u32 },
    Stop { boundary: MusicBoundary },
    SetVolume(f32),
}

struct PlayingMusic {
    id: AudioInstanceId,
    key: String,
    handle: Handle<AudioSource>,
    track: MusicTrack,
    origin: u64,
}
struct PendingMusic { key: String, boundary: MusicBoundary, fade_bars: u32, origin: Option<u64> }
#[derive(Resource)]
pub struct MusicState {
    current: Option<PlayingMusic>,
    fading: Vec<PlayingMusic>,
    pending: Option<PendingMusic>,
    pub volume: f32,
    pub current_track: Option<String>,
}
impl Default for MusicState {
    fn default() -> Self { Self { current: None, fading: Vec::new(), pending: None, volume: 1.0, current_track: None } }
}

pub struct MusicPlugin;
impl Plugin for MusicPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(TomlAssetPlugin::<MusicLibrary>::new(&["music.toml"]));
        app.init_resource::<MusicLibraryHandle>().init_resource::<MusicState>().add_message::<MusicCommand>();
        app.add_systems(OnEnter(AppState::InGame), start_game_music);
        app.add_systems(OnExit(AppState::InGame), stop_game_music);
        app.add_systems(Update, music_controller.in_set(GameTickSet::Pre).after(AudioSet::Update));
    }
}

fn start_game_music(mut commands: MessageWriter<MusicCommand>) {
    commands.write(MusicCommand::Play { track: "main".to_owned(), boundary: MusicBoundary::NextBeat });
}
fn stop_game_music(mut state: ResMut<MusicState>, mut engine: ResMut<AudioEngine>) {
    state.pending = None;
    state.current_track = None;
    engine.set_command_time(AudioStartTime::Immediate);
    if let Some(current) = state.current.take() { engine.stop_one(current.id); }
    for fading in state.fading.drain(..) { engine.stop_one(fading.id); }
}

#[derive(bevy::ecs::system::SystemParam)]
struct MusicAssets<'w> {
    library: Res<'w, MusicLibraryHandle>,
    libraries: Res<'w, Assets<MusicLibrary>>,
    preloaded: Res<'w, PreloadedAssets>,
}

fn music_controller(
    mut commands: MessageReader<MusicCommand>, mut state: ResMut<MusicState>,
    mut engine: ResMut<AudioEngine>, assets: MusicAssets, time: Res<GameTime>,
) {
    let tick = time.total_ticks();
    for command in commands.read() { apply_music_command(command, &mut state, &mut engine, tick); }
    recover_music(&mut state, &engine);
    state.fading.retain(|music| engine.contains(music.id));
    let Some(library) = assets.libraries.get(&assets.library.0) else { return; };
    if state.current.is_none() { prepare_default_music(library, &assets.preloaded, &mut engine); }
    try_start_music(&mut state, &mut engine, library, &assets.preloaded, tick);
    let handles: Vec<_> = state.current.iter().chain(&state.fading).map(|music| music.handle.clone()).collect();
    engine.retain_music(&handles);
}

fn prepare_default_music(library: &MusicLibrary, assets: &PreloadedAssets, engine: &mut AudioEngine) {
    let Some(track) = library.tracks.get("main") else { return; };
    let Some(handle) = music_handle(assets, track) else { return; };
    engine.prepare(&handle);
}

fn music_handle(assets: &PreloadedAssets, track: &MusicTrack) -> Option<Handle<AudioSource>> {
    assets.get_multi_asset(&track.audio_asset)?.first().map(|handle| handle.clone().typed::<AudioSource>())
}

fn boundary_tick(tick: u64, boundary: MusicBoundary, beats_per_bar: u32) -> u64 {
    match boundary {
        MusicBoundary::Immediate => tick,
        MusicBoundary::NextBeat => next_boundary(tick, TICKS_PER_BEAT),
        MusicBoundary::NextBar => next_boundary(tick, TICKS_PER_BEAT * u64::from(beats_per_bar)),
    }
}

fn apply_music_command(command: &MusicCommand, state: &mut MusicState, engine: &mut AudioEngine, tick: u64) {
    match command {
        MusicCommand::Play { track, boundary } => state.pending = Some(PendingMusic { key: track.clone(), boundary: *boundary, fade_bars: 1, origin: None }),
        MusicCommand::Transition { track, boundary, fade_bars } => state.pending = Some(PendingMusic { key: track.clone(), boundary: *boundary, fade_bars: *fade_bars, origin: None }),
        MusicCommand::Stop { boundary } => stop_music(state, engine, tick, *boundary),
        MusicCommand::SetVolume(volume) => set_music_volume(state, engine, *volume),
    }
}

fn stop_music(state: &mut MusicState, engine: &mut AudioEngine, tick: u64, boundary: MusicBoundary) {
    state.pending = None;
    state.current_track = None;
    let beats = state.current.as_ref().map_or(4, |music| music.track.beats_per_bar);
    let at = boundary_tick(tick, boundary, beats);
    if let Some(current) = state.current.take() { state.fading.push(current); }
    for music in &state.fading {
        engine.fade(music.id, 0.0, AudioStartTime::Tick(at), Duration::from_millis(10), true);
    }
}

fn set_music_volume(state: &mut MusicState, engine: &mut AudioEngine, volume: f32) {
    if !volume.is_finite() { return; }
    state.volume = volume.clamp(0.0, 2.0);
    engine.set_bus_volume(AudioBus::Music, state.volume);
}

fn recover_music(state: &mut MusicState, engine: &AudioEngine) {
    if state.pending.is_some() || state.current.as_ref().is_none_or(|music| engine.contains(music.id)) { return; }
    let Some(current) = state.current.take() else { return; };
    state.pending = Some(PendingMusic { key: current.key, boundary: MusicBoundary::NextBeat, fade_bars: 0, origin: Some(current.origin) });
}

fn try_start_music(state: &mut MusicState, engine: &mut AudioEngine, library: &MusicLibrary, assets: &PreloadedAssets, tick: u64) {
    let Some(pending) = &state.pending else { return; };
    let Some(track) = library.tracks.get(&pending.key).filter(|track| track.valid()) else {
        warn!("Music track '{}' is missing or has invalid timing metadata", pending.key);
        state.pending = None;
        return;
    };
    let Some(handle) = music_handle(assets, track) else { return; };
    engine.prepare(&handle);
    if engine.preparation_failed(&handle) { state.pending = None; return; }
    if !engine.is_prepared(&handle) || !engine.running() { return; }
    let at = boundary_tick(tick, pending.boundary, track.beats_per_bar);
    let fade = Duration::from_secs_f64(f64::from(pending.fade_bars) * f64::from(track.beats_per_bar) * 2.0 / 3.0);
    let origin = pending.origin.unwrap_or(at);
    let config = music_playback(track, at, origin, fade);
    engine.set_bus_volume(AudioBus::Music, state.volume);
    let Some(id) = engine.play_music(&handle, config) else { return; };
    if let Some(current) = state.current.take() {
        engine.fade(current.id, 0.0, AudioStartTime::Tick(at), fade, true);
        state.fading.push(current);
    }
    let key = pending.key.clone();
    state.current_track = Some(key.clone());
    state.current = Some(PlayingMusic { id, key, handle, track: track.clone(), origin });
    state.pending = None;
}

fn music_playback(track: &MusicTrack, at: u64, origin: u64, fade: Duration) -> AudioPlaybackConfig {
    let elapsed = at.saturating_sub(origin) as f64 / f64::from(theseeker_engine::audio::transport::TICK_RATE_HZ);
    AudioPlaybackConfig {
        timing: AudioStartTime::Tick(at), volume: track.gain, looped: true,
        loop_region: Some((track.loop_start, track.loop_end, track.sample_rate)),
        position: Duration::from_secs_f64(track.first_downbeat as f64 / f64::from(track.sample_rate) + elapsed),
        fade_in: fade, ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn configured_soundtrack_is_an_exact_480_beat_loop() {
        let library: MusicLibrary = toml::from_str(include_str!("../../assets/audio/music/tracks.music.toml")).unwrap();
        let track = &library.tracks["main"];
        assert!(track.valid());
        assert_eq!((track.loop_end - track.loop_start) * 3 / (u64::from(track.sample_rate) * 2), 480);
        let recovered = music_playback(track, 64 + 96 * 321, 64, Duration::ZERO);
        assert_eq!(recovered.position, Duration::from_secs(321));
        assert_eq!(recovered.loop_region, Some((0, 15_360_000, 48_000)));
        assert_eq!(recovered.volume, 0.04);
    }

    #[test]
    fn beat_and_bar_requests_always_select_a_strictly_later_boundary() {
        assert_eq!(boundary_tick(64, MusicBoundary::NextBeat, 4), 128);
        assert_eq!(boundary_tick(255, MusicBoundary::NextBar, 4), 256);
        assert_eq!(boundary_tick(256, MusicBoundary::NextBar, 4), 512);
        assert_eq!(boundary_tick(192, MusicBoundary::NextBar, 3), 384);
        assert_eq!(boundary_tick(65, MusicBoundary::Immediate, 4), 65);
    }
}
