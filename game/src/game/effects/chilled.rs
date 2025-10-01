//! Chilled Effect - Temporary movement speed reduction from ice spider projectiles
//!
//! ## Architecture
//! This effect integrates with the player stat modification pipeline by:
//! 1. Being applied as a component when ice spider projectiles hit the player
//! 2. Modifying `PlayerStatMod.speed` multiplicatively each tick
//! 3. Running after passives but before movement systems in the execution order
//!
//! ## Design Rationale
//! We modify `PlayerStatMod` rather than `PlayerStats` because:
//! - PlayerStatMod is reset to 1.0 each frame, providing a clean slate
//! - Effects can stack multiplicatively with passives naturally
//! - The pattern extends cleanly to other temporary effects

use bevy::prelude::*;

use crate::game::player::{Player, PlayerGfx, PlayerStatMod};
use theseeker_engine::gent::Gent;

// Chilled effect constants
pub const CHILLED_DURATION_TICKS: u32 = 192; // 2 seconds at 96 Hz
pub const CHILLED_SPEED_REDUCTION: f32 = 0.5;
pub const CHILLED_COLOR: Color = Color::srgba(0.48, 0.65, 1.0, 1.0); // Light blue tint

/// Component representing the chilled effect on the player
#[derive(Component, Debug, Clone)]
pub struct ChilledEffect {
    pub remaining_ticks: u32,
    pub speed_modifier: f32,
}

impl ChilledEffect {
    /// Create a basic ice spider chilled effect
    pub fn ice_spider() -> Self {
        Self {
            remaining_ticks: CHILLED_DURATION_TICKS,
            speed_modifier: CHILLED_SPEED_REDUCTION,
        }
    }
}

/// Apply chilled effect to player movement stats and visual appearance.
/// Multiplies `PlayerStatMod.speed` each tick and applies a blue tint.
pub fn chilled_effect_system(
    mut commands: Commands,
    mut query: Query<
        (
            Entity,
            &mut ChilledEffect,
            &Gent,
            &mut PlayerStatMod,
        ),
        With<Player>,
    >,
    mut gfx_query: Query<&mut Sprite, With<PlayerGfx>>,
) {
    for (entity, mut chilled, gent, mut stats) in query.iter_mut() {
        // Apply movement speed reduction (stacks multiplicatively with other modifiers)
        stats.speed *= chilled.speed_modifier;

        // Apply visual blue tint
        if let Ok(mut sprite) = gfx_query.get_mut(gent.e_gfx) {
            sprite.color = CHILLED_COLOR;
        }

        // Remove effect when duration expires
        chilled.remaining_ticks = chilled.remaining_ticks.saturating_sub(1);
        if chilled.remaining_ticks == 0 {
            commands.entity(entity).remove::<ChilledEffect>();
        }
    }
}

/// Restore normal color when chilled ends
pub fn chilled_exit_restoration_system(
    mut removed: RemovedComponents<ChilledEffect>,
    query: Query<&Gent, With<Player>>,
    mut gfx_query: Query<&mut Sprite, With<PlayerGfx>>,
) {
    for entity in removed.read() {
        if let Ok(gent) = query.get(entity) {
            // Restore normal color
            if let Ok(mut sprite) = gfx_query.get_mut(gent.e_gfx) {
                sprite.color = Color::WHITE;
            }
        }
    }
}

/// Refresh chilled duration if already chilled (doesn't stack, just refreshes)
pub fn chilled_refresh_system(
    mut query: Query<&mut ChilledEffect, (With<Player>, Added<ChilledEffect>)>,
) {
    for mut chilled in query.iter_mut() {
        // If chilled was just added, ensure it has full duration
        // This handles the case where player gets chilled while already chilled
        if chilled.remaining_ticks < CHILLED_DURATION_TICKS {
            chilled.remaining_ticks = CHILLED_DURATION_TICKS;
        }
    }
}
