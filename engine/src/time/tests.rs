use super::*;
use bevy::ecs::system::SystemState;

#[derive(Resource, Default)]
struct Executed(Vec<u64>);

fn record(time: Res<GameTime>, clock: Res<ClockTime>, mut executed: ResMut<Executed>) {
    assert_eq!(clock.delta(), Duration::from_secs_f64(time.seconds_per_tick()));
    executed.0.push(time.tick());
}

struct Harness { world: World, state: ClockState, origin: Instant }
impl Harness {
    fn new() -> Self {
        let mut app = App::new();
        app.add_plugins(GameTimePlugin);
        let config = AudioEngineConfig { output_enabled: false, ..Default::default() };
        // Re-exported constructor keeps the test independent of device hardware.
        let engine = crate::audio::test_engine(&config);
        app.insert_resource(config).insert_resource(engine).init_resource::<AudioClock>().init_resource::<Executed>();
        app.add_systems(GameTickUpdate, record);
        Self { world: std::mem::take(app.world_mut()), state: ClockState::default(), origin: Instant::now() }
    }

    fn step(&mut self, milliseconds: u64) {
        let mut params = SystemState::<(ClockInputs, ClockOutputs, ResMut<AudioEngine>)>::new(&mut self.world);
        let (inputs, mut out, mut engine) = params.get_mut(&mut self.world).expect("fixture system parameters are valid");
        advance_master_clock(inputs, &mut out, &mut engine, &mut self.state, self.origin + Duration::from_millis(milliseconds));
        params.apply(&mut self.world);
        run_gametickupdate_schedule(&mut self.world);
    }
}

#[test]
fn silent_transport_pauses_and_resumes_without_skipping_simulation_ticks() {
    let mut harness = Harness::new();
    harness.step(0);
    harness.step(25);
    harness.world.resource_mut::<GameTransport>().paused = true;
    harness.step(40);
    let frozen = harness.world.resource::<GameTime>().tick();
    harness.step(500);
    assert_eq!(harness.world.resource::<GameTime>().tick(), frozen);
    harness.world.resource_mut::<GameTransport>().paused = false;
    harness.step(510);
    assert!(harness.world.resource::<GameTime>().tick() > frozen);
    let ticks = &harness.world.resource::<Executed>().0;
    assert_eq!(*ticks, (0..ticks.len() as u64).collect::<Vec<_>>());
}

#[test]
fn short_stalls_catch_up_but_severe_backlogs_resynchronize_without_skipping() {
    let mut harness = Harness::new();
    harness.step(0);
    harness.step(50);
    assert_eq!(harness.world.resource::<GameTransport>().resynchronizations, 0);
    let before = harness.world.resource::<GameTime>().tick();
    harness.step(2000);
    assert_eq!(harness.world.resource::<GameTime>().tick(), before);
    assert_eq!(harness.world.resource::<GameTransport>().resynchronizations, 1);
    harness.step(2020);
    let ticks = &harness.world.resource::<Executed>().0;
    assert_eq!(*ticks, (0..ticks.len() as u64).collect::<Vec<_>>());
}

#[test]
fn device_reconnection_requests_a_new_epoch_and_keeps_the_current_tick() {
    let mut harness = Harness::new();
    harness.step(0);
    harness.step(30);
    let tick = harness.world.resource::<GameTime>().tick();
    harness.world.insert_resource(AudioClock::test_running(harness.origin + Duration::from_millis(40), 0, 1));
    harness.step(40);
    assert_eq!(harness.world.resource::<GameTime>().tick(), tick);
    assert!(harness.state.awaiting_generation.is_some());
    assert_eq!(harness.world.resource::<GameTransport>().resynchronizations, 1);
}

#[test]
fn losing_a_device_during_epoch_handshake_returns_to_silent_progress() {
    let mut harness = Harness::new();
    harness.step(0);
    harness.world.insert_resource(AudioClock::test_running(harness.origin + Duration::from_millis(10), 0, 1));
    harness.step(10);
    assert!(harness.state.awaiting_generation.is_some());
    let tick = harness.world.resource::<GameTime>().tick();
    harness.world.insert_resource(AudioClock::default());
    harness.step(30);
    assert!(harness.state.awaiting_generation.is_none());
    assert!(harness.world.resource::<GameTime>().tick() > tick);
}

#[test]
fn lead_calibration_uses_sustained_frame_pacing_instead_of_one_loading_hitch() {
    let mut intervals = FrameIntervals::default();
    for _ in 0..31 { intervals.record(1.0 / 60.0); }
    assert_eq!(intervals.record(0.09), Some(1.0 / 60.0));
    for _ in 0..32 { intervals.record(1.0 / 30.0); }
    assert_eq!(intervals.record(1.0 / 30.0), Some(1.0 / 30.0));
    assert_eq!(intervals.record(2.0), None);
}

#[test]
fn slower_rendering_grows_lead_without_resetting_audio_or_skipping_ticks() {
    let mut harness = Harness::new();
    harness.state.audio_active = true;
    harness.state.device_generation = 1;
    for milliseconds in (0..=1600).step_by(40) {
        harness.world.insert_resource(AudioClock::test_running(
            harness.origin + Duration::from_millis(milliseconds), milliseconds as i64 * 48, 1));
        harness.step(milliseconds);
    }
    let lead = harness.world.resource::<GameTransport>().lead_seconds;
    assert!(lead > 4.0 / 96.0);
    // A late-play report and a faster display must not reset or shrink the timeline.
    harness.world.resource_mut::<AudioEngine>().diagnostics.late_plays += 1;
    for milliseconds in (1610..=2000).step_by(10) {
        harness.world.insert_resource(AudioClock::test_running(
            harness.origin + Duration::from_millis(milliseconds), milliseconds as i64 * 48, 1));
        harness.step(milliseconds);
    }
    assert_eq!(harness.world.resource::<GameTransport>().lead_seconds, lead);
    assert_eq!(harness.world.resource::<GameTransport>().resynchronizations, 0);
    assert!(harness.state.awaiting_generation.is_none());
    let ticks = &harness.world.resource::<Executed>().0;
    assert_eq!(*ticks, (0..ticks.len() as u64).collect::<Vec<_>>());
}

#[test]
fn a_few_startup_frames_do_not_inflate_the_persistent_scheduling_lead() {
    let mut intervals = FrameIntervals::default();
    for _ in 0..3 { assert_eq!(intervals.record(0.08), None); }
    for _ in 0..28 { assert_eq!(intervals.record(1.0 / 60.0), None); }
    assert_eq!(intervals.record(1.0 / 60.0), Some(1.0 / 60.0));
}

#[test]
fn epoch_handshake_waits_for_audio_without_advancing_its_clock_estimate() {
    let mut harness = Harness::new();
    harness.step(0);
    harness.step(30);
    harness.world.insert_resource(AudioClock::test_running(harness.origin + Duration::from_millis(40), 0, 1));
    harness.step(40);
    let seconds = harness.state.seconds;
    let tick = harness.world.resource::<GameTime>().tick();
    harness.step(80);
    assert_eq!(harness.state.seconds, seconds);
    assert_eq!(harness.world.resource::<GameTime>().tick(), tick);
    let epoch = harness.state.awaiting_generation.unwrap();
    harness.world.insert_resource(AudioClock::test_running(
        harness.origin + Duration::from_millis(90), (seconds * 48_000.0).round() as i64, epoch));
    harness.step(90);
    assert!(harness.state.awaiting_generation.is_none());
    assert_eq!(harness.world.resource::<GameTransport>().resynchronizations, 1);
    assert!((harness.state.seconds - seconds).abs() < 1.0 / 48_000.0);
}
