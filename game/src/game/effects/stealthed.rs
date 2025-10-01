//! Stealth Effect - Temporary invisibility with optional speed modifier
//!
//! ## Architecture
//! This effect integrates with the player stat modification pipeline by:
//! 1. Being applied when the player activates the stealth skill
//! 2. Modifying `PlayerStatMod.speed` multiplicatively each tick (no bonus by default)
//! 3. Making the player semi-transparent for visual feedback
//! 4. Breaking on damage dealt
//!
//! ## Design Consistency
//! Like the chilled effect, this modifies PlayerStatMod rather than directly
//! changing velocity, ensuring it works across all movement states and stacks
//! properly with other modifiers.

use bevy::prelude::*;

use crate::game::combat::DamageInfo;
use crate::game::player::skills::cooldowns::Cooldowns;
use crate::game::player::skills::types::{stealth_metadata, SkillId};
use crate::game::player::{
    Passive, Passives, Player, PlayerGfx, PlayerStatMod,
};
use crate::prelude::*;
use theseeker_engine::gent::Gent;

// Stealth constants
pub const STEALTH_DURATION: f32 = 3.0;
pub const STEALTH_SPEED_MULT: f32 = 1.0; // no speed bonus (1.0) by default

#[derive(Component, Debug)]
pub struct StealthEffect {
    /// Hard deadline in game ticks when stealth must end.
    /// Using ticks avoids any floating point drift or scheduling anomalies.
    pub start_tick: u64,
    pub expire_tick: u64,
    pub speed_modifier: f32,
    /// Damage source that should not break stealth (e.g., the Dash Strike that triggered Pulse Drive).
    pub ignore_damage_from: Option<Entity>,
}

impl StealthEffect {
    /// Create a standard stealth effect with a hard tick deadline.
    /// `now_tick` is the current `GameTime.tick()` when the effect starts.
    pub fn new(now_tick: u64) -> Self {
        // Convert seconds to ticks using the global game tick rate (96 Hz).
        // We keep this local to effect construction so runtime updates only compare ticks.
        let duration_ticks = (STEALTH_DURATION * 96.0).round() as u64;
        Self {
            start_tick: now_tick,
            expire_tick: now_tick.saturating_add(duration_ticks),
            speed_modifier: STEALTH_SPEED_MULT,
            ignore_damage_from: None,
        }
    }
}

/// Apply stealth effect to movement speed and visual appearance.
/// Multiplies `PlayerStatMod.speed` and reduces sprite alpha while active.
pub fn stealth_effect_system(
    mut commands: Commands,
    mut query: Query<
        (
            Entity,
            &StealthEffect,
            &Gent,
            &mut PlayerStatMod,
        ),
        With<Player>,
    >,
    mut gfx_query: Query<&mut Sprite, With<PlayerGfx>>,
    time: Res<GameTime>,
) {
    for (entity, stealth, gent, mut stats) in query.iter_mut() {
        // Apply movement speed boost (stacks multiplicatively with other modifiers)
        stats.speed *= stealth.speed_modifier;

        // Apply visual transparency
        if let Ok(mut sprite) = gfx_query.get_mut(gent.e_gfx) {
            sprite.color.set_alpha(0.3);
        }

        // Remove effect when deadline reached (robust against drift or missed frames)
        if time.tick() >= stealth.expire_tick {
            commands.entity(entity).remove::<StealthEffect>();
        }
    }
}

/// Restore full visibility when stealth ends.
pub fn stealth_exit_visibility_system(
    mut removed: RemovedComponents<StealthEffect>,
    query: Query<&Gent, With<Player>>,
    mut gfx_query: Query<&mut Sprite, With<PlayerGfx>>,
) {
    for entity in removed.read() {
        if let Ok(gent) = query.get(entity) {
            if let Ok(mut sprite) = gfx_query.get_mut(gent.e_gfx) {
                sprite.color.set_alpha(1.0);
            }
        }
    }
}

/// When stealth ends (duration expired or broken by damage), stamp the stealth cooldown.
pub fn start_stealth_cooldown_on_remove(
    mut removed: RemovedComponents<StealthEffect>,
    players: Query<Entity, With<Player>>,
    mut cooldowns: ResMut<Cooldowns>,
    time: Res<GameTime>,
    stats: Query<Option<&PlayerStatMod>, With<Player>>,
) {
    let now_tick = time.tick() as u64;
    for entity in removed.read() {
        if players.contains(entity) {
            let cdr_snapshot = stats
                .get(entity)
                .ok()
                .flatten()
                .map(|s| s.cdr)
                .unwrap_or(1.0);
            cooldowns.start(
                entity,
                SkillId::Stealth,
                stealth_metadata().cooldown,
                cdr_snapshot,
                now_tick,
            );
        }
    }
}

/// Break stealth if the player deals damage.
pub fn stealth_damage_break_system(
    mut commands: Commands,
    mut damage_events: EventReader<DamageInfo>,
    query: Query<Option<&StealthEffect>, With<Player>>,
    // Do not break stealth when the player hits the bell
    is_bell: Query<(), With<crate::game::player::spawns::amplified_bell::Bell>>,
    passives_q: Query<&Passives>,
    time: Res<GameTime>,
) {
    let current_tick = time.tick();
    for event in damage_events.read() {
        let Some(stealth) = query.get(event.owner).ok().flatten() else {
            continue;
        };

        // Ignore damage sourced from the attack that granted stealth (e.g., Dash Strike crit).
        if stealth.ignore_damage_from == Some(event.source) {
            continue;
        }

        // Ignore damage that occurred on or before the tick stealth was granted.
        if current_tick <= stealth.start_tick {
            continue;
        }

        // Only break stealth if the target is NOT a bell
        if is_bell.get(event.target).is_err() {
            // If Shadow Cloak is equipped, do not break stealth on damage
            let has_shadow_cloak = passives_q
                .get(event.owner)
                .ok()
                .map(|p| p.contains(&Passive::ShadowCloak))
                .unwrap_or(false);
            if !has_shadow_cloak {
                commands.entity(event.owner).remove::<StealthEffect>();
            }
        }
    }
}

// Removed: legacy stealthed-crit cooldown reset behavior. This functionality is now
// owned by the Ice Dagger passive on backstab kill.
