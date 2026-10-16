//! Integer musical boundaries. A sample is one frame, independently of channels.

pub const FUNDAMENTAL_HZ: u32 = 48;
pub const TICK_RATE_HZ: u32 = FUNDAMENTAL_HZ * 2;
pub const TICKS_PER_BEAT: u64 = 64;
pub const BEATS_PER_BAR: u64 = 4;

/// Round an absolute tick position once; never accumulate rounded periods.
pub fn tick_sample(tick: u64, sample_rate: u32, tick_rate: u32) -> i64 {
    let divisor = u128::from(tick_rate.max(1));
    let numerator = u128::from(tick) * u128::from(sample_rate);
    ((numerator + divisor / 2) / divisor).min(i64::MAX as u128) as i64
}

pub fn duration_sample(duration: std::time::Duration, sample_rate: u32) -> i64 {
    ((duration.as_nanos().saturating_mul(u128::from(sample_rate))
        + 500_000_000) / 1_000_000_000).min(i64::MAX as u128) as i64
}

pub fn next_boundary(tick: u64, period: u64) -> u64 {
    let period = period.max(1);
    tick.saturating_add(period - tick % period)
}

#[cfg(test)]
fn assert_day_boundaries(rate: u32) {
    for tick in (0..96 * 60 * 60 * 24u64).step_by(97) {
        let sample = tick_sample(tick, rate, TICK_RATE_HZ);
        let error = (i128::from(sample) * 96 - i128::from(tick) * i128::from(rate)).abs();
        assert!(error <= 48, "rate={rate} tick={tick}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn musical_relationships_are_exact_at_48k() {
        assert_eq!(tick_sample(1, 48_000, TICK_RATE_HZ), 500);
        assert_eq!(tick_sample(8, 48_000, TICK_RATE_HZ), 4_000);
        assert_eq!(tick_sample(TICKS_PER_BEAT, 48_000, TICK_RATE_HZ), 32_000);
        assert_eq!(tick_sample(TICKS_PER_BEAT * BEATS_PER_BAR, 48_000, TICK_RATE_HZ), 128_000);
    }

    #[test]
    fn absolute_conversion_never_accumulates_rounding_error() {
        for rate in [44_100, 48_000, 96_000] {
            assert_day_boundaries(rate);
            assert_eq!(tick_sample(96 * 60 * 60 * 24, rate, 96), i64::from(rate) * 86_400);
        }
        assert_eq!(tick_sample(u64::MAX, u32::MAX, 1), i64::MAX);
    }
}
