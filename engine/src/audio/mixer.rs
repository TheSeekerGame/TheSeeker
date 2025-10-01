use std::sync::atomic::{AtomicI64, AtomicU64, Ordering as MemOrdering};
use std::sync::Mutex;

// NEW: light-weight lock-free channels for communicating with the audio callback
use crossbeam_channel::{unbounded, Receiver, Sender, TryRecvError};
use std::cmp::Reverse;
use std::collections::BinaryHeap;

use rodio::cpal::FromSample;
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

// ============================================================================
//  Channel messages exchanged between game logic and the audio mixing thread.
// ============================================================================

#[derive(Debug)]
enum PrecisionMixerCommand {
    /// Queue a new sound for playback.
    Add {
        id: PreciseAudioId,
        label: Option<String>,
        track: PrecisionMixerQueuedTrack,
    },
    /// Stop one specific sound by its id.
    StopOne(PreciseAudioId),
    /// Stop a collection of ids.
    StopMany(Vec<PreciseAudioId>),
    /// Stop all currently playing/queued sounds that share the same label.
    StopLabel(String),
    /// Stop everything.
    StopAll,
}

/// Audio Source that mixes many sounds with accurate timings.
///
/// Internally everything uses `f32` sample format to ensure no clipping.
///
/// (largely follows the design and implementation of Rodio's DynamicMixer)
pub struct PrecisionMixer {
    controller: Arc<PrecisionMixerController>,
    sample_count: i64,
    current_channel: u16,

    // NEW: communication primitives + local state ---------------------------------
    cmd_rx: Receiver<PrecisionMixerCommand>,
    finished_tx: Sender<PreciseAudioId>,

    // pending → BinaryHeap keyed by start sample (min-heap via Reverse)
    pending_heap: BinaryHeap<(Reverse<i64>, PreciseAudioId)>,
    pending: HashMap<PreciseAudioId, PrecisionMixerQueuedTrack>,

    // active sounds
    playing: HashMap<PreciseAudioId, PrecisionMixerActiveTrack>,
    label2id: HashMap<String, HashSet<PreciseAudioId>>,
    id2label: HashMap<PreciseAudioId, String>,
}

pub struct PrecisionMixerController {
    sample_count: AtomicI64,
    tick_rate: f32,
    sample_rate: u32,
    channels: u16,

    // NEW: lock-free communication channels --------------------------------------
    cmd_tx: Sender<PrecisionMixerCommand>,
    // Receiver lives on the mixer side – we keep it in a Mutex<Option<..>> so it
    // can be taken exactly once when the `PrecisionMixer` is constructed.
    cmd_rx: Mutex<Option<Receiver<PrecisionMixerCommand>>>,

    // Finished sound notifications (mixer → logic)
    finished_tx: Sender<PreciseAudioId>,
    finished_rx: Receiver<PreciseAudioId>,

    // Lightweight atomic counter so game-logic can cheaply query if *anything*
    // is currently playing.
    playing_count: AtomicU64,
}

#[derive(Default)]
struct PrecisionMixerTracks {
    pending: HashMap<PreciseAudioId, PrecisionMixerQueuedTrack>,
    playing: HashMap<PreciseAudioId, PrecisionMixerActiveTrack>,
    label2id: HashMap<String, HashSet<PreciseAudioId>>,
    id2label: HashMap<PreciseAudioId, String>,
}

impl PrecisionMixerTracks {
    #[allow(dead_code)]
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

    #[allow(dead_code)]
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

    #[allow(dead_code)]
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

// NOTE: `start_at_sample_number == -1` means "start as soon as possible".
struct PrecisionMixerQueuedTrack {
    start_at_sample_number: i64,
    first_sample: MySample,
    volume: f32,
    pan: f32,
    source: BoxedSource,
}

impl std::fmt::Debug for PrecisionMixerQueuedTrack {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PrecisionMixerQueuedTrack")
            .field(
                "start_at_sample_number",
                &self.start_at_sample_number,
            )
            .field("first_sample", &self.first_sample)
            .field("volume", &self.volume)
            .field("pan", &self.pan)
            .field("source", &"<BoxedSource>")
            .finish()
    }
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

        // Create channels (command → mixer, finished ← mixer)
        let (cmd_tx, cmd_rx) = unbounded::<PrecisionMixerCommand>();
        let (finished_tx, finished_rx) = unbounded::<PreciseAudioId>();

        Arc::new(PrecisionMixerController {
            sample_count: AtomicI64::new(0),
            tick_rate,
            sample_rate,
            channels,

            cmd_tx,
            cmd_rx: Mutex::new(Some(cmd_rx)),
            finished_tx,
            finished_rx,
            playing_count: AtomicU64::new(0),
        })
    }

    // --- internal helpers ------------------------------------------------------

    fn take_cmd_receiver(&self) -> Receiver<PrecisionMixerCommand> {
        self.cmd_rx
            .lock()
            .unwrap()
            .take()
            .expect("PrecisionMixerReceiver already taken!")
    }

    fn finished_tx(&self) -> Sender<PreciseAudioId> {
        self.finished_tx.clone()
    }

    /// Helper: send command to the mixer, log if the channel is closed.
    fn send_cmd(&self, cmd: PrecisionMixerCommand) {
        if let Err(e) = self.cmd_tx.send(cmd) {
            error!(
                "PrecisionMixer command channel closed: {}",
                e
            );
        }
    }

    pub fn has_playing(&self) -> bool {
        self.playing_count.load(MemOrdering::Relaxed) > 0
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
        self.send_cmd(PrecisionMixerCommand::StopOne(id));
    }

    pub fn stop_all(&self) {
        self.send_cmd(PrecisionMixerCommand::StopAll);
    }

    pub fn stop_label(&self, label: &str) {
        self.send_cmd(PrecisionMixerCommand::StopLabel(
            label.to_owned(),
        ));
    }

    pub fn stop_many(&self, ids: &[PreciseAudioId]) {
        self.send_cmd(PrecisionMixerCommand::StopMany(
            ids.to_vec(),
        ));
    }

    pub fn stop_many_with_label(
        &self,
        _ids: &mut Vec<PreciseAudioId>,
        label: &str,
    ) {
        self.send_cmd(PrecisionMixerCommand::StopLabel(
            label.to_owned(),
        ));
    }

    pub fn cleanup_stale_ids(&self, ids: &mut Vec<PreciseAudioId>) {
        use std::collections::HashSet;
        let mut finished: HashSet<PreciseAudioId> = Default::default();
        while let Ok(id) = self.finished_rx.try_recv() {
            finished.insert(id);
        }
        ids.retain(|id| !finished.contains(id));
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
            let track = PrecisionMixerQueuedTrack {
                start_at_sample_number: start_at_sample_number.unwrap_or(-1),
                first_sample,
                volume,
                pan,
                source,
            };
            self.send_cmd(PrecisionMixerCommand::Add {
                id,
                label: label.map(|s| s.to_owned()),
                track,
            });
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
        // ---------------------------------------------------------------------
        //  Process inbound commands from the game-logic thread.
        // ---------------------------------------------------------------------
        loop {
            match self.cmd_rx.try_recv() {
                Ok(cmd) => match cmd {
                    PrecisionMixerCommand::Add { id, label, track } => {
                        if let Some(ref lbl) = label {
                            self.label2id
                                .entry(lbl.clone())
                                .or_insert_with(HashSet::default)
                                .insert(id);
                            self.id2label.insert(id, lbl.clone());
                        }
                        self.pending_heap.push((
                            Reverse(track.start_at_sample_number),
                            id,
                        ));
                        self.pending.insert(id, track);
                    },
                    PrecisionMixerCommand::StopOne(id) => {
                        self.playing.remove(&id);
                        self.pending.remove(&id);
                    },
                    PrecisionMixerCommand::StopMany(ids) => {
                        for id in ids {
                            self.playing.remove(&id);
                            self.pending.remove(&id);
                        }
                    },
                    PrecisionMixerCommand::StopLabel(label) => {
                        if let Some(set) = self.label2id.remove(&label) {
                            for id in set {
                                self.playing.remove(&id);
                                self.pending.remove(&id);
                                self.id2label.remove(&id);
                            }
                        }
                    },
                    PrecisionMixerCommand::StopAll => {
                        self.playing.clear();
                        self.pending.clear();
                        self.label2id.clear();
                        self.id2label.clear();
                    },
                },
                Err(TryRecvError::Empty) => break, // nothing left this tick
                Err(TryRecvError::Disconnected) => break, // should not happen
            }
        }

        // sync with controller's sample counter
        self.sample_count =
            self.controller.sample_count.load(MemOrdering::Relaxed);

        // ---------------------------------------------------------------------
        //  Move ready-to-start pending tracks into the active list (only on the
        //  first channel to keep stereo pairs aligned).
        // ---------------------------------------------------------------------
        if self.current_channel == 0 {
            while let Some(&(Reverse(start_at), id)) = self.pending_heap.peek()
            {
                let start_at_sample_number = if start_at < 0 {
                    self.sample_count
                } else {
                    start_at
                };

                if start_at_sample_number > self.sample_count {
                    break; // earliest pending item is still in the future
                }

                self.pending_heap.pop();
                if let Some(mut track) = self.pending.remove(&id) {
                    let missed_by = self.sample_count - start_at_sample_number;
                    if missed_by > 0 {
                        for _ in 0..(missed_by * track.source.channels() as i64)
                        {
                            if let Some(value) = track.source.next() {
                                track.first_sample = value;
                            } else {
                                // sound is already over
                                break;
                            }
                        }
                    }

                    self.playing.insert(
                        id,
                        PrecisionMixerActiveTrack {
                            done: false,
                            current_channel: 0,
                            pan: track.pan,
                            volume: track.volume,
                            next_sample: track.first_sample,
                            source: track.source,
                        },
                    );

                    // update global playing count
                    self.controller
                        .playing_count
                        .fetch_add(1, MemOrdering::Relaxed);
                }
            }
        }

        // ------------------------------------------------------------------
        //  Actual mixing – iterate over active tracks and sum their samples.
        // ------------------------------------------------------------------
        let mut sum = MySample::zero_value();
        let channels = self.channels();
        for (_id, track) in self.playing.iter_mut() {
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
        // Collect finished tracks so we can update bookkeeping atomically.
        let mut finished: Vec<PreciseAudioId> = Vec::new();
        self.playing.retain(|id, track| {
            if track.done {
                finished.push(*id);
                false
            } else {
                true
            }
        });

        for id in finished {
            let _ = self.finished_tx.send(id);
            self.controller
                .playing_count
                .fetch_sub(1, MemOrdering::Relaxed);
            if let Some(label) = self.id2label.remove(&id) {
                if let Some(set) = self.label2id.get_mut(&label) {
                    set.remove(&id);
                    if set.is_empty() {
                        self.label2id.remove(&label);
                    }
                }
            }
        }

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
        let cmd_rx = controller.take_cmd_receiver();
        let finished_tx = controller.finished_tx();

        PrecisionMixer {
            current_channel: 0,
            sample_count: 0,
            controller: controller.clone(),

            cmd_rx,
            finished_tx,

            pending_heap: BinaryHeap::with_capacity(16),
            pending: HashMap::with_capacity(16),
            playing: HashMap::with_capacity(16),
            label2id: HashMap::with_capacity(8),
            id2label: HashMap::with_capacity(8),
        }
    }

    #[allow(dead_code)]
    pub fn controller(&self) -> Arc<PrecisionMixerController> {
        self.controller.clone()
    }
}

#[allow(dead_code)]
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
