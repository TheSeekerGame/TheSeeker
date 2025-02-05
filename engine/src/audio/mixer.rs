use std::sync::atomic::{
    AtomicI64, AtomicU64, Ordering as MemOrdering,
};
use std::sync::Mutex;

use cpal::FromSample;
use rodio::{Sample, Source};

use crate::prelude::*;

pub(super) type MySample = f32;
type BoxedSource = Box<dyn Source<Item = MySample> + Send + Sync>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PreciseAudioId(u64);

impl PreciseAudioId {
    fn new() -> Self {
        Self(NEXT_ID.fetch_add(1, MemOrdering::Relaxed))
    }
}

static NEXT_ID: AtomicU64 = AtomicU64::new(0);

/// Audio Source that mixes many sounds with accurate timings.
///
/// Internally everything uses `f32` sample format to ensure no clipping.
///
/// (largely follows the design and implementation of Rodio's DynamicMixer)
pub struct PrecisionMixer {
    controller: Arc<PrecisionMixerController>,
    sample_count: i64,
    current_channel: u16,
}

pub struct PrecisionMixerController {
    sample_count: AtomicI64,
    tick_rate: f32,
    sample_rate: u32,
    channels: u16,
    tracks: Mutex<PrecisionMixerTracks>,
}

#[derive(Default)]
struct PrecisionMixerTracks {
    pending: HashMap<PreciseAudioId, PrecisionMixerQueuedTrack>,
    playing: HashMap<PreciseAudioId, PrecisionMixerActiveTrack>,
    label2id: HashMap<String, HashSet<PreciseAudioId>>,
    id2label: HashMap<PreciseAudioId, String>,
}

impl PrecisionMixerTracks {
    fn add_to_label(&mut self, label: &str, id: PreciseAudioId) {
        if let Some(old_label) = self.id2label.remove(&id) {
            if let Some(l) = self.label2id.get_mut(&old_label) {
                l.remove(&id);
                if l.is_empty() {
                    self.label2id.remove(&old_label);
                }
            }
        }

        if let Some(l) = self.label2id.get_mut(label) {
            l.insert(id);
        } else {
            let mut set = HashSet::default();
            set.insert(id);
            self.label2id.insert(label.to_owned(), set);
        }

        self.id2label.insert(id, label.to_owned());
    }

    fn remove(&mut self, id: PreciseAudioId) {
        self.playing.remove(&id);
        self.pending.remove(&id);

        if let Some(label) = self.id2label.remove(&id) {
            if let Some(l) = self.label2id.get_mut(&label) {
                l.remove(&id);
                if l.is_empty() {
                    self.label2id.remove(&label);
                }
            }
        }
    }

    fn remove_label(&mut self, label: &str) {
        if let Some(l) = self.label2id.get(label) {
            for id in l.iter() {
                self.id2label.remove(id);
                self.pending.remove(id);
                self.playing.remove(id);
            }
            self.label2id.remove(label);
        }
    }
}

struct PrecisionMixerQueuedTrack {
    start_at_sample_number: Option<i64>,
    first_sample: MySample,
    volume: f32,
    pan: f32,
    source: Option<BoxedSource>,
}

struct PrecisionMixerActiveTrack {
    done: bool,
    volume: f32,
    pan: f32,
    current_channel: u16,
    next_sample: MySample,
    source: BoxedSource,
}

impl PrecisionMixerController {
    pub fn new(
        channels: u16,
        sample_rate: u32,
        tick_rate: f32,
    ) -> Arc<PrecisionMixerController> {
        if channels > 2 {
            panic!("PrecisionMixer does not support > 2 audio channels!");
        }
        Arc::new(PrecisionMixerController {
            sample_count: AtomicI64::new(0),
            tracks: Mutex::new(default()),
            channels,
            sample_rate,
            tick_rate,
        })
    }

    pub fn has_playing(&self) -> bool {
        !self.tracks.lock().unwrap().playing.is_empty()
    }

    pub fn reset_sample_counter(&self, new: i64) {
        self.sample_count.store(new, MemOrdering::Relaxed);
    }

    pub fn sample_count(&self) -> i64 {
        self.sample_count.load(MemOrdering::Relaxed)
    }

    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    pub fn stop_one(&self, id: PreciseAudioId) {
        let mut tracks = self.tracks.lock().unwrap();
        tracks.remove(id);
    }

    pub fn stop_all(&self) {
        let mut tracks = self.tracks.lock().unwrap();
        tracks.playing.clear();
        tracks.pending.clear();
        tracks.label2id.clear();
    }

    pub fn stop_label(&self, label: &str) {
        let mut tracks = self.tracks.lock().unwrap();
        tracks.remove_label(label);
    }

    pub fn stop_many(&self, ids: &[PreciseAudioId]) {
        let mut tracks = self.tracks.lock().unwrap();
        for id in ids {
            tracks.remove(*id);
        }
    }

    pub fn stop_many_with_label(&self, ids: &mut Vec<PreciseAudioId>, label: &str) {
        let mut tracks = self.tracks.lock().unwrap();
        ids.retain(|id| {
            let has_label = tracks.id2label.get(id).map(|s| s.as_str()) == Some(label);
            if has_label {
                tracks.remove(*id);
            }
            !has_label
        });
    }

    pub fn cleanup_stale_ids(&self, ids: &mut Vec<PreciseAudioId>) {
        let tracks = self.tracks.lock().unwrap();
        ids.retain(|id| {
            tracks.playing.contains_key(id) || tracks.pending.contains_key(id)
        });
    }

    fn play_at_sample_number<T, S>(
        &self,
        label: Option<&str>,
        start_at_sample_number: Option<i64>,
        source: T,
        volume: f32,
        pan: f32,
    ) -> PreciseAudioId
    where
        T: Source<Item = S> + Send + Sync + 'static,
        S: Sample + Send + 'static,
        MySample: FromSample<S>,
    {
        if source.channels() > 2 {
            panic!("GameTickMixer does not support > 2 audio channels!");
        }
        let mut source = Box::new(source.convert_samples::<MySample>());
        let id = PreciseAudioId::new();
        if let Some(first_sample) = (&mut *source).next() {
            let mut tracks = self.tracks.lock().unwrap();
            tracks.pending.insert(
                id,
                PrecisionMixerQueuedTrack {
                    start_at_sample_number,
                    first_sample,
                    volume,
                    pan,
                    source: Some(source),
                },
            );
            if let Some(label) = label {
                tracks.add_to_label(label, id);
            }
        }
        id
    }

    pub fn play_immediately<T, S>(
        &self,
        label: Option<&str>,
        source: T,
        volume: f32,
        pan: f32,
    ) -> PreciseAudioId
    where
        T: Source<Item = S> + Send + Sync + 'static,
        S: Sample + Send + 'static,
        MySample: FromSample<S>,
    {
        self.play_at_sample_number(label, None, source, volume, pan)
    }

    pub fn play_at_time<T, S>(
        &self,
        label: Option<&str>,
        dur: Duration,
        source: T,
        volume: f32,
        pan: f32,
    ) -> PreciseAudioId
    where
        T: Source<Item = S> + Send + Sync + 'static,
        S: Sample + Send + 'static,
        MySample: FromSample<S>,
    {
        let seconds = dur.as_secs();
        let nanos = dur.subsec_nanos();
        let start_at_sample_number = (seconds as u64 * self.sample_rate as u64)
            + (self.sample_rate as u64 * nanos as u64 / 1_000_000_000);
        self.play_at_sample_number(
            label,
            Some(start_at_sample_number as i64),
            source,
            volume,
            pan,
        )
    }

    pub fn play_at_tick<T, S>(
        &self,
        label: Option<&str>,
        tick: u32,
        offset_nanos: i32,
        source: T,
        volume: f32,
        pan: f32,
    ) -> PreciseAudioId
    where
        T: Source<Item = S> + Send + Sync + 'static,
        S: Sample + Send + 'static,
        MySample: FromSample<S>,
    {
        let start_at_sample_number = ((tick as f64 * self.sample_rate as f64
            / self.tick_rate as f64)
            as i64
            + (self.sample_rate as i64 * offset_nanos as i64 / 1_000_000_000))
            as i64;
        self.play_at_sample_number(
            label,
            Some(start_at_sample_number),
            source,
            volume,
            pan,
        )
    }
}

impl Iterator for PrecisionMixer {
    type Item = MySample;

    fn next(&mut self) -> Option<Self::Item> {
        self.sample_count =
            self.controller.sample_count.load(MemOrdering::Relaxed);

        let mut tracks_guard = self.controller.tracks.lock().unwrap();
        let tracks = &mut *tracks_guard;

        // PROCESS PENDING

        // we must only start new tracks when our current channel is 0
        // to ensure that channels don't get swapped around
        if self.current_channel == 0 {
            // we must ensure that each track is started so that it is
            // perfectly aligned to its desired sample number
            for (id, track) in tracks.pending.iter_mut() {
                let start_at_sample_number =
                    track.start_at_sample_number.unwrap_or(self.sample_count);

                if start_at_sample_number > self.sample_count {
                    // we are early, it's not time for this one yet
                    continue;
                }

                let Some(mut source) = track.source.take() else {
                    continue;
                };

                // if we are already late, we have to skip ahead into the source
                let missed_by = self.sample_count - start_at_sample_number;
                if missed_by > 0 {
                    eprintln!("AUDIO MISSED BY {}", missed_by);
                }
                for _ in 0..(missed_by * source.channels() as i64) {
                    if let Some(value) = source.next() {
                        track.first_sample = value;
                    } else {
                        // the sound is already over before it even started playing ;)
                        continue;
                    }
                }

                tracks.playing.insert(*id, PrecisionMixerActiveTrack {
                    done: false,
                    current_channel: 0,
                    pan: track.pan,
                    volume: track.volume,
                    next_sample: track.first_sample,
                    source,
                });
            }

            tracks.pending.retain(|_id, track| track.source.is_some());
        }

        // MIX

        let mut sum = MySample::zero_value();
        let channels = self.channels();
        for (_id, track) in tracks.playing.iter_mut() {
            let source_channels = track.source.channels();
            let (pan_l, pan_r) = pan_lr(track.pan.clamp(-1.0, 1.0));
            match (channels, source_channels) {
                (1, 1) => {
                    sum = sum.saturating_add(track.next_sample * track.volume);
                    if let Some(value) = track.source.next() {
                        track.next_sample = value;
                    } else {
                        track.done = true;
                    }
                },
                (2, 1) => {
                    // only advance the mono source every other sample
                    // (mix one source sample into both L + R, with panning)
                    if self.current_channel == 0 {
                        sum = sum.saturating_add(
                            track.next_sample * track.volume * pan_l,
                        );
                    }
                    if self.current_channel == 1 {
                        sum = sum.saturating_add(
                            track.next_sample * track.volume * pan_r,
                        );
                        if let Some(value) = track.source.next() {
                            track.next_sample = value;
                        } else {
                            track.done = true;
                        }
                    }
                },
                (1, 2) => {
                    // consume 2 samples from source and mix them (stereo -> mono)
                    sum = sum.saturating_add(
                        track.next_sample * 0.5 * track.volume * pan_l,
                    );
                    if let Some(value) = track.source.next() {
                        sum = sum
                            .saturating_add(value * 0.5 * track.volume * pan_r);
                    } else {
                        track.done = true;
                    }
                    if let Some(value) = track.source.next() {
                        track.next_sample = value;
                    } else {
                        track.done = true;
                    }
                },
                (2, 2) => {
                    // make sure the channels are aligned and not swapped
                    match (
                        self.current_channel,
                        track.current_channel,
                    ) {
                        (0, 0) => {
                            // left channel of both source and mixer
                            sum = sum.saturating_add(
                                track.next_sample * track.volume * pan_l,
                            );
                            if let Some(value) = track.source.next() {
                                track.next_sample = value;
                                track.current_channel += 1;
                                if track.current_channel
                                    >= track.source.channels()
                                {
                                    track.current_channel = 0;
                                }
                            } else {
                                track.done = true;
                            }
                        },
                        (1, 1) => {
                            // right channel of both source and mixer
                            sum = sum.saturating_add(
                                track.next_sample * track.volume * pan_r,
                            );
                            if let Some(value) = track.source.next() {
                                track.next_sample = value;
                                track.current_channel += 1;
                                if track.current_channel
                                    >= track.source.channels()
                                {
                                    track.current_channel = 0;
                                }
                            } else {
                                track.done = true;
                            }
                        },
                        (1, 0) | (0, 1) => {
                            // mismatch! output nothing for this sample to catch up
                        },
                        _ => unreachable!(),
                    }
                },
                _ => {
                    panic!("GameTickMixer does not support > 2 audio channels!")
                },
            }
        }
        tracks.playing.retain(|_id, track| !track.done);

        // BOOKKEEPING

        self.current_channel += 1;
        if self.current_channel >= self.channels() {
            self.current_channel = 0;
            self.sample_count = self
                .controller
                .sample_count
                .fetch_add(1, MemOrdering::Relaxed);
        }

        Some(sum)
    }
}

impl Source for PrecisionMixer {
    #[inline]
    fn current_frame_len(&self) -> Option<usize> {
        None
    }

    #[inline]
    fn channels(&self) -> u16 {
        self.controller.channels
    }

    #[inline]
    fn sample_rate(&self) -> u32 {
        self.controller.sample_rate
    }

    #[inline]
    fn total_duration(&self) -> Option<Duration> {
        None
    }
}

impl PrecisionMixer {
    pub fn new(controller: Arc<PrecisionMixerController>) -> Self {
        PrecisionMixer {
            current_channel: 0,
            sample_count: 0,
            controller: controller.clone(),
        }
    }

    pub fn controller(&self) -> Arc<PrecisionMixerController> {
        self.controller.clone()
    }
}

pub fn init_mixer(
    channels: u16,
    sample_rate: u32,
    tick_rate: f32,
) -> (
    Arc<PrecisionMixerController>,
    PrecisionMixer,
) {
    if channels > 2 {
        panic!("GameTickMixer does not support > 2 audio channels!");
    }
    let controller =
        PrecisionMixerController::new(channels, sample_rate, tick_rate);
    let mixer = PrecisionMixer::new(controller.clone());
    (controller, mixer)
}

fn pan_lr(pan: f32) -> (f32, f32) {
    if pan == 0.0 {
        (1.0, 1.0)
    } else if pan >= -1.0 && pan < 0.0 {
        (1.0, pan + 1.0)
    } else if pan <= 1.0 && pan > 0.0 {
        (1.0 - pan, 1.0)
    } else {
        (0.0, 0.0)
    }
}
