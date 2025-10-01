/// Bow auto-aim helper.
///
/// Behaviour overview:
/// - Scans left/right each tick and becomes decisive only when exactly one side has a hittable enemy (XOR).
/// - Applies a short hysteresis lock to avoid flicker at boundaries and a cooldown between direction flips.
/// - While airborne, can linger for a short time after conditions break to preserve shot intent.
/// - Opts out entirely during burning dash; resets on stealth boundary changes via `reset_autoaim_on_stealth_change`.
use crate::game::enemy::Enemy;
use crate::game::gentstate::Facing;
use crate::game::player::weapon::CurrentWeapon;
use bevy::prelude::*;

// Ranged auto-aim detection range
const RANGED_AUTO_AIM_RANGE: f32 = 200.0;
// Minimum ticks between auto-aim direction changes (to prevent rapid flickering)
const AUTO_AIM_COOLDOWN_TICKS: u16 = 16;
// Ticks to maintain auto-aim when airborne after conditions break
const AUTO_AIM_AIRBORNE_LINGER_TICKS: u16 = 48;

use super::{states::BurningDashing, Grounded, Player};

#[derive(Resource, Debug, Clone)]
pub struct BowAutoAimState {
    /// True when a single side has clear line-of-sight to an enemy and auto-aim should drive facing
    pub(crate) decisive: bool,
    /// Facing chosen by auto-aim when decisive
    pub(crate) target: Option<Facing>,
    /// Small hysteresis to prevent flickering when LOS alternates around boundaries
    pub(crate) lock_ticks: u8,
    /// Cooldown timer preventing rapid auto-aim direction changes (counts down to 0)
    pub(crate) cooldown_ticks: u16,
    /// Airborne linger timer - maintains auto-aim after conditions break while in air (counts down to 0)
    pub(crate) linger_ticks: u16,
    /// True when linger has been activated during current airborne session (reset on landing)
    pub(crate) linger_used: bool,
    /// The enemy entity that triggered auto-aim (used to check relative position during linger)
    pub(crate) target_enemy: Option<Entity>,
}

impl Default for BowAutoAimState {
    fn default() -> Self {
        Self {
            decisive: false,
            target: None,
            lock_ticks: 0,
            cooldown_ticks: 0,
            linger_ticks: 0,
            linger_used: false,
            target_enemy: None,
        }
    }
}

impl BowAutoAimState {
    /// Returns true if auto-aim should block manual-facing updates.
    /// Blocks when: decisive, and either (a) we can apply the desired facing now (cooldown 0)
    /// or (b) the player is already facing the desired target. This avoids "stuck"
    /// windows where auto-aim wants to flip but is still in cooldown — manual input
    /// remains responsive in that gap.
    pub(crate) fn blocks_manual(&self, current: Facing) -> bool {
        if !self.decisive {
            return false;
        }
        if let Some(target) = &self.target {
            self.cooldown_ticks == 0 || current == *target
        } else {
            false
        }
    }
}

/// Continuously adjusts facing when the bow is equipped.
/// Works in all player states (locomotion and skill states).
/// Chooses the side with an unobstructed enemy within range and applies lock + cooldown.
/// When airborne, auto-aim can linger for a few ticks after conditions break (see constant).
pub fn continuous_bow_auto_aim(
    mut query: Query<
        (
            &mut Facing,
            &Transform,
            Has<Grounded>,
            Has<BurningDashing>,
        ),
        With<Player>,
    >,
    _enemy_query: Query<&Transform, With<Enemy>>,
    weapon: CurrentWeapon,
    spatial_query: theseeker_engine::physics::PhysicsWorld,
    mut bow_autoaim_state: ResMut<BowAutoAimState>,
) {
    // Clear auto-aim state if bow is not equipped
    if !weapon.has_bow_equipped() {
        bow_autoaim_state.decisive = false;
        bow_autoaim_state.target = None;
        bow_autoaim_state.lock_ticks = 0;
        bow_autoaim_state.cooldown_ticks = 0;
        bow_autoaim_state.linger_ticks = 0;
        bow_autoaim_state.linger_used = false;
        bow_autoaim_state.target_enemy = None;
        return;
    }

    // Tick timers once per frame
    if bow_autoaim_state.cooldown_ticks > 0 {
        bow_autoaim_state.cooldown_ticks =
            bow_autoaim_state.cooldown_ticks.saturating_sub(1);
    }

    if bow_autoaim_state.linger_ticks > 0 {
        bow_autoaim_state.linger_ticks =
            bow_autoaim_state.linger_ticks.saturating_sub(1);
    }

    for (mut facing, transform, is_grounded, is_burning_dashing) in
        query.iter_mut()
    {
        // Skip auto-aim when burning dashing
        if is_burning_dashing {
            bow_autoaim_state.decisive = false;
            bow_autoaim_state.target = None;
            bow_autoaim_state.lock_ticks = 0;
            bow_autoaim_state.linger_ticks = 0;
            bow_autoaim_state.linger_used = false;
            bow_autoaim_state.target_enemy = None;
            continue;
        }

        let origin = transform.translation.truncate();
        let max_dist = RANGED_AUTO_AIM_RANGE;

        // Check for enemies on both sides and capture entities.
        // Cast to the right: skip non-enemy hits by excluding and retrying.
        let mut right_hit = None;
        let mut exclude: Option<Entity> = None;
        let mut remaining = 3; // limit retries
        while remaining > 0 {
            remaining -= 1;
            if let Some((hit_e, toi)) = spatial_query.ray_cast(
                origin,
                Vec2::X,
                max_dist,
                true,
                theseeker_engine::physics::groups::player_body(),
                exclude,
            ) {
                if _enemy_query.get(hit_e).is_ok() {
                    right_hit = Some((hit_e, toi));
                    break;
                } else {
                    exclude = Some(hit_e);
                    continue;
                }
            } else {
                break;
            }
        }
        // Cast to the left with the same skip logic.
        let mut left_hit = None;
        let mut exclude_l: Option<Entity> = None;
        let mut remaining_l = 3;
        while remaining_l > 0 {
            remaining_l -= 1;
            if let Some((hit_e, toi)) = spatial_query.ray_cast(
                origin,
                Vec2::NEG_X,
                max_dist,
                true,
                theseeker_engine::physics::groups::player_body(),
                exclude_l,
            ) {
                if _enemy_query.get(hit_e).is_ok() {
                    left_hit = Some((hit_e, toi));
                    break;
                } else {
                    exclude_l = Some(hit_e);
                    continue;
                }
            } else {
                break;
            }
        }

        const AUTOAIM_LOCK_TICKS: u8 = 6; // ~62ms at 96Hz - hysteresis to prevent flicker

        // Auto-aim triggers when exactly one side has an enemy (XOR condition)
        if right_hit.is_some() ^ left_hit.is_some() {
            // Valid auto-aim conditions - clear linger and reset availability
            bow_autoaim_state.linger_ticks = 0;
            bow_autoaim_state.linger_used = false; // Reset so linger can be used when conditions break again

            let (desired, enemy_entity) = if let Some((entity, _)) = right_hit {
                (Facing::Right, entity)
            } else if let Some((entity, _)) = left_hit {
                (Facing::Left, entity)
            } else {
                unreachable!()
            };

            // Store the enemy entity for linger position checks
            bow_autoaim_state.target_enemy = Some(enemy_entity);

            // Check if we should change facing (respecting cooldown)
            let should_change_facing =
                if let Some(current_target) = &bow_autoaim_state.target {
                    // Only change if: different direction AND cooldown expired
                    *current_target != desired
                        && bow_autoaim_state.cooldown_ticks == 0
                } else {
                    // No previous target, can change immediately
                    true
                };

            if should_change_facing {
                // Apply the facing change and reset cooldown
                *facing = desired.clone();
                bow_autoaim_state.cooldown_ticks = AUTO_AIM_COOLDOWN_TICKS;
            }

            // Update state (even if we didn't change facing due to cooldown)
            bow_autoaim_state.decisive = true;
            bow_autoaim_state.target = Some(desired);
            bow_autoaim_state.lock_ticks = AUTOAIM_LOCK_TICKS;
        } else if bow_autoaim_state.lock_ticks > 0 {
            // Maintain previous target during lock period (hysteresis)
            bow_autoaim_state.lock_ticks =
                bow_autoaim_state.lock_ticks.saturating_sub(1);
            bow_autoaim_state.decisive = true;
            // Don't reset linger_used here either - lock period is just hysteresis
            // Apply the locked facing if cooldown allows
            if let Some(target) = &bow_autoaim_state.target {
                if bow_autoaim_state.cooldown_ticks == 0 || *facing == *target {
                    *facing = target.clone();
                }
            }
        } else {
            // No valid auto-aim conditions (no enemies or enemies on both sides)
            let is_airborne = !is_grounded;

            // Check if we should start linger
            // This happens when auto-aim WAS active but now isn't, while airborne
            let should_start_linger = is_airborne 
                && bow_autoaim_state.decisive  // Was active
                && bow_autoaim_state.target.is_some()  // Had a target
                && !bow_autoaim_state.linger_used  // Haven't used linger this jump
                && bow_autoaim_state.linger_ticks == 0; // Not currently lingering

            if should_start_linger {
                // Start linger timer
                bow_autoaim_state.linger_ticks = AUTO_AIM_AIRBORNE_LINGER_TICKS;
                bow_autoaim_state.linger_used = true;
                // Keep decisive true during linger
                bow_autoaim_state.decisive = true;
            } else if bow_autoaim_state.linger_ticks > 0 {
                // Currently lingering - check if we should break linger
                let should_break_linger = if let Some(enemy_entity) =
                    bow_autoaim_state.target_enemy
                {
                    // Check if player is 2 pixels below the target enemy
                    if let Ok(enemy_transform) = _enemy_query.get(enemy_entity)
                    {
                        let player_y = transform.translation.y;
                        let enemy_y = enemy_transform.translation.y;
                        player_y < enemy_y - 2.0 // Break linger if player is 2+ pixels below enemy
                    } else {
                        false // Enemy no longer exists, keep lingering
                    }
                } else {
                    false
                };

                if should_break_linger {
                    // Break the linger
                    bow_autoaim_state.linger_ticks = 0;
                    bow_autoaim_state.decisive = false;
                    bow_autoaim_state.target = None;
                    bow_autoaim_state.target_enemy = None;
                } else {
                    // Continue lingering - maintain auto-aim
                    bow_autoaim_state.decisive = true;
                    // Maintain facing during linger (respecting cooldown)
                    if let Some(target) = &bow_autoaim_state.target {
                        if bow_autoaim_state.cooldown_ticks == 0
                            || *facing == *target
                        {
                            *facing = target.clone();
                        }
                    }
                }
            } else {
                // No linger and no valid conditions - deactivate auto-aim
                bow_autoaim_state.decisive = false;
                bow_autoaim_state.target = None;
                bow_autoaim_state.target_enemy = None;
            }

            // Reset linger availability when grounded
            if is_grounded {
                bow_autoaim_state.linger_ticks = 0;
                bow_autoaim_state.linger_used = false;
                bow_autoaim_state.target_enemy = None;
            }
        }
    }
}
