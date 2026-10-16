//! Tick-authored strike timing is independent of procedural pose generation.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use super::action::ActiveAction;

#[derive(Debug, Clone, Copy, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TickStrike {
    pub windup: u32,
    pub strike: u32,
    pub recover: u32,
    pub hit_window: [u32; 2],
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum StrikePhase { Windup, Strike(f32), Recover(f32) }

impl TickStrike {
    pub fn duration(self) -> u32 { self.windup + self.strike + self.recover }

    pub fn phase(self, age: u32) -> StrikePhase {
        if age < self.windup { return StrikePhase::Windup; }
        if age < self.windup + self.strike {
            return StrikePhase::Strike((age - self.windup) as f32 / self.strike as f32);
        }
        StrikePhase::Recover((age - self.windup - self.strike) as f32 / self.recover as f32)
    }

    pub fn hit_active(self, age: u32) -> bool {
        age >= self.hit_window[0] && age <= self.hit_window[1]
    }

    pub fn validate(self) -> Result<(), String> {
        let duration = self.windup.checked_add(self.strike).and_then(|value| value.checked_add(self.recover));
        if self.strike == 0 || self.recover == 0 || duration.is_none_or(|duration| self.hit_window[1] >= duration)
            || self.hit_window[0] > self.hit_window[1]
        {
            return Err("strike requires positive phases and a bounded ordered hit window".to_owned());
        }
        Ok(())
    }
}

#[derive(Component)]
#[component(on_discard = super::action::cleanup_action_slot::<ProceduralCombat>)]
pub struct ProceduralCombat {
    pub action: Option<ActiveAction>,
    pub cooldown: u32,
}

impl Default for ProceduralCombat {
    fn default() -> Self { Self { action: None, cooldown: 18 } }
}

impl super::action::ActionSlot for ProceduralCombat {
    fn take_action(&mut self) -> Option<ActiveAction> { self.action.take() }
}
