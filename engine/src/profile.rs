//! Tick profiling, switched on by the `SEEKER_TICK_PROFILE` env var: after each second of ticks,
//! profiled systems log their average and worst time.
use std::sync::OnceLock;

use bevy::ecs::system::SystemParam;

use crate::prelude::*;

/// Windows under this average and worst time are not logged.
const MIN_PROFILE_LOG_MS: f64 = 0.05;

pub fn profiling_enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| std::env::var("SEEKER_TICK_PROFILE").is_ok_and(|value| value != "0"))
}

/// One system's times over the current window.
#[derive(Default)]
pub struct TickSystemProfile {
    ticks: u32,
    total: Duration,
    max: Duration,
}

impl TickSystemProfile {
    /// Adds a tick's time. Once a second of ticks has passed, returns the window's average and worst
    /// milliseconds and its tick count, unless both times are negligible.
    pub fn sample(&mut self, elapsed: Duration, hz: f64) -> Option<(f64, f64, u32)> {
        self.ticks = self.ticks.saturating_add(1);
        self.total += elapsed;
        self.max = self.max.max(elapsed);
        if self.ticks < hz.round().max(1.0) as u32 {
            return None;
        }
        let window = std::mem::take(self);
        let avg_ms = window.total.as_secs_f64() * 1000.0 / window.ticks.max(1) as f64;
        let max_ms = window.max.as_secs_f64() * 1000.0;
        (avg_ms >= MIN_PROFILE_LOG_MS || max_ms >= MIN_PROFILE_LOG_MS).then_some((avg_ms, max_ms, window.ticks))
    }
}

/// Times a tick system: `let _profile = profiler.scope("label");` covers the rest of the system.
#[derive(SystemParam)]
pub struct TickProfiler<'w, 's> {
    time: Res<'w, GameTime>,
    window: Local<'s, TickSystemProfile>,
}

impl TickProfiler<'_, '_> {
    pub fn scope(&mut self, label: &'static str) -> ProfileScope<'_> {
        ProfileScope { start: profiling_enabled().then(Instant::now), hz: self.time.hz, label, window: &mut self.window }
    }
}

/// Records its system's time when dropped.
#[must_use = "the profile covers the time until the scope drops"]
pub struct ProfileScope<'a> {
    window: &'a mut TickSystemProfile,
    start: Option<Instant>,
    hz: f64,
    label: &'static str,
}

impl Drop for ProfileScope<'_> {
    fn drop(&mut self) {
        let Some((avg_ms, max_ms, ticks)) = self.start.and_then(|start| self.window.sample(start.elapsed(), self.hz)) else {
            return;
        };
        info!(target: "theseeker_engine::tick_system_profile", "{}: avg_ms={avg_ms:.2} max_ms={max_ms:.2} ticks={ticks}", self.label);
    }
}
