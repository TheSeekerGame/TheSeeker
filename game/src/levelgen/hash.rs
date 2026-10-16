//! Deterministic hashes behind every generation roll: a seed reproduces its level only while these
//! stay unchanged.

const GOLDEN_GAMMA: u64 = 0x9E37_79B9_7F4A_7C15;

/// SplitMix64's output mix.
pub(super) fn mix64(mut z: u64) -> u64 {
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

pub(super) fn splitmix64(x: u64) -> u64 {
    mix64(x.wrapping_add(GOLDEN_GAMMA))
}

/// Whether a hash lands within a `prob<N>` percentage.
pub(super) fn within_percent(hash: u64, percent: u8) -> bool {
    hash % 100 < u64::from(percent)
}

/// A tile's hash for one rule (`order`) of one rule set (`salt`).
pub(super) fn tile_hash(x: i32, y: i32, order: u32, salt: u64) -> u64 {
    mix64(((x as i64 as u64) << 32) ^ ((y as i64 as u64) << 16) ^ u64::from(order) ^ salt ^ GOLDEN_GAMMA)
}
