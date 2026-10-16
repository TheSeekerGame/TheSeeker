//! Opt-in audiovisual calibration and movement comparison controls.
use crate::prelude::*;
use theseeker_engine::audio::AudioEngine;
use theseeker_engine::presentation::PresentationDebug;
use std::io::Write;

#[derive(Resource, Default)]
struct Probe { visible: bool, clicks: u8, frame_latency: u32 }
#[derive(Component)]
struct ProbeText;

#[derive(Resource, Default)]
struct TimingLog {
    file: Option<std::io::BufWriter<std::fs::File>>,
    last: Duration,
    paused: bool,
    resynchronizations: u64,
    lines: u64,
}

pub struct TimingProbePlugin;
impl Plugin for TimingProbePlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(Probe { visible: std::env::var_os("SEEKER_TIMING_PROBE").is_some(), clicks: 0, frame_latency: crate::frame_metrics::DEFAULT_FRAME_LATENCY });
        app.init_resource::<TimingLog>().add_plugins(crate::frame_metrics::FrameMetricsPlugin);
        app.add_systems(Startup, (spawn_probe, open_timing_log));
        app.add_systems(Update, (probe_controls, update_probe).chain().in_set(GameTickSet::Post));
        app.add_systems(Last, write_timing_log.after(crate::frame_metrics::finish_frame));
        app.add_systems(GameTickUpdate, gameplay_click);
        app.add_systems(GameTickPost, player_presentation_override.in_set(theseeker_engine::presentation::PresentationSet::Prepare));
    }
}

fn open_timing_log(mut log: ResMut<TimingLog>) {
    let path = std::env::var_os("SEEKER_TIMING_LOG").map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::path::PathBuf::from("target/timing-diagnostics.log"));
    match std::fs::File::create(&path) {
        Ok(file) => {
            let mut file = std::io::BufWriter::new(file);
            let _ = writeln!(file, "The Seeker timing diagnostics — new session {:?}\n\
                One row per second and on pause/resynchronization. Sample frames include all channels.\n\
                click mode: 0=off, 1=callback reference, 2=gameplay. Callback gaps are suspected underruns, not confirmed device losses.", std::time::SystemTime::now());
            let _ = file.flush();
            log.file = Some(file);
            info!("Timing diagnostics: {}", path.display());
        },
        Err(error) => warn!("Cannot create timing log {}: {error}", path.display()),
    }
}

fn write_timing_log(
    mut log: ResMut<TimingLog>, time: Res<Time<Real>>,
    (probe, settings, music, mut metrics): (Res<Probe>, Res<TimingSettings>, Res<crate::music::MusicState>, ResMut<crate::frame_metrics::FrameMetrics>), engine: Res<AudioEngine>,
    (transport, render_metrics): (Res<GameTransport>, Res<crate::frame_metrics::RenderMetrics>), cursor: Res<RenderInterpolation>, debug: Res<PresentationDebug>,
) {
    let changed = log.paused != transport.paused || log.resynchronizations != transport.resynchronizations;
    if !changed && time.elapsed().saturating_sub(log.last) < Duration::from_secs(1) { return; }
    if log.lines >= 86_400 { return; }
    log.last = time.elapsed();
    log.paused = transport.paused;
    log.resynchronizations = transport.resynchronizations;
    log.lines += 1;
    let Some(file) = &mut log.file else { return; };
    let stats = engine.diagnostics;
    let render = render_metrics.take();
    let result = writeln!(file, "t={:.3}s frame_ms={:.3} tick={:.3} paused={} clicks={} rate={} buffer={} lead_ms={:.3} latency_ms={:.3} healthy={} epoch={} resync={} late_plays={} late_controls={} max_late_frames={} queue_pressure={} rejected={} stolen={} callbacks={} callback_max_us={} gaps={} timestamp_fallbacks={} clipped={} history={} proxies={} pixels={} interpolate={} av_ms={:.1} display_ms={:.3} music={:?} music_volume={:.3} frame_latency={} panel={} fps={:.2} frame_avg_ms={:.3} frame_max_ms={:.3} main_avg_ms={:.3} main_max_ms={:.3} sim_avg_ms={:.3} sim_max_ms={:.3} sim_ticks={} focused_frames={}/{} window={}x{} render_frames={} render_prepare_ms={:.3} render_submit_ms={:.3} render_max_ms={:.3}",
        time.elapsed_secs_f64(), time.delta_secs_f64() * 1000.0, cursor.position(), transport.paused, probe.clicks,
        engine.sample_rate(), engine.block_frames(), transport.lead_seconds * 1000.0,
        engine.clock().latency().as_secs_f64() * 1000.0, engine.clock().healthy(Instant::now()), transport.generation,
        transport.resynchronizations, stats.late_plays, stats.late_controls, stats.max_lateness_frames,
        engine.dropped_commands, stats.rejected_events, stats.stolen_voices, stats.callbacks,
        stats.max_callback_micros, stats.delayed_callbacks, stats.timestamp_fallbacks, stats.clipped_samples,
        debug.retained_samples, debug.proxies, debug.player_pixels.unwrap_or(true), !debug.interpolation_disabled,
        settings.av_offset_ms, settings.display_latency_ms, music.current_track, music.volume, probe.frame_latency, probe.visible,
        if metrics.frame_seconds > 0.0 { f64::from(metrics.frames) / metrics.frame_seconds } else { 0.0 },
        metrics.frame_seconds * 1000.0 / f64::from(metrics.frames.max(1)), metrics.max_frame_seconds * 1000.0,
        metrics.main_seconds * 1000.0 / f64::from(metrics.frames.max(1)), metrics.max_main_seconds * 1000.0,
        metrics.simulation_seconds * 1000.0 / f64::from(metrics.frames.max(1)), metrics.max_simulation_seconds * 1000.0, metrics.ticks,
        metrics.focused_frames, metrics.frames, metrics.window_size.x, metrics.window_size.y, render.frames,
        render.prepare_seconds * 1000.0 / f64::from(render.frames.max(1)),
        render.submit_seconds * 1000.0 / f64::from(render.frames.max(1)), render.max_seconds * 1000.0)
        .and_then(|()| file.flush());
    *metrics = crate::frame_metrics::FrameMetrics::default();
    if let Err(error) = result { warn!("Timing log write failed: {error}"); log.file = None; }
}

fn spawn_probe(mut commands: Commands) {
    commands.spawn((ProbeText, Text::default(), TextFont { font_size: FontSize::Px(14.0), ..Default::default() },
        TextColor(Color::WHITE), BackgroundColor(Color::srgba(0.02, 0.02, 0.03, 0.92)),
        Node { position_type: PositionType::Absolute, left: Val::Px(12.0), top: Val::Px(12.0), max_width: Val::Percent(96.0),
            padding: UiRect::all(Val::Px(12.0)), ..Default::default() }, GlobalZIndex(10000)));
}

fn probe_controls(
    keys: Res<ButtonInput<KeyCode>>, mut probe: ResMut<Probe>, mut engine: ResMut<AudioEngine>,
    mut debug: ResMut<PresentationDebug>, mut settings: ResMut<TimingSettings>,
    mut windows: Query<&mut Window, With<bevy::window::PrimaryWindow>>,
) {
    if keys.just_pressed(KeyCode::F9) { probe.visible = !probe.visible; }
    if keys.just_pressed(KeyCode::F8) {
        probe.frame_latency = if probe.frame_latency == 1 { 2 } else { 1 };
        if let Ok(mut window) = windows.single_mut() {
            window.desired_maximum_frame_latency = std::num::NonZeroU32::new(probe.frame_latency);
        }
    }
    if !probe.visible { return; }
    if keys.just_pressed(KeyCode::F10) {
        probe.clicks = (probe.clicks + 1) % 3;
        engine.reference_clicks(probe.clicks == 1);
    }
    if keys.just_pressed(KeyCode::F11) { debug.player_pixels = Some(!debug.player_pixels.unwrap_or(true)); }
    if keys.just_pressed(KeyCode::F12) { debug.interpolation_disabled = !debug.interpolation_disabled; }
    if keys.just_pressed(KeyCode::BracketLeft) { settings.av_offset_ms = (settings.av_offset_ms - 1.0).max(-100.0); }
    if keys.just_pressed(KeyCode::BracketRight) { settings.av_offset_ms = (settings.av_offset_ms + 1.0).min(100.0); }
}

fn gameplay_click(probe: Res<Probe>, time: Res<GameTime>, mut engine: ResMut<AudioEngine>) {
    if probe.clicks == 2 { engine.play_tick_click(time.tick()); }
}

fn player_presentation_override(
    debug: Res<PresentationDebug>,
    mut query: Query<&mut theseeker_engine::gent::TransformGfxFromGent, With<crate::game::player::PlayerGfx>>,
) {
    for mut binding in &mut query { binding.pixel_aligned = debug.player_pixels.unwrap_or(true); }
}

type ProbeDisplayKey = (u64, u8, bool, bool, u64, u32);

fn update_probe(
    probe: Res<Probe>, engine: Res<AudioEngine>, transport: Res<GameTransport>, cursor: Res<RenderInterpolation>,
    (settings, mut last): (Res<TimingSettings>, Local<Option<ProbeDisplayKey>>), debug: Res<PresentationDebug>,
    mut query: Query<(&mut Text, &mut Visibility, &mut BackgroundColor), With<ProbeText>>,
) {
    let Ok((mut text, mut visible, mut background)) = query.single_mut() else { return; };
    *visible = if probe.visible { Visibility::Visible } else { Visibility::Hidden };
    if !probe.visible { *last = None; return; }
    let tick = cursor.tick();
    let key = (tick / 8, probe.clicks, debug.player_pixels.unwrap_or(true), debug.interpolation_disabled, settings.av_offset_ms.to_bits(), probe.frame_latency);
    if *last == Some(key) { return; }
    *last = Some(key);
    let stats = engine.diagnostics;
    let mode = ["off", "callback reference", "gameplay scheduling"][usize::from(probe.clicks)];
    background.0 = if (tick / 8).is_multiple_of(2) { Color::srgba(0.04, 0.08, 0.18, 0.95) }
        else { Color::srgba(0.12, 0.04, 0.07, 0.95) };
    text.0 = format!("TIMING PROBE   12 Hz frame {}   beat {} / 4   bar {}\n\
        tick {}   clicks: {}   {} Hz / {} sample frames\n\
        lead {:.2} ms   AV offset {:+.1} ms   output latency ~{:.2} ms\n\
        late {} (plays {} / controls {}, max {:.2} ms)   queue retries/drops {}   rejected {}   stolen {}\n\
        callback max {} us   gaps {}   timestamp fallbacks {}\n\
        history {} samples / {} sprites   resync {}\n\
        F9 panel   F10 clicks   F11 pixels ({})   F12 interpolation ({})\n\
        [ / ] AV offset   P pause   F8 queued frames ({})",
        tick / 8, tick / 64 % 4 + 1, tick / 256 + 1, tick, mode, engine.sample_rate(), engine.block_frames(),
        transport.lead_seconds * 1000.0, settings.av_offset_ms, engine.clock().latency().as_secs_f64() * 1000.0,
        stats.late_events, stats.late_plays, stats.late_controls, stats.max_lateness_frames as f64 * 1000.0 / f64::from(engine.sample_rate()), engine.dropped_commands, stats.rejected_events, stats.stolen_voices,
        stats.max_callback_micros, stats.delayed_callbacks, stats.timestamp_fallbacks, debug.retained_samples, debug.proxies, transport.resynchronizations,
        debug.player_pixels.unwrap_or(true), !debug.interpolation_disabled, probe.frame_latency);
}
