use super::custom::*;
use super::mixer::Mixer;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{BufferSize, SampleFormat, SampleRate, StreamConfig, SupportedBufferSize};

struct SelectedConfig {
    config: StreamConfig,
    sample_format: SampleFormat,
    sample_rate: u32,
    block_frames: u32,
}

fn select_output_config(
    device: &cpal::Device,
    desired_sample_rate: u32,
    desired_block_frames: u32,
) -> Result<SelectedConfig, String> {
    let mut best: Option<(i32, SelectedConfig)> = None;
    let configs = device
        .supported_output_configs()
        .map_err(|e| e.to_string())?;

    for range in configs {
        let format_score = match range.sample_format() {
            SampleFormat::F32 => 3,
            SampleFormat::I16 => 2,
            SampleFormat::U16 => 1,
            _ => continue,
        };
        let channels = range.channels();
        let channel_score = if channels == 2 {
            2
        } else { i32::from(channels > 2) };
        let min_rate = range.min_sample_rate().0;
        let max_rate = range.max_sample_rate().0;
        let chosen_rate = desired_sample_rate.clamp(min_rate, max_rate);
        let rate_score = if chosen_rate == desired_sample_rate {
            2
        } else {
            0
        };

        let supported = range
            .try_with_sample_rate(SampleRate(chosen_rate))
            .ok_or_else(|| "Unsupported sample rate".to_string())?;
        let buffer_size =
            selected_buffer_size(range.buffer_size(), desired_block_frames);

        let mut config = supported.config();
        config.buffer_size = buffer_size;
        let block_frames = match config.buffer_size {
            BufferSize::Fixed(frames) => frames,
            BufferSize::Default => desired_block_frames,
        };

        let score = rate_score * 100 + channel_score * 10 + format_score;
        let selected = SelectedConfig {
            config,
            sample_format: range.sample_format(),
            sample_rate: chosen_rate,
            block_frames,
        };

        match best {
            Some((best_score, _)) if best_score >= score => {},
            _ => {
                best = Some((score, selected));
            },
        }
    }

    if let Some((_score, selected)) = best {
        Ok(selected)
    } else {
        Err("No supported output configs".to_string())
    }
}

fn selected_buffer_size(
    supported: &SupportedBufferSize,
    desired_block_frames: u32,
) -> BufferSize {
    let SupportedBufferSize::Range { min, max } = supported else {
        return BufferSize::Default;
    };

    if desired_block_frames >= *min && desired_block_frames <= *max {
        return BufferSize::Fixed(desired_block_frames);
    }

    BufferSize::Default
}


pub(super) fn start(
    config: &AudioEngineConfig,
    commands: crossbeam_channel::Receiver<super::mixer::Command>,
    controls: crossbeam_channel::Receiver<super::mixer::Control>,
    snapshots: crossbeam_channel::Sender<ClockSnapshot>,
    obsolete_snapshots: crossbeam_channel::Receiver<ClockSnapshot>,
    running: std::sync::Arc<std::sync::atomic::AtomicBool>,
) -> Result<(cpal::Stream, u32, u32), String> {
    let device = cpal::default_host().default_output_device().ok_or("No output device")?;
    let mut selected = select_output_config(&device, config.desired_sample_rate, config.desired_block_frames)?;
    // A backend can advertise a range but reject a particular buffer size.
    let make_state = || Callback {
        mixer: Mixer::new(commands.clone(), controls.clone(), config.max_sfx_voices.max(1),
            config.max_background_voices.max(2), config.command_capacity.max(1), selected.sample_rate, config.tick_rate_hz),
        snapshots: snapshots.clone(), obsolete_snapshots: obsolete_snapshots.clone(), previous_arrival: None, channels: selected.config.channels as usize,
        rate: selected.sample_rate, timestamps: TimestampAnchor::default(),
    };
    let first = build_stream(&device, &selected, make_state(), running.clone());
    let stream = if let Ok(stream) = first { stream } else {
        let state = make_state();
        selected.config.buffer_size = BufferSize::Default;
        build_stream(&device, &selected, state, running.clone())?
    };
    stream.play().map_err(|error| error.to_string())?;
    running.store(true, std::sync::atomic::Ordering::Release);
    bevy::log::info!("Custom audio: {} Hz, {} channels, {:?}", selected.sample_rate, selected.config.channels, selected.config.buffer_size);
    Ok((stream, selected.sample_rate, selected.block_frames))
}

fn build_stream(
    device: &cpal::Device, selected: &SelectedConfig, mut state: Callback,
    running: std::sync::Arc<std::sync::atomic::AtomicBool>,
) -> Result<cpal::Stream, String> {
    let on_error = move |_error| { running.store(false, std::sync::atomic::Ordering::Release); };
    match selected.sample_format {
        SampleFormat::F32 => device.build_output_stream(&selected.config, move |data: &mut [f32], info| state.process(data, info), on_error, None),
        SampleFormat::I16 => device.build_output_stream(&selected.config, move |data: &mut [i16], info| state.process(data, info), on_error, None),
        SampleFormat::U16 => device.build_output_stream(&selected.config, move |data: &mut [u16], info| state.process(data, info), on_error, None),
        _ => return Err("Unsupported sample format".to_owned()),
    }.map_err(|error| error.to_string())
}

#[derive(Default)]
struct TimestampAnchor {
    first: Option<(cpal::StreamInstant, std::time::Instant)>,
    previous: Option<cpal::StreamInstant>,
    fallbacks: u64,
}

impl TimestampAnchor {
    fn playback(&mut self, info: &cpal::OutputCallbackInfo, now: std::time::Instant, block: std::time::Duration) -> std::time::Instant {
        let timestamp = info.timestamp();
        let chronological = self.previous.is_none_or(|previous| timestamp.callback.duration_since(&previous).is_some_and(|duration| !duration.is_zero()));
        self.previous = Some(timestamp.callback);
        let latency = timestamp.playback.duration_since(&timestamp.callback).filter(|latency| *latency <= std::time::Duration::from_millis(500));
        let Some(latency) = latency.filter(|_| chronological) else {
            self.first = None;
            self.fallbacks += 1;
            return now + block;
        };
        let (first, host) = *self.first.get_or_insert((timestamp.callback, now));
        let candidate = timestamp.callback.duration_since(&first).and_then(|duration| host.checked_add(duration));
        let Some(playback) = validated_playback(candidate, latency, now) else {
            self.first = Some((timestamp.callback, now));
            self.fallbacks += 1;
            return now + block;
        };
        playback
    }
}

struct Callback {
    mixer: Mixer,
    snapshots: crossbeam_channel::Sender<ClockSnapshot>,
    obsolete_snapshots: crossbeam_channel::Receiver<ClockSnapshot>,
    previous_arrival: Option<std::time::Instant>,
    channels: usize,
    rate: u32,
    timestamps: TimestampAnchor,
}

impl Callback {
    fn process<T: cpal::Sample + cpal::FromSample<f32>>(&mut self, output: &mut [T], info: &cpal::OutputCallbackInfo) {
        let now = std::time::Instant::now();
        let frames = output.len() / self.channels;
        if frames == 0 { return; }
        let duration = std::time::Duration::from_secs_f64(frames as f64 / f64::from(self.rate));
        let playback = self.timestamps.playback(info, now, duration);
        self.mixer.begin_callback();
        if self.previous_arrival.replace(now).is_some_and(|previous| now.saturating_duration_since(previous) > duration * 2) {
            self.mixer.stats.delayed_callbacks += 1;
        }
        self.mixer.stats.timestamp_fallbacks = self.timestamps.fallbacks;
        let mut scratch = [0.0f32; 512];
        for chunk in output.chunks_mut(256 * self.channels) {
            let count = chunk.len() / self.channels;
            self.mixer.render(&mut scratch[..count * 2]);
            self.write_output(chunk, &scratch[..count * 2]);
        }
        self.mixer.stats.max_callback_micros = self.mixer.stats.max_callback_micros.max(now.elapsed().as_micros() as u64);
        let snapshot = ClockSnapshot {
            device_end: self.mixer.device_frame, playback_end: playback + duration,
            observed: now, mapping: self.mixer.mapping, epoch: self.mixer.epoch,
            block_frames: frames as u32, stats: self.mixer.stats,
            latency: playback.saturating_duration_since(now),
        };
        // Dropping an obsolete Copy snapshot cannot allocate or destroy audio resources.
        if let Err(error) = self.snapshots.try_send(snapshot) {
            let _ = self.obsolete_snapshots.try_recv();
            let _ = self.snapshots.try_send(error.into_inner());
        }
    }

    fn write_output<T: cpal::Sample + cpal::FromSample<f32>>(&mut self, output: &mut [T], stereo: &[f32]) {
        for (frame, pair) in output.chunks_mut(self.channels).zip(stereo.as_chunks::<2>().0.iter()) {
            self.write_frame(frame, pair);
        }
    }

    fn write_frame<T: cpal::Sample + cpal::FromSample<f32>>(&mut self, frame: &mut [T], pair: &[f32]) {
            frame.fill(T::from_sample(0.0));
            let left = if self.channels == 1 { (pair[0] + pair[1]) * 0.5 } else { pair[0] };
            frame[0] = self.output_sample(left);
            if self.channels > 1 { frame[1] = self.output_sample(pair[1]); }
    }

    fn output_sample<T: cpal::Sample + cpal::FromSample<f32>>(&mut self, value: f32) -> T {
        if value.abs() > 1.0 { self.mixer.stats.clipped_samples += 1; }
        T::from_sample(value.clamp(-1.0, 1.0))
    }
}

fn validated_playback(candidate: Option<std::time::Instant>, latency: std::time::Duration, now: std::time::Instant) -> Option<std::time::Instant> {
    let candidate = candidate?;
    let distance = candidate.saturating_duration_since(now).max(now.saturating_duration_since(candidate));
    if distance > std::time::Duration::from_millis(100) || latency > std::time::Duration::from_millis(500) { return None; }
    candidate.checked_add(latency)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    #[test]
    fn timestamp_validation_rejects_missing_implausible_and_discontinuous_estimates() {
        let now = Instant::now();
        assert_eq!(validated_playback(None, Duration::ZERO, now), None);
        assert_eq!(validated_playback(Some(now), Duration::from_secs(1), now), None);
        assert_eq!(validated_playback(Some(now + Duration::from_secs(1)), Duration::ZERO, now), None);
        assert_eq!(validated_playback(Some(now - Duration::from_secs(1)), Duration::ZERO, now), None);
        assert_eq!(validated_playback(Some(now - Duration::from_millis(1)), Duration::from_millis(5), now), Some(now + Duration::from_millis(4)));
    }
}
