//! The low-health vignette pulse.
use std::f32::consts::TAU;

use super::post_processing::vignette::VignetteSettings;
use crate::game::combat::Health;
use crate::game::player::Player;
use crate::prelude::*;

/// The share of health at and below which the vignette pulses.
const LOW_HEALTH_THRESHOLD: f32 = 0.25;
const LOW_HEALTH_PULSE_BPM: f32 = 103.0;
/// The pulse's intensity at its weakest beat, and at the threshold.
const LOW_HEALTH_MIN_PULSE_INTENSITY: f32 = 0.45;
const LOW_HEALTH_MIN_DANGER_INTENSITY: f32 = 0.60;
/// How the pulse's intensity reddens, darkens, and fogs the vignette.
const LOW_HEALTH_COLOR_BASE: f32 = 0.42;
const LOW_HEALTH_COLOR_PULSE_RANGE: f32 = 0.36;
const LOW_HEALTH_BRIGHTNESS_DROP: f32 = 0.14;
const LOW_HEALTH_FOG_STRENGTH_BOOST: f32 = 0.12;

pub struct LowHealthVignettePlugin;

impl Plugin for LowHealthVignettePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, update_hp_vignette);
    }
}

/// Pulses the vignette red to the beat while the player's health is low, stronger the lower
/// it is, and calms it otherwise.
fn update_hp_vignette(
    player: Query<&Health, With<Player>>,
    time: Res<PresentationTime>,
    mut vignettes: Query<&mut VignetteSettings>,
) {
    let calm = VignetteSettings::default();
    let low_health = player
        .single()
        .ok()
        .filter(|health| health.max > 0)
        .map(|health| health.current as f32 / health.max as f32)
        .filter(|share| *share <= LOW_HEALTH_THRESHOLD);
    let target = low_health.map_or(calm, |share| {
        let beat = (time.elapsed_secs() * (LOW_HEALTH_PULSE_BPM / 60.0) * TAU).cos().abs();
        let danger = ((LOW_HEALTH_THRESHOLD - share) / LOW_HEALTH_THRESHOLD).clamp(0.0, 1.0);
        let intensity =
            LOW_HEALTH_MIN_PULSE_INTENSITY.lerp(1.0, beat) * LOW_HEALTH_MIN_DANGER_INTENSITY.lerp(1.0, danger);
        let tint = (LOW_HEALTH_COLOR_BASE - intensity * LOW_HEALTH_COLOR_PULSE_RANGE).clamp(0.0, 1.0);
        VignetteSettings {
            color: Vec3::new(1.0, tint, tint),
            base_brightness: (calm.base_brightness - intensity * LOW_HEALTH_BRIGHTNESS_DROP).max(0.0),
            fog_strength: calm.fog_strength + intensity * LOW_HEALTH_FOG_STRENGTH_BOOST,
            ..calm
        }
    });
    for mut settings in &mut vignettes {
        *settings = target;
    }
}
