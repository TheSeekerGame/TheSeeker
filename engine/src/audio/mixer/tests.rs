use super::*;
use crossbeam_channel::{bounded, Sender};

struct Harness {
    mixer: Mixer,
    commands: Sender<Command>,
    controls: Sender<Control>,
    samples: Vec<Arc<SampleData>>,
    completions: Vec<Arc<AtomicU64>>,
}

impl Harness {
    fn new(capacity: usize, sfx: usize, background: usize) -> Self {
        let (commands, receiver) = bounded(capacity);
        let (controls, control_receiver) = bounded(capacity);
        Self { mixer: Mixer::new(receiver, control_receiver, sfx, background, capacity, 48_000, 96),
            commands, controls, samples: Vec::new(), completions: Vec::new() }
    }

    fn play(&mut self, at: i64, data: Vec<f32>, loop_range: Option<(usize, usize)>, bus: AudioBus) -> u64 {
        let id = self.completions.len() as u64 + 1;
        let sample = Arc::new(SampleData { frames: data.len(), channels: 1, data });
        let ended = Arc::new(AtomicU64::new(u64::MAX));
        self.samples.push(sample.clone());
        self.completions.push(ended.clone());
        self.commands.try_send(Command { at, epoch: self.mixer.epoch, order: id, kind: CommandKind::Play(Play {
            id, sample, ended, bus, volume: 1.0, pan: -1.0, loop_range, position: 0, fade_frames: 0,
        }) }).unwrap();
        id
    }

    fn render(&mut self, frames: usize) -> Vec<f32> {
        self.mixer.begin_callback();
        let mut stereo = vec![0.0; frames * 2];
        self.mixer.render(&mut stereo);
        stereo.as_chunks::<2>().0.iter().map(|frame| frame[0]).collect()
    }

    fn stop(&self, at: i64, id: u64) {
        self.controls.try_send(Control::Event(Command { at, epoch: self.mixer.epoch,
            order: 1000, kind: CommandKind::Stop(id) })).unwrap();
    }
}

#[test]
fn gameplay_clicks_have_exact_spacing_for_arbitrary_callback_partitions() {
    for partitions in [&[256][..], &[1, 17, 511, 1024, 3][..], &[479, 500, 501][..]] {
        let mut harness = Harness::new(128, 8, 2);
        for tick in 0..96 { harness.play(tick * 500, vec![1.0], None, AudioBus::Sfx); }
        let mut output = Vec::new();
        let mut block = 0;
        while output.len() < 48_000 {
            let size = partitions[block % partitions.len()].min(48_000 - output.len());
            output.extend(harness.render(size));
            block += 1;
        }
        let onsets: Vec<_> = output.iter().enumerate().filter_map(|(index, &value)| (value != 0.0).then_some(index)).collect();
        assert_eq!(onsets, (0..96).map(|tick| tick * 500).collect::<Vec<_>>());
        assert_eq!(harness.mixer.stats.late_events, 0);
    }
}

#[test]
fn start_stop_and_loop_changes_are_applied_inside_the_buffer() {
    let mut harness = Harness::new(16, 2, 2);
    let id = harness.play(3, vec![1.0, 2.0, 3.0, 4.0], Some((1, 3)), AudioBus::Music);
    harness.stop(10, id);
    assert_eq!(harness.render(14), [0.0, 0.0, 0.0, 1.0, 2.0, 3.0, 2.0, 3.0, 2.0, 3.0, 0.0, 0.0, 0.0, 0.0]);
    assert_eq!(harness.completions[0].load(Ordering::Acquire), 1);
    assert_eq!(Arc::strong_count(&harness.samples[0]), 1);
}

#[test]
fn late_one_shots_keep_the_original_onset_and_expired_ones_are_silent() {
    let mut harness = Harness::new(16, 2, 2);
    harness.render(3);
    harness.play(1, vec![1.0, 2.0, 3.0, 4.0], None, AudioBus::Sfx);
    harness.play(0, vec![9.0], None, AudioBus::Sfx);
    assert_eq!(harness.render(4), [3.0, 4.0, 0.0, 0.0]);
    assert_eq!(harness.mixer.stats.late_events, 2);
}

#[test]
fn pause_freezes_music_playhead_and_resume_preserves_phase() {
    let mut harness = Harness::new(16, 2, 2);
    harness.play(0, vec![1.0, 2.0, 3.0, 4.0], Some((0, 4)), AudioBus::Music);
    harness.controls.try_send(Control::Pause(3)).unwrap();
    assert_eq!(harness.render(7), [1.0, 2.0, 3.0, 0.0, 0.0, 0.0, 0.0]);
    assert_eq!(harness.mixer.transport_frame, 3);
    assert_eq!(harness.mixer.mapping.position(2.0), 2.0);
    assert_eq!(harness.mixer.mapping.position(6.0), 3.0);
    harness.controls.try_send(Control::ResumeAfter(2)).unwrap();
    assert_eq!(harness.render(6), [0.0, 0.0, 4.0, 1.0, 2.0, 3.0]);
    assert_eq!(harness.mixer.transport_frame, 7);
}

#[test]
fn voice_exhaustion_protects_music_and_completes_stolen_sfx() {
    let mut harness = Harness::new(16, 1, 1);
    harness.play(0, vec![1.0], Some((0, 1)), AudioBus::Music);
    harness.play(0, vec![2.0], Some((0, 1)), AudioBus::Sfx);
    harness.play(0, vec![3.0], Some((0, 1)), AudioBus::Sfx);
    harness.play(0, vec![9.0], Some((0, 1)), AudioBus::Ambience);
    assert_eq!(harness.render(2), [4.0, 4.0]);
    assert_eq!(harness.mixer.stats.stolen_voices, 1);
    assert_eq!(harness.mixer.stats.rejected_events, 1);
    assert_ne!(harness.completions[1].load(Ordering::Acquire), u64::MAX);
    assert_ne!(harness.completions[3].load(Ordering::Acquire), u64::MAX);
}

#[test]
fn saturated_event_storage_does_not_lose_a_stop() {
    let mut harness = Harness::new(1, 1, 1);
    let id = harness.play(0, vec![1.0], Some((0, 1)), AudioBus::Music);
    harness.render(1);
    harness.play(100, vec![2.0], None, AudioBus::Sfx);
    harness.render(1);
    harness.play(200, vec![3.0], None, AudioBus::Sfx);
    harness.render(1);
    harness.stop(3, id);
    assert_eq!(harness.render(2), [0.0, 0.0]);
    assert_eq!(harness.mixer.stats.rejected_events, 1);
}

#[test]
fn stop_all_cancels_queued_and_future_voices_but_allows_new_requests() {
    let mut harness = Harness::new(8, 2, 2);
    harness.play(8, vec![5.0], None, AudioBus::Sfx);
    harness.controls.try_send(Control::Event(Command { at: 0, epoch: 1, order: 2, kind: CommandKind::StopAll(1) })).unwrap();
    harness.play(2, vec![1.0], None, AudioBus::Sfx);
    assert_eq!(harness.render(10), [0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0]);
    assert_ne!(harness.completions[0].load(Ordering::Acquire), u64::MAX);
}

#[test]
fn new_epoch_rejects_old_requests_and_starts_at_explicit_phase() {
    let mut harness = Harness::new(8, 2, 2);
    harness.play(0, vec![5.0], Some((0, 1)), AudioBus::Music);
    harness.controls.try_send(Control::Reset { frame: 500, epoch: 2 }).unwrap();
    harness.controls.try_send(Control::ResumeAfter(4)).unwrap();
    assert_eq!(harness.render(6), [0.0; 6]);
    assert_eq!(harness.mixer.transport_frame, 502);
    assert_eq!(harness.mixer.epoch, 2);
    assert_ne!(harness.completions[0].load(Ordering::Acquire), u64::MAX);
}

#[test]
fn crossfade_and_music_volume_are_independent_and_sample_scheduled() {
    let mut harness = Harness::new(16, 2, 2);
    let old = harness.play(0, vec![1.0], Some((0, 1)), AudioBus::Music);
    harness.play(0, vec![0.25], Some((0, 1)), AudioBus::Sfx);
    harness.render(2);
    let new = harness.play(2, vec![1.0], Some((0, 1)), AudioBus::Music);
    for (order, kind) in [
        CommandKind::Gain { id: old, volume: 0.0, frames: 4, stop_after: true },
        CommandKind::Gain { id: new, volume: 0.0, frames: 0, stop_after: false },
        CommandKind::Gain { id: new, volume: 1.0, frames: 4, stop_after: false },
    ].into_iter().enumerate() {
        harness.controls.try_send(Control::Event(Command { at: 2, epoch: 1, order: 100 + order as u64, kind })).unwrap();
    }
    harness.controls.try_send(Control::Event(Command { at: 4, epoch: 1, order: 200,
        kind: CommandKind::BusGain { bus: AudioBus::Music, volume: 0.5 } })).unwrap();
    assert_eq!(harness.render(6), [1.25, 1.25, 0.75, 0.75, 0.75, 0.75]);
    assert_ne!(harness.completions[0].load(Ordering::Acquire), u64::MAX);
    assert_eq!(harness.completions[2].load(Ordering::Acquire), u64::MAX);
}

#[test]
fn expired_one_shot_cannot_steal_an_audible_voice() {
    let mut harness = Harness::new(8, 1, 1);
    harness.play(0, vec![1.0], Some((0, 1)), AudioBus::Sfx);
    harness.render(100);
    harness.play(1, vec![5.0], None, AudioBus::Sfx);
    assert_eq!(harness.render(2), [1.0, 1.0]);
    assert_eq!(harness.mixer.stats.stolen_voices, 0);
}
