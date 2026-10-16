//! Deterministic critical-strike tracking.
//!
//! Uses a simple polyrhythm (every 17th and 19th hit) to schedule crits and
//! exposes a flag for the next hit. The multiplier is stored on the component.
use bevy::prelude::*;

/// Allows the entity to apply critical strikes.
/// Crits are deterministic and trigger every 17th and 19th successful hits.
#[derive(Component, Default, Debug, Reflect)]
pub struct Crits {
    pub next_hit_is_critical: bool,
    /// Counts number of successful hits
    pub hit_count: u32,
    /// Critical damage multiplier
    pub crit_damage_multiplier: f32,
}

impl Crits {
    pub fn new(multiplier: f32) -> Self {
        Self {
            next_hit_is_critical: false,
            hit_count: 0,
            crit_damage_multiplier: multiplier,
        }
    }

    pub fn schedule_next_crit(&mut self) {
        self.next_hit_is_critical = true;
    }
}

/// Update the crit flag based on the polyrhythm pattern.
pub fn track_crits(mut query: Query<&mut Crits>) {
    for mut crits in query.iter_mut() {
        if crits.hit_count != 0
            && (crits.hit_count % 17 == 0 || crits.hit_count % 19 == 0)
        {
            crits.next_hit_is_critical = true;
        }
    }
}
