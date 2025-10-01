//! Input buffering at 96 Hz for early input compensation.
//!
//! Responsibilities:
//! - Record just-pressed actions with their tick and directional variant
//! - Provide short, per-action windows to consume those inputs later in the tick flow
//! - Offer helpers to treat buffered inputs as if pressed this tick
use bevy::prelude::*;
use leafwing_input_manager::action_state::ActionState;
use std::collections::VecDeque;

use crate::game::player::{Player, PlayerAction, PlayerStateSet};
use theseeker_engine::time::GameTime;

/// Input buffering for early input compensation at 96 Hz.
pub struct InputBufferPlugin;

impl Plugin for InputBufferPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            crate::GameTickUpdate,
            (
                update_input_buffers,
                clear_expired_buffers,
            )
                .chain()
                .in_set(PlayerStateSet::Input)
                .before(PlayerStateSet::Behavior),
        );
    }
}

/// Tracks buffered inputs with their originating tick and up/down variant.
#[derive(Component, Debug)]
pub struct InputBuffer {
    /// Most-recent-first semantics via back insertion; keep short to avoid stale inputs.
    entries: VecDeque<BufferedInput>,
    /// Maximum number of entries to keep
    max_entries: usize,
}

impl Default for InputBuffer {
    fn default() -> Self {
        Self {
            entries: VecDeque::with_capacity(10),
            max_entries: 10,
        }
    }
}

#[derive(Debug, Clone)]
pub struct BufferedInput {
    pub action: PlayerAction,
    pub tick: u64,
    pub variant_modifier: InputVariant,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum InputVariant {
    Normal,
    Up,   // Jump/Up held at press time
    Down, // Fall/Down held at press time
}

/// Buffer windows per action (ticks @ 96 Hz)
pub struct BufferConfig;

impl BufferConfig {
    /// Buffer time at 96Hz
    pub const fn buffer_time(action: PlayerAction) -> u32 {
        match action {
            // Generous to aid pre-landing and wall-jumps
            PlayerAction::Jump => 12, // ~125ms

            // Moderate windows for ability cooldown timing
            PlayerAction::Skill1 => 8, // ~83ms
            PlayerAction::Skill2 => 8,
            PlayerAction::Skill3 => 8,
            PlayerAction::Skill4 => 8,
            PlayerAction::Skill5 => 8,

            // Minimal or none for the rest
            PlayerAction::Fall => 4, // Small buffer for fast-fall timing
            PlayerAction::Interact => 4,
            PlayerAction::UpModifier => 0,

            // No buffering
            PlayerAction::Move => 0,
            PlayerAction::SwapCombatStyle => 0,
            PlayerAction::SwapMeleeWeapon => 0,
            PlayerAction::ToggleControlOverlay => 0,
            PlayerAction::TogglePassiveInventory => 0,
        }
    }

    /// Whether an action uses buffering
    pub const fn should_buffer(action: PlayerAction) -> bool {
        Self::buffer_time(action) > 0
    }
}

impl InputBuffer {
    /// Record a new input if the action is bufferable and not already recorded for this tick.
    pub fn push(
        &mut self,
        action: PlayerAction,
        tick: u64,
        variant: InputVariant,
    ) {
        if !BufferConfig::should_buffer(action) {
            return;
        }

        if self
            .entries
            .iter()
            .any(|e| e.action == action && e.tick == tick)
        {
            return;
        }

        self.entries.push_back(BufferedInput {
            action,
            tick,
            variant_modifier: variant,
        });

        // Trim to capacity
        while self.entries.len() > self.max_entries {
            self.entries.pop_front();
        }
    }

    /// Return the most recent buffered input still within the window.
    pub fn check_buffered(
        &self,
        action: PlayerAction,
        current_tick: u64,
    ) -> Option<&BufferedInput> {
        let buffer_time = BufferConfig::buffer_time(action) as u64;

        self.entries
            .iter()
            .rev() // Check most recent first
            .find(|input| {
                input.action == action
                    && current_tick.saturating_sub(input.tick) <= buffer_time
            })
    }

    /// Remove and return the most recent valid buffered input.
    pub fn consume(&mut self, action: PlayerAction) -> Option<BufferedInput> {
        let current_tick = self.entries.back().map(|e| e.tick).unwrap_or(0);
        let buffer_time = BufferConfig::buffer_time(action) as u64;

        let index = self.entries.iter().rposition(|input| {
            input.action == action
                && current_tick.saturating_sub(input.tick) <= buffer_time
        });

        index.and_then(|i| self.entries.remove(i))
    }

    /// Drop inputs older than a small global horizon.
    pub fn clear_expired(&mut self, current_tick: u64) {
        const MAX_AGE: u64 = 20; // ~208ms

        self.entries
            .retain(|input| current_tick.saturating_sub(input.tick) <= MAX_AGE);
    }

    /// Clear all buffered inputs
    pub fn clear(&mut self) {
        self.entries.clear();
    }

    /// Clear specific action (use after a successful trigger)
    pub fn clear_action(&mut self, action: PlayerAction) {
        self.entries.retain(|input| input.action != action);
    }
}

/// Record newly pressed actions into the buffer with their variant.
fn update_input_buffers(
    mut query: Query<
        (
            &mut InputBuffer,
            &ActionState<PlayerAction>,
        ),
        With<Player>,
    >,
    time: Res<GameTime>,
) {
    for (mut buffer, action_state) in query.iter_mut() {
        let current_tick = time.tick() as u64;

        for action in [
            PlayerAction::Jump,
            PlayerAction::Skill1,
            PlayerAction::Skill2,
            PlayerAction::Skill3,
            PlayerAction::Skill4,
            PlayerAction::Skill5,
            PlayerAction::Fall,
            PlayerAction::Interact,
        ] {
            if BufferConfig::should_buffer(action)
                && action_state.just_pressed(&action)
            {
                // Derive variant from directional modifiers (Down wins ties)
                let variant = if action_state.pressed(&PlayerAction::Fall) {
                    InputVariant::Down
                } else if action_state.pressed(&PlayerAction::UpModifier)
                    || action_state.pressed(&PlayerAction::Jump)
                {
                    InputVariant::Up
                } else {
                    InputVariant::Normal
                };

                buffer.push(action, current_tick, variant);
            }
        }
    }
}

    /// Remove stale inputs beyond a small global horizon.
fn clear_expired_buffers(
    mut query: Query<&mut InputBuffer, With<Player>>,
    time: Res<GameTime>,
) {
    let current_tick = time.tick();

    for mut buffer in query.iter_mut() {
        buffer.clear_expired(current_tick);
    }
}

/// Helper for checking buffered inputs alongside fresh presses.
pub trait InputBufferExt {
    /// True if an action is buffered or was just pressed this tick.
    fn pressed_or_buffered(&self, action: PlayerAction) -> bool;

    /// Variant for an action considering buffered and just-pressed cases.
    fn get_input_variant(&self, action: PlayerAction) -> InputVariant;
}

/// Implementation for queries that have both ActionState and InputBuffer
impl InputBufferExt
    for (
        &ActionState<PlayerAction>,
        &InputBuffer,
        u64,
    )
{
    fn pressed_or_buffered(&self, action: PlayerAction) -> bool {
        let (action_state, buffer, current_tick) = self;

        if action_state.just_pressed(&action) {
            return true;
        }

        buffer.check_buffered(action, *current_tick).is_some()
    }

    fn get_input_variant(&self, action: PlayerAction) -> InputVariant {
        let (action_state, buffer, current_tick) = self;

        if action_state.just_pressed(&action) {
            if action_state.pressed(&PlayerAction::Fall) {
                InputVariant::Down
            } else if action_state.pressed(&PlayerAction::UpModifier)
                || action_state.pressed(&PlayerAction::Jump)
            {
                InputVariant::Up
            } else {
                InputVariant::Normal
            }
        } else if let Some(buffered) =
            buffer.check_buffered(action, *current_tick)
        {
            buffered.variant_modifier
        } else {
            InputVariant::Normal
        }
    }
}
