//! Locomotion selects a movement model explicitly; frame entries carry local displacement.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use theseeker_engine::animation::{PlaybackState, PlaybackEventKind, PlaybackId};
use theseeker_engine::assets::animation::FrameId;
use super::action::TickDisplacement;

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum WalkMotion {
    #[default]
    Continuous,
    FrameSteps {
        /// Clip supplying movement data; its sprites need not be used by this actor.
        track: String,
        #[serde(default = "unit_scale")]
        scale: f32,
        /// Replaces the referenced track when supplied, including an empty track.
        steps: Option<Vec<FrameStep>>,
    },
}

fn unit_scale() -> f32 { 1.0 }

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FrameStep {
    pub frame: FrameId,
    pub delta: [f32; 2],
}

#[derive(Clone, Default)]
pub enum ResolvedWalkMotion {
    #[default]
    Continuous,
    FrameSteps(Vec<FrameStep>),
}

impl ResolvedWalkMotion {
    pub fn collect_steps(&self, signals: Option<&PlaybackState>, playback: PlaybackId, forward: f32, output: &mut Vec<TickDisplacement>) {
        output.clear();
        let (Self::FrameSteps(steps), Some(signals)) = (self, signals) else { return; };
        for event in signals.ordered.iter().filter(|event| event.playback == playback) {
            append_entry_steps(&event.kind, steps, forward, output);
        }
    }
}

fn append_entry_steps(event: &PlaybackEventKind, steps: &[FrameStep], forward: f32, output: &mut Vec<TickDisplacement>) {
    let PlaybackEventKind::FrameEntered(frame) = event else { return; };
    output.extend(steps.iter().filter(|step| step.frame == *frame)
        .map(|step| TickDisplacement(Vec2::new(step.delta[0] * forward, step.delta[1]))));
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum MotionBlockedBehavior { #[default] None, Stop, TurnAround }
