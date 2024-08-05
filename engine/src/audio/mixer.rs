use std::sync::{atomic::{AtomicBool, AtomicI32, Ordering as MemOrdering}, Mutex};

use rodio::{Sample, Source};
use cpal::FromSample;

use crate::prelude::*;

pub(super) type MySample = f32;
type BoxedSource = Box<dyn Source<Item = MySample> + Send + Sync>;

/// Audio Source that mixes many sounds with accurate timings.
///
/// Internally everything uses `f32` sample format to ensure no clipping.
///
/// (largely follows the design and implementation of Rodio's DynamicMixer)
pub struct PrecisionMixer {
    controller: Arc<PrecisionMixerController>,
    sample_count: i64,
    current_channel: u16,
    playing: Vec<PrecisionMixerActiveTrack>,
}

pub struct PrecisionMixerController {
    reset_triggered: AtomicBool,
    reset_offset: AtomicI32,
    has_pending: AtomicBool,
    tick_rate: f32,
    sample_rate: u32,
    channels: u16,
    pending: Mutex<Vec<PrecisionMixerQueuedTrack>>,
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
            reset_triggered: AtomicBool::new(false),
            reset_offset: AtomicI32::new(0),
            has_pending: AtomicBool::new(false),
            pending: Mutex::new(Vec::with_capacity(16)),
            channels,
            sample_rate,
            tick_rate,
        })
    }
    fn play_at_sample_number<T, S>(&self, start_at_sample_number: Option<i64>, source: T, volume: f32, pan: f32)
    where
        T: Source<Item = S> + Send + Sync + 'static,
        S: Sample + Send + 'static,
        MySample: FromSample<S>,
    {
        if source.channels() > 2 {
            panic!("GameTickMixer does not support > 2 audio channels!");
        }
        let mut source = Box::new(source.convert_samples::<MySample>());
        if let Some(first_sample) = (&mut *source).next() {
            self.pending.lock().unwrap().push(PrecisionMixerQueuedTrack {
                start_at_sample_number,
                first_sample,
                volume,
                pan,
                source: Some(source),
            });
        }
        self.has_pending.store(true, MemOrdering::SeqCst);
    }

    pub fn play_immediately<T, S>(&self, source: T, volume: f32, pan: f32)
    where
        T: Source<Item = S> + Send + Sync + 'static,
        S: Sample + Send + 'static,
        MySample: FromSample<S>,
    {
        self.play_at_sample_number(None, source, volume, pan);
    }

    pub fn play_at_time<T, S>(&self, dur: Duration, source: T, volume: f32, pan: f32)
    where
        T: Source<Item = S> + Send + Sync + 'static,
        S: Sample + Send + 'static,
        MySample: FromSample<S>,
    {
        let seconds = dur.as_secs();
        let nanos = dur.subsec_nanos();
        let start_at_sample_number =
            (seconds as u64 * self.sample_rate as u64) +
            (self.sample_rate as u64 * nanos as u64 / 1_000_000_000);
        self.play_at_sample_number(Some(start_at_sample_number as i64), source, volume, pan);
    }

    pub fn play_at_tick<T, S>(&self, tick: u32, offset_nanos: i32, source: T, volume: f32, pan: f32)
    where
        T: Source<Item = S> + Send + Sync + 'static,
        S: Sample + Send + 'static,
        MySample: FromSample<S>,
    {
        let start_at_sample_number = (
            (tick as f64 * self.sample_rate as f64 / self.tick_rate as f64) as i64 +
            (self.sample_rate as i64 * offset_nanos as i64 / 1_000_000_000)
        ) as i64;
        self.play_at_sample_number(Some(start_at_sample_number), source, volume, pan);
    }

    pub fn trigger_reset(&self, delay_ms: i32) {
        let sample_offset = -delay_ms as i64 * self.sample_rate as i64 / 1000;
        self.reset_offset.store(sample_offset as i32, MemOrdering::SeqCst);
        self.reset_triggered.store(true, MemOrdering::SeqCst);
    }
}

impl Iterator for PrecisionMixer {
    type Item = MySample;

    fn next(&mut self) -> Option<Self::Item> {
        if self.controller.reset_triggered.swap(false, MemOrdering::SeqCst) {
            self.sample_count = self.controller.reset_offset.load(MemOrdering::SeqCst) as i64;
        }
        if self.controller.has_pending.load(MemOrdering::SeqCst) {
            self.process_pending();
        }

        let value = self.mix();

        self.current_channel += 1;
        if self.current_channel >= self.channels() {
            self.current_channel = 0;
            self.sample_count += 1;
        }

        if self.playing.is_empty() {
            Some(MySample::zero_value())
        } else {
            Some(value)
        }
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
            sample_count: 0,
            current_channel: 0,
            playing: Vec::with_capacity(16),
            controller: controller.clone(),
        }
    }
    pub fn controller(&self) -> Arc<PrecisionMixerController> {
        self.controller.clone()
    }

    fn mix(&mut self) -> MySample {
        let mut sum = MySample::zero_value();
        let channels = self.channels();
        for track in self.playing.iter_mut() {
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
                }
                (2, 1) => {
                    // only advance the mono source every other sample
                    // (mix one source sample into both L + R, with panning)
                    if self.current_channel == 0 {
                        sum = sum.saturating_add(track.next_sample * track.volume * pan_l);
                    }
                    if self.current_channel == 1 {
                        sum = sum.saturating_add(track.next_sample * track.volume * pan_r);
                        if let Some(value) = track.source.next() {
                            track.next_sample = value;
                        } else {
                            track.done = true;
                        }
                    }
                }
                (1, 2) => {
                    // consume 2 samples from source and mix them (stereo -> mono)
                    sum = sum.saturating_add(track.next_sample * 0.5 * track.volume * pan_l);
                    if let Some(value) = track.source.next() {
                        sum = sum.saturating_add(value * 0.5 * track.volume * pan_r);
                    } else {
                        track.done = true;
                    }
                    if let Some(value) = track.source.next() {
                        track.next_sample = value;
                    } else {
                        track.done = true;
                    }
                }
                (2, 2) => {
                    // make sure the channels are aligned and not swapped
                    match (self.current_channel, track.current_channel) {
                        (0, 0) => {
                            // left channel of both source and mixer
                            sum = sum.saturating_add(track.next_sample * track.volume * pan_l);
                            if let Some(value) = track.source.next() {
                                track.next_sample = value;
                            } else {
                                track.done = true;
                            }
                        }
                        (1, 1) => {
                            // right channel of both source and mixer
                            sum = sum.saturating_add(track.next_sample * track.volume * pan_r);
                            if let Some(value) = track.source.next() {
                                track.next_sample = value;
                            } else {
                                track.done = true;
                            }
                        }
                        (1, 0) | (0, 1) => {
                            // mismatch! output nothing for this sample to catch up
                        }
                        _ => unreachable!(),
                    }
                }
                _ => panic!("GameTickMixer does not support > 2 audio channels!")
            }
        }
        self.playing.retain(|track| !track.done);
        sum
    }

    fn process_pending(&mut self) {
        if self.current_channel != 0 {
            // we must only start new tracks when our current channel is 0
            // to ensure that channels don't get swapped around
            return;
        }

        let mut pending = self.controller.pending.lock().unwrap();

        // we must ensure that each track is started so that it is
        // perfectly aligned to its desired sample number

        for track in pending.iter_mut() {
            let start_at_sample_number = track.start_at_sample_number
                .unwrap_or(self.sample_count);

            if start_at_sample_number > self.sample_count {
                // we are early, it's not time for this one yet
                continue;
            }

            let Some(mut source) = track.source.take() else {
                continue;
            };

            // if we are already late, we have to skip ahead into the source
            let missed_by = self.sample_count - start_at_sample_number;
            for _ in 0..(missed_by * source.channels() as i64) {
                if let Some(value) = source.next() {
                    track.first_sample = value;
                } else {
                    // the sound is already over before it even started playing ;)
                    continue;
                }
            }

            self.playing.push(PrecisionMixerActiveTrack {
                done: false,
                current_channel: 0,
                pan: track.pan,
                volume: track.volume,
                next_sample: track.first_sample,
                source,
            })
        }

        pending.retain(|track| track.source.is_some());

        let has_pending = !pending.is_empty();
        self.controller.has_pending.store(has_pending, MemOrdering::SeqCst);
    }
}

pub fn init_mixer(
    channels: u16,
    sample_rate: u32,
    tick_rate: f32,
) -> (Arc<PrecisionMixerController>, PrecisionMixer) {
    if channels > 2 {
        panic!("GameTickMixer does not support > 2 audio channels!");
    }
    let controller = PrecisionMixerController::new(channels, sample_rate, tick_rate);
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
