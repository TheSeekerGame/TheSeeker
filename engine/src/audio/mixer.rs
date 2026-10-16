//! Bounded, device-independent mixer. Tests feed the same core arbitrary buffers.
use std::sync::{atomic::{AtomicU64, Ordering}, Arc};

use crossbeam_channel::Receiver;

#[derive(Debug)]
pub(super) struct SampleData {
    pub channels: usize,
    pub frames: usize,
    pub data: Vec<f32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudioBus { Sfx, Ambience, Music }

#[derive(Debug)]
pub(super) struct Play {
    pub id: u64,
    pub sample: Arc<SampleData>,
    pub ended: Arc<AtomicU64>,
    pub bus: AudioBus,
    pub volume: f32,
    pub pan: f32,
    pub loop_range: Option<(usize, usize)>,
    pub position: usize,
    pub fade_frames: usize,
}

#[derive(Debug)]
pub(super) enum CommandKind {
    Play(Play),
    Stop(u64),
    StopAll(u64),
    BusGain { bus: AudioBus, volume: f32 },
    Gain { id: u64, volume: f32, frames: usize, stop_after: bool },
}

#[derive(Debug)]
pub(super) struct Command {
    pub at: i64,
    pub epoch: u64,
    pub order: u64,
    pub kind: CommandKind,
}

#[derive(Debug)]
pub(super) enum Control {
    Event(Command),
    Pause(i64),
    ResumeAfter(i64),
    Reset { frame: i64, epoch: u64 },
    Reference(bool),
}

/// Piecewise mapping survives a pause without stopping the hardware stream.
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct Mapping {
    pub device: i64,
    pub transport: i64,
    pub paused: bool,
    pub previous_paused: bool,
}

impl Mapping {
    pub fn position(self, device: f64) -> f64 {
        let delta = device - self.device as f64;
        let paused = if delta < 0.0 { self.previous_paused } else { self.paused };
        (self.transport as f64 + if paused { 0.0 } else { delta }).max(0.0)
    }
}

struct Voice {
    play: Play,
    gain: f32,
    target: f32,
    remaining: usize,
    stop_after: bool,
    left: f32,
    right: f32,
}

impl Voice {
    fn new(play: Play, late: usize) -> Self {
        let angle = (play.pan.clamp(-1.0, 1.0) + 1.0) * std::f32::consts::FRAC_PI_4;
        let target = play.volume;
        let remaining = play.fade_frames.saturating_sub(late);
        let gain = if play.fade_frames == 0 { target }
            else { target * (late.min(play.fade_frames) as f32 / play.fade_frames as f32) };
        let mut voice = Self {
            play, gain, target, remaining, stop_after: false,
            left: angle.cos(), right: angle.sin(),
        };
        voice.play.position = voice.play.position.saturating_add(late);
        voice
    }

}

#[derive(Debug, Clone, Copy, Default)]
pub struct MixerDiagnostics {
    pub late_events: u64,
    pub late_plays: u64,
    pub late_controls: u64,
    pub max_lateness_frames: u64,
    pub rejected_events: u64,
    pub stolen_voices: u64,
    pub clipped_samples: u64,
    pub callbacks: u64,
    pub max_callback_micros: u64,
    pub resync_requests: u64,
    pub delayed_callbacks: u64,
    pub timestamp_fallbacks: u64,
}

pub(super) struct Mixer {
    commands: Receiver<Command>,
    controls: Receiver<Control>,
    scheduled: Vec<Command>,
    voices: Vec<Option<Voice>>,
    sfx_voices: usize,
    pub device_frame: i64,
    pub transport_frame: i64,
    pub epoch: u64,
    pub mapping: Mapping,
    pub stats: MixerDiagnostics,
    pause_at: Option<i64>,
    resume_at: Option<i64>,
    reference: bool,
    reference_tick: u64,
    sample_rate: u32,
    tick_rate: u32,
    command_budget: usize,
    cancelled_through: u64,
    bus_gains: [f32; 3],
}

impl Mixer {
    pub fn new(
        commands: Receiver<Command>, controls: Receiver<Control>,
        sfx_voices: usize, background_voices: usize,
        capacity: usize, sample_rate: u32, tick_rate: u32,
    ) -> Self {
        Self {
            commands, controls, scheduled: Vec::with_capacity(capacity * 2),
            voices: (0..sfx_voices + background_voices).map(|_| None).collect(),
            sfx_voices, device_frame: 0, transport_frame: 0, epoch: 1,
            mapping: Mapping::default(), stats: MixerDiagnostics::default(),
            pause_at: None, resume_at: None, reference: false, reference_tick: 0,
            sample_rate, tick_rate, command_budget: capacity, cancelled_through: 0, bus_gains: [1.0; 3],
        }
    }

    pub fn begin_callback(&mut self) { begin_callback(self); }


    fn schedule(&mut self, command: Command, critical: bool) {
        if command.epoch != self.epoch {
            self.finish_command(command);
            return;
        }
        if self.scheduled.len() == self.scheduled.capacity() {
            self.stats.rejected_events += 1;
            self.reject(command, critical);
            return;
        }
        let index = self.scheduled.partition_point(|event| (event.at, event.order) <= (command.at, command.order));
        self.scheduled.insert(index, command);
    }

    fn reject(&mut self, command: Command, critical: bool) {
        if !critical { self.finish_command(command); return; }
        if let Some(index) = self.scheduled.iter().rposition(|event| matches!(event.kind, CommandKind::Play(_))) {
            let rejected = self.scheduled.remove(index);
            self.finish_command(rejected);
            self.schedule(command, true);
            return;
        }
        // Exhaustion of control storage enters an explicit silent resynchronization.
        self.stop_all();
        self.set_paused(true);
        self.stats.resync_requests += 1;
        self.apply(command);
    }

    fn finish_command(&self, command: Command) {
        if let CommandKind::Play(play) = command.kind {
            play.ended.store(self.stats.callbacks, Ordering::Release);
        }
    }

    fn finish_voice(&mut self, index: usize) {
        if let Some(voice) = self.voices[index].take() {
            voice.play.ended.store(self.stats.callbacks, Ordering::Release);
        }
    }

    fn stop_all(&mut self) {
        while let Some(command) = self.scheduled.pop() { self.finish_command(command); }
        for index in 0..self.voices.len() { self.finish_voice(index); }
    }

    fn apply(&mut self, command: Command) {
        let late = self.transport_frame.saturating_sub(command.at).max(0) as usize;
        if late > 0 {
            self.stats.late_events += 1;
            self.stats.max_lateness_frames = self.stats.max_lateness_frames.max(late as u64);
            let play = matches!(command.kind, CommandKind::Play(_));
            self.stats.late_plays += u64::from(play);
            self.stats.late_controls += u64::from(!play);
        }
        match command.kind {
            CommandKind::Play(play) => self.start_voice(play, late),
            CommandKind::Stop(id) => stop_id(self, id),
            CommandKind::StopAll(id) => cancel_through(self, id),
            CommandKind::BusGain { bus, volume } => self.bus_gains[bus as usize] = volume,
            CommandKind::Gain { id, volume, frames, stop_after } => {
                change_gain(self, id, volume, frames, stop_after);
            },
        }
    }

    fn start_voice(&mut self, play: Play, late: usize) {
        if play.id <= self.cancelled_through || !play_has_tail(&play, late) {
            play.ended.store(self.stats.callbacks, Ordering::Release);
            return;
        }
        let range = if play.bus == AudioBus::Sfx { 0..self.sfx_voices }
            else { self.sfx_voices..self.voices.len() };
        let free = range.clone().find(|&index| self.voices[index].is_none());
        let stolen = (play.bus == AudioBus::Sfx).then(|| {
            range.min_by_key(|&index| self.voices[index].as_ref().map_or(0, |voice| voice.play.id))
        }).flatten();
        let Some(index) = free.or(stolen) else {
            self.stats.rejected_events += 1;
            play.ended.store(self.stats.callbacks, Ordering::Release);
            return;
        };
        if self.voices[index].is_some() {
            self.stats.stolen_voices += 1;
            self.finish_voice(index);
        }
        self.voices[index] = Some(Voice::new(play, late));
    }


    fn set_paused(&mut self, paused: bool) {
        self.mapping = Mapping {
            device: self.device_frame, transport: self.transport_frame,
            previous_paused: self.mapping.paused, paused,
        };
    }

    /// Stereo interleaved scratch buffer, of any bounded size supplied by caller.
    pub fn render(&mut self, output: &mut [f32]) { render_mixer(self, output); }

    fn apply_due(&mut self) {
        if self.pause_at.is_some_and(|at| at <= self.transport_frame) {
            self.set_paused(true);
            self.pause_at = None;
        }
        if self.resume_at.is_some_and(|at| at <= self.device_frame) {
            self.set_paused(false);
            self.resume_at = None;
        }
        while self.scheduled.first().is_some_and(|event| event.at <= self.transport_frame) {
            let command = self.scheduled.remove(0);
            self.apply(command);
        }
    }

    fn span_frames(&self, remaining: usize) -> usize {
        let boundary = if self.mapping.paused {
            self.resume_at.map(|at| at - self.device_frame)
        } else {
            self.scheduled.first().map(|event| event.at).into_iter()
                .chain(self.pause_at).min().map(|at| at - self.transport_frame)
        };
        boundary.map_or(remaining, |frames| remaining.min(frames.max(1) as usize))
    }

}

fn play_has_tail(play: &Play, late: usize) -> bool {
    if let Some((start, end)) = play.loop_range { return start < end && end <= play.sample.frames; }
    play.position.saturating_add(late) < play.sample.frames
}

fn render_voice(voice: &mut Voice, out: &mut [f32], bus_gains: &[f32; 3]) -> bool {
    for frame in out.as_chunks_mut::<2>().0 {
        if !resolve_voice_position(voice) { return false; }
        let position = voice.play.position * voice.play.sample.channels;
        let left = voice.play.sample.data[position];
        let right = voice.play.sample.data[position + usize::from(voice.play.sample.channels > 1)];
        frame[0] += left * voice.left * voice.gain * bus_gains[voice.play.bus as usize];
        frame[1] += right * voice.right * voice.gain * bus_gains[voice.play.bus as usize];
        voice.play.position += 1;
        if voice.remaining > 0 {
            voice.gain += (voice.target - voice.gain) / voice.remaining as f32;
            voice.remaining -= 1;
        }
        if voice.remaining == 0 && voice.stop_after { return false; }
    }
    true
}

fn resolve_voice_position(voice: &mut Voice) -> bool {
    if let Some((start, end)) = voice.play.loop_range {
        if voice.play.position >= end {
            voice.play.position = start + (voice.play.position - start) % (end - start);
        }
    }
    voice.play.position < voice.play.sample.frames
}

fn handle_control(mixer: &mut Mixer, control: Control) {
    match control {
        Control::Event(command) => mixer.schedule(command, true),
        Control::Pause(at) => { mixer.pause_at = Some(at); mixer.resume_at = None; },
        Control::ResumeAfter(frames) => { mixer.resume_at = Some(mixer.device_frame.saturating_add(frames.max(0))); mixer.pause_at = None; },
        Control::Reset { frame, epoch } => {
            mixer.stop_all();
            mixer.epoch = epoch;
            mixer.transport_frame = frame;
            mixer.mapping = Mapping { device: mixer.device_frame, transport: frame, paused: true, previous_paused: true };
            mixer.pause_at = None;
            mixer.resume_at = None;
            mixer.reference_tick = (frame.max(0) as u128 * u128::from(mixer.tick_rate)
                / u128::from(mixer.sample_rate)) as u64;
        },
        Control::Reference(enabled) => {
            mixer.reference = enabled;
            mixer.reference_tick = (mixer.transport_frame.max(0) as u128 * u128::from(mixer.tick_rate)
                / u128::from(mixer.sample_rate)) as u64;
        },
    }
}

fn stop_id(mixer: &mut Mixer, id: u64) {
    for index in 0..mixer.voices.len() {
        if mixer.voices[index].as_ref().is_some_and(|voice| voice.play.id == id) {
            mixer.finish_voice(index);
        }
    }
    let mut index = 0;
    while index < mixer.scheduled.len() {
        let matches = matches!(&mixer.scheduled[index].kind, CommandKind::Play(play) if play.id == id);
        if matches { let command = mixer.scheduled.remove(index); mixer.finish_command(command); }
        else { index += 1; }
    }
}

fn change_gain(mixer: &mut Mixer, id: u64, volume: f32, frames: usize, stop_after: bool) {
    for voice in mixer.voices.iter_mut().flatten().filter(|voice| voice.play.id == id) {
        voice.target = volume;
        voice.remaining = frames;
        voice.stop_after = stop_after;
        if frames == 0 { voice.gain = volume; }
    }
    if frames == 0 && stop_after { stop_id(mixer, id); }
}

pub fn render_mixer(mixer: &mut Mixer, output: &mut [f32]) {
    output.fill(0.0);
    let frames = output.len() / 2;
    let mut offset = 0;
    while offset < frames {
        mixer.apply_due();
        let count = mixer.span_frames(frames - offset);
        if !mixer.mapping.paused {
            mix_span(mixer, &mut output[offset * 2..(offset + count) * 2]);
            mixer.transport_frame += count as i64;
        }
        mixer.device_frame += count as i64;
        offset += count;
    }
}

fn mix_span(mixer: &mut Mixer, output: &mut [f32]) {
    for index in 0..mixer.voices.len() {
        let finished = mixer.voices[index].as_mut().is_some_and(|voice| !render_voice(voice, output, &mixer.bus_gains));
        if finished { mixer.finish_voice(index); }
    }
    if mixer.reference { mix_reference(mixer, output); }
}

fn mix_reference(mixer: &mut Mixer, output: &mut [f32]) {
    let end = mixer.transport_frame + (output.len() / 2) as i64;
    loop {
        let at = super::transport::tick_sample(mixer.reference_tick, mixer.sample_rate, mixer.tick_rate);
        if at >= end { break; }
        if at >= mixer.transport_frame {
            let index = (at - mixer.transport_frame) as usize * 2;
            output[index] += 0.1;
            output[index + 1] += 0.1;
        }
        mixer.reference_tick += 1;
    }
}

#[cfg(test)]
mod tests;

fn begin_callback(mixer: &mut Mixer) {
    mixer.stats.callbacks += 1;
    for _ in 0..mixer.command_budget {
        let Ok(control) = mixer.controls.try_recv() else { break; };
        handle_control(mixer, control);
    }
    for _ in 0..mixer.command_budget {
        let Ok(command) = mixer.commands.try_recv() else { break; };
        mixer.schedule(command, false);
    }
}

fn cancel_through(mixer: &mut Mixer, id: u64) {
    mixer.cancelled_through = mixer.cancelled_through.max(id);
    for index in 0..mixer.voices.len() {
        if mixer.voices[index].as_ref().is_some_and(|voice| voice.play.id <= id) { mixer.finish_voice(index); }
    }
    let mut index = 0;
    while index < mixer.scheduled.len() {
        if matches!(&mixer.scheduled[index].kind, CommandKind::Play(play) if play.id <= id) {
            let command = mixer.scheduled.remove(index);
            mixer.finish_command(command);
        } else { index += 1; }
    }
}
