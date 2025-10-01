use bevy::prelude::*;
use theseeker_engine::assets::animation::SpriteAnimation;
use theseeker_engine::gent::Gent;
use theseeker_engine::physics::LinearVelocity;
use theseeker_engine::script::ScriptPlayer;
use theseeker_engine::time::GameTickUpdate;

use crate::game::combat::damage_source::Backstab;
use crate::game::combat::{
    sparks::WeaponHitSlot, DamageSource, Health, SparkSource, Stealthed,
};
use crate::game::enemy::{Defense, Enemy};
use crate::game::gentstate::Facing;
use crate::game::player::skills::flicker_strike::FlickerOutroTransition;
use crate::game::player::skills::types::flicker_strike_metadata;
use crate::game::player::spawns::amplified_bell::Bell;
use crate::game::player::states::{
    transition_action, FlickerPhase, FlickerStriking, FlickerVariant, InAir,
    OverridesLocomotion, Ready, WeaponType,
};
use crate::game::player::weapon::PlayerMeleeWeapon;
use crate::game::player::FlickerAbility;
use crate::game::player::{Player, PlayerStatMod, PlayerStateSet};
use crate::game::player::player_anim::set_direction_slots;

// Debug logging helper
macro_rules! debug_log {
    ($($arg:tt)*) => {
        #[cfg(debug_assertions)]
        {
            println!("[FLICKER_DEBUG] {}", format!($($arg)*));
        }
    };
}

pub struct FlickerStrikingStatePlugin;

impl Plugin for FlickerStrikingStatePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            GameTickUpdate,
            (
                flicker_striking_enter_system,
                flicker_striking_phase_system,
                flicker_striking_targeting_system,
                flicker_striking_movement_system,
                flicker_striking_collision_system,
                flicker_striking_animation_system,
                flicker_striking_outro_system,
                flicker_striking_exit_system,
            )
                .chain()
                .in_set(PlayerStateSet::Behavior),
        );
    }
}

fn flicker_striking_enter_system(
    mut query: Query<
        (Entity, &mut FlickerStriking, &Gent),
        (With<Player>, Added<FlickerStriking>),
    >,
    mut gfx_query: Query<&mut ScriptPlayer<SpriteAnimation>>,
) {
    for (entity, mut flickering, gent) in query.iter_mut() {
        debug_log!(
            "ENTER: Entity {:?} starting FlickerStrike with weapon {:?}",
            entity,
            flickering.weapon_type
        );
        debug_log!(
            "ENTER: Initial phase: {:?}, ticks_per_frame: {}",
            flickering.phase,
            flickering.ticks_per_frame
        );

        // Start intro animation
        if let Ok(mut script_player) = gfx_query.get_mut(gent.e_gfx) {
            let anim_key = get_animation_key(
                &flickering.weapon_type,
                &flickering.current_variant,
            );
            debug_log!(
                "ENTER: Starting animation: {}",
                anim_key
            );
            script_player.play_key(&anim_key);
        }
    }
}

fn flicker_striking_phase_system(
    mut commands: Commands,
    mut query: Query<(Entity, &mut FlickerStriking), With<Player>>,
    enemy_query: Query<
        (
            Entity,
            &Transform,
            &Health,
            Has<Defense>,
        ),
        (
            Or<(With<Enemy>, With<Bell>)>,
            Without<Player>,
        ),
    >,
) {
    for (entity, mut flickering) in query.iter_mut() {
        flickering.tick = flickering.tick.saturating_add(1);

        let ticks_in_phase = flickering.tick - flickering.phase_start_tick;

        debug_log!("PHASE: Entity {:?} tick {} in phase {:?} (ticks_in_phase: {}, phase_start: {})",
                  entity, flickering.tick, flickering.phase, ticks_in_phase, flickering.phase_start_tick);

        // Check if current target is still valid (enemy might have died)
        if let Some(target) = flickering.current_target {
            match enemy_query.get(target) {
                Ok((_e, _t, health, _has_def)) => {
                    // Target exists, check if it's dead
                    if health.current <= 0 {
                        debug_log!(
                            "PHASE: Target {:?} is dead, clearing target",
                            target
                        );
                        flickering.current_target = None;
                        flickering.damage_applied = false; // Reset so new target can be damaged
                    }
                },
                Err(_) => {
                    // Target entity no longer exists
                    debug_log!(
                        "PHASE: Target {:?} no longer exists, clearing target",
                        target
                    );
                    flickering.current_target = None;
                    flickering.damage_applied = false; // Reset so new target can be damaged
                },
            }
        }

        match flickering.phase {
            FlickerPhase::Intro => {
                let intro_frames = match flickering.weapon_type {
                    WeaponType::Sword => 1,
                    WeaponType::Hammer => 2,
                    _ => 1,
                };
                let intro_ticks = intro_frames * flickering.ticks_per_frame;

                debug_log!(
                    "PHASE: Intro - frames: {}, ticks: {}, progress: {}/{}",
                    intro_frames,
                    intro_ticks,
                    ticks_in_phase,
                    intro_ticks
                );

                if ticks_in_phase >= intro_ticks {
                    debug_log!("PHASE: Transitioning from Intro to Dashing");
                    // Transition to dashing
                    flickering.phase = FlickerPhase::Dashing;
                    flickering.phase_start_tick = flickering.tick;
                    flickering.current_target = None; // Will be set by targeting system
                }
            },
            FlickerPhase::Dashing => {
                // Handled by collision system
                debug_log!(
                    "PHASE: Dashing - target: {:?}, damage_applied: {}",
                    flickering.current_target,
                    flickering.damage_applied
                );
            },
            FlickerPhase::Damage => {
                let damage_frames = match flickering.weapon_type {
                    WeaponType::Sword => 1,
                    WeaponType::Hammer => 2,
                    _ => 1,
                };
                let damage_ticks = damage_frames * flickering.ticks_per_frame;

                debug_log!("PHASE: Damage - frames: {}, ticks: {}, progress: {}/{}, current_target: {:?}",
                          damage_frames, damage_ticks, ticks_in_phase, damage_ticks, flickering.current_target);

                if ticks_in_phase >= damage_ticks {
                    // If an outro was requested during intro/dash, honor it after this damage resolves
                    if flickering.pending_outro {
                        debug_log!("PHASE: Damage complete with pending outro; transitioning to Outro");
                        flickering.phase = FlickerPhase::Outro;
                        flickering.phase_start_tick = flickering.tick;
                        flickering.current_target = None;
                        flickering.damage_applied = false;
                        flickering.pending_outro = false; // consume request
                    } else {
                        debug_log!("PHASE: Damage phase complete, clearing target and transitioning to Dashing");
                        // Back to dashing for next target
                        flickering.phase = FlickerPhase::Dashing;
                        flickering.phase_start_tick = flickering.tick;
                        flickering.current_target = None; // This will trigger target selection in next frame
                        flickering.damage_applied = false;
                    }
                }
            },
            FlickerPhase::Outro => {
                // Check for outro completion
                let outro_frames = 1;
                let outro_ticks = outro_frames * flickering.ticks_per_frame;

                debug_log!(
                    "PHASE: Outro - frames: {}, ticks: {}, progress: {}/{}",
                    outro_frames,
                    outro_ticks,
                    ticks_in_phase,
                    outro_ticks
                );

                if ticks_in_phase >= outro_ticks {
                    debug_log!(
                        "PHASE: Completing FlickerStrike - removing components"
                    );
                    // Remove state completely
                    end_flicker_strike(&mut commands, entity);
                }
            },
        }
    }
}

fn end_flicker_strike(commands: &mut Commands, entity: Entity) {
    transition_action(commands, entity, Ready);
}

fn flicker_striking_targeting_system(
    mut commands: Commands,
    mut query: Query<
        (
            Entity,
            &mut FlickerStriking,
            &Transform,
            &FlickerAbility,
        ),
        With<Player>,
    >,
    enemy_query: Query<
        (
            Entity,
            &Transform,
            &Health,
            Has<Defense>,
        ),
        (
            Or<(With<Enemy>, With<Bell>)>,
            Without<Player>,
        ),
    >,
    has_outro: Query<&FlickerOutroTransition>,
) {
    for (entity, mut flickering, player_transform, flicker_ability) in
        query.iter_mut()
    {
        let meta = flicker_strike_metadata();
        let player_pos = player_transform.translation.truncate();

        debug_log!(
            "TARGETING: Player at {:?}, phase: {:?}",
            player_pos,
            flickering.phase
        );

        // Handle requested outro transitions carefully to avoid cancelling mid-dash
        if has_outro.single().is_ok()
            && !matches!(flickering.phase, FlickerPhase::Outro)
        {
            if matches!(flickering.phase, FlickerPhase::Dashing)
                && flickering.current_target.is_some()
            {
                // Defer outro until after this dash completes
                debug_log!("TARGETING: Outro requested but mid-dash; deferring until post-dash");
                flickering.pending_outro = true;
                commands.entity(entity).remove::<FlickerOutroTransition>();
            } else if matches!(flickering.phase, FlickerPhase::Intro) {
                // Do not cancel during intro; defer to after intro completes
                debug_log!("TARGETING: Outro requested during intro; deferring until after intro");
                flickering.pending_outro = true;
                commands.entity(entity).remove::<FlickerOutroTransition>();
            } else {
                debug_log!(
                    "TARGETING: Starting outro transition at tick {}",
                    flickering.tick
                );
                flickering.phase = FlickerPhase::Outro;
                flickering.phase_start_tick = flickering.tick;
                flickering.current_target = None;
                commands.entity(entity).remove::<FlickerOutroTransition>();
                continue;
            }
        }

        // Do not process pending_outro here before first dash; it will be
        // honored after a dash completes (in Damage), ensuring at least
        // intro + one dash occurs even on a quick tap.

        // Only target during dashing phase with no current target
        if !matches!(flickering.phase, FlickerPhase::Dashing) {
            debug_log!(
                "TARGETING: Skipping - not in dashing phase (phase: {:?})",
                flickering.phase
            );
            continue;
        }

        if flickering.current_target.is_some() {
            debug_log!("TARGETING: Already has target {:?}, skipping new target selection", flickering.current_target);
            continue;
        }

        debug_log!("TARGETING: Need to select new target (phase: {:?}, current_variant: {:?})", 
                  flickering.phase, flickering.current_variant);

        // Energy gate per dash: require enough for a chunk before initiating a new dash
        if flicker_ability.energy < meta.chunk_cost {
            debug_log!(
                "TARGETING: Insufficient energy for next dash (have {:.2}, need {:.2}); transitioning to Outro",
                flicker_ability.energy,
                meta.chunk_cost
            );
            flickering.phase = FlickerPhase::Outro;
            flickering.phase_start_tick = flickering.tick;
            flickering.current_target = None;
            continue;
        }

        // Gather candidates in range
        let mut nondef_candidates: Vec<(Entity, Vec2, f32)> = Vec::new();
        let mut def_candidates: Vec<(Entity, Vec2, f32)> = Vec::new();
        let mut alive_enemy_count = 0;
        let mut alive_enemies_in_range = 0;

        for (enemy_entity, enemy_transform, health, has_defense) in
            enemy_query.iter()
        {
            // Skip dead enemies
            if health.current <= 0 {
                debug_log!(
                    "TARGETING: Skipping dead enemy {:?} with health {}",
                    enemy_entity,
                    health.current
                );
                continue;
            }

            alive_enemy_count += 1;

            let enemy_pos = enemy_transform.translation.truncate();
            let distance = player_pos.distance(enemy_pos);

            debug_log!(
                "TARGETING: Alive enemy {:?} at {:?}, distance: {:.1}, in range: {}",
                enemy_entity,
                enemy_pos,
                distance,
                distance <= meta.range
            );

            if distance <= meta.range {
                alive_enemies_in_range += 1;
                if has_defense {
                    def_candidates.push((enemy_entity, enemy_pos, distance));
                    debug_log!(
                        "TARGETING: Defense candidate {:?}",
                        enemy_entity
                    );
                } else {
                    nondef_candidates.push((enemy_entity, enemy_pos, distance));
                    debug_log!(
                        "TARGETING: Non-defense candidate {:?}",
                        enemy_entity
                    );
                }
            }
        }

        // Use non-defense targets if available; otherwise, fall back to defense-only mode
        let have_nondef_now = !nondef_candidates.is_empty();
        let mut valid_targets: Vec<(Entity, Vec2, f32)> = if have_nondef_now {
            // From non-defense candidates, filter out those already damaged unless only 1 candidate
            if nondef_candidates.len() <= 1 {
                nondef_candidates
            } else {
                nondef_candidates
                    .into_iter()
                    .filter(|(e, _, _)| {
                        !flickering.damaged_entities.contains(e)
                    })
                    .collect()
            }
        } else {
            // No non-defense candidates; use defense
            def_candidates
        };

        debug_log!(
            "TARGETING: Found {} valid targets from {} candidates ({} alive total)",
            valid_targets.len(), alive_enemies_in_range, alive_enemy_count
        );
        debug_log!(
            "TARGETING: Already damaged: {:?}",
            flickering.damaged_entities
        );

        if valid_targets.is_empty() {
            // No valid targets found
            debug_log!("TARGETING: No valid targets. Alive enemies: {}, alive in range: {}", 
                      alive_enemy_count, alive_enemies_in_range);

            if alive_enemies_in_range == 0 {
                debug_log!("TARGETING: No enemies in range, transitioning to outro at tick {}", flickering.tick);
                // Transition to outro
                flickering.phase = FlickerPhase::Outro;
                flickering.phase_start_tick = flickering.tick;
                flickering.current_target = None; // Ensure target is cleared
                                                  // Ghosting will be removed when FlickerStriking is removed
            } else {
                debug_log!("TARGETING: Clearing damaged set to allow re-targeting (damaged count: {})", 
                          flickering.damaged_entities.len());
                // Clear damaged set to allow re-targeting
                flickering.damaged_entities.clear();
            }
            continue;
        }

        // If we previously selected in defense-only mode, and we still don't have any non-defense
        // candidates, do not chain to another target; end now after the single hit.
        if flickering.defense_only_mode && !have_nondef_now {
            debug_log!("TARGETING: Defense-only mode persists; ending after single hit");
            flickering.phase = FlickerPhase::Outro;
            flickering.phase_start_tick = flickering.tick;
            flickering.current_target = None;
            flickering.defense_only_mode = false;
            continue;
        }

        // Select target
        let (target_entity, target_pos, target_index_opt) =
            if flickering.last_target.is_none() {
                // First selection in this FlickerStrike: pick nearest.
                // If multiple at the exact same distance, choose pseudo-randomly using tick.
                let min_dist = valid_targets
                    .iter()
                    .map(|t| t.2)
                    .fold(f32::INFINITY, |a, b| a.min(b));
                let ties: Vec<(usize, (Entity, Vec2, f32))> = valid_targets
                    .iter()
                    .cloned()
                    .enumerate()
                    .filter(|(_, (_e, _pos, d))| *d == min_dist)
                    .collect();
                let choice = if ties.len() == 1 {
                    ties[0].clone()
                } else {
                    let pick = (flickering.tick as usize) % ties.len();
                    ties[pick].clone()
                };
                let (idx, (entity, pos, _)) = choice;
                (entity, pos, Some(idx))
            } else {
                // Subsequent selections: pseudo-random based on tick, avoid immediate repeats
                let mut target_index =
                    (flickering.tick as usize) % valid_targets.len();
                if valid_targets.len() > 1 {
                    if let Some(prev) = flickering.last_target {
                        if valid_targets[target_index].0 == prev {
                            target_index =
                                (target_index + 1) % valid_targets.len();
                        }
                    }
                }
                let (entity, pos, _dist) = valid_targets[target_index];
                (entity, pos, Some(target_index))
            };

        if let Some(idx) = target_index_opt {
            debug_log!(
                "TARGETING: Selected target {:?} at index {}",
                target_entity,
                idx
            );
        } else {
            debug_log!(
                "TARGETING: Selected target {:?} (nearest)",
                target_entity
            );
        }

        // Calculate angle and select variant
        let direction = target_pos - player_pos;
        let angle = direction.y.atan2(direction.x);
        let angle_degrees = angle.to_degrees();
        flickering.current_variant = select_variant_from_angle(angle);
        // Remember last target to avoid immediate repeats
        flickering.last_target = Some(target_entity);
        flickering.current_target = Some(target_entity);

        // Mark defense-only mode for this run if we had to select from defense targets
        flickering.defense_only_mode = !have_nondef_now;

        debug_log!(
            "TARGETING: Target {:?} at {:?}, player at {:?}",
            target_entity,
            target_pos,
            player_pos
        );
        debug_log!(
            "TARGETING: Direction vector: {:?}, angle: {:.1}° => variant: {:?}",
            direction,
            angle_degrees,
            flickering.current_variant
        );
    }
}

fn flicker_striking_movement_system(
    mut query: Query<
        (
            &mut LinearVelocity,
            &mut Transform,
            &mut Facing,
            &FlickerStriking,
        ),
        With<Player>,
    >,
    enemy_query: Query<
        &Transform,
        (
            Or<(With<Enemy>, With<Bell>)>,
            Without<Player>,
        ),
    >,
) {
    const DASH_SPEED: f32 = 16.0; // Pixels per tick - reduced for more precise collision detection

    for (mut velocity, player_transform, mut facing, flickering) in
        query.iter_mut()
    {
        // Only move during dashing phase with a target
        if !matches!(flickering.phase, FlickerPhase::Dashing) {
            debug_log!("MOVEMENT: Not in dashing phase (phase: {:?}), setting velocity to zero", flickering.phase);
            velocity.0 = Vec2::ZERO;
            continue;
        }

        let Some(target_entity) = flickering.current_target else {
            debug_log!("MOVEMENT: No target set, cannot move");
            velocity.0 = Vec2::ZERO;
            continue;
        };

        let Ok(target_transform) = enemy_query.get(target_entity) else {
            debug_log!(
                "MOVEMENT: Target {:?} no longer exists, stopping movement",
                target_entity
            );
            velocity.0 = Vec2::ZERO;
            continue;
        };

        let player_pos = player_transform.translation.truncate();
        let target_pos = target_transform.translation.truncate();
        let direction = (target_pos - player_pos).normalize_or_zero();

        // Set velocity toward target
        let distance_to_target = player_pos.distance(target_pos);
        debug_log!(
            "MOVEMENT: Distance to target: {:.1}, dash speed: {:.1}",
            distance_to_target,
            DASH_SPEED
        );

        if distance_to_target < DASH_SPEED {
            // Adjust velocity to reach exactly
            velocity.0 = direction * distance_to_target;
            debug_log!(
                "MOVEMENT: Adjusting velocity to reach exactly: {:?}",
                velocity.0
            );
        } else {
            velocity.0 = direction * DASH_SPEED;
            debug_log!(
                "MOVEMENT: Setting full dash velocity: {:?}",
                velocity.0
            );
        }

        // Update facing direction
        if direction.x > 0.0 {
            *facing = Facing::Right;
        } else if direction.x < 0.0 {
            *facing = Facing::Left;
        }
    }
}

fn flicker_striking_collision_system(
    mut commands: Commands,
    mut query: Query<
        (
            Entity,
            &mut FlickerStriking,
            &Transform,
            &mut FlickerAbility,
            Option<&PlayerStatMod>,
    Has<crate::game::effects::stealthed::StealthEffect>,
        ),
        With<Player>,
    >,
    enemy_query: Query<
        (
            Entity,
            &Transform,
            &crate::game::gentstate::Facing,
        ),
        (
            Or<(With<Enemy>, With<Bell>)>,
            Without<crate::game::player::Player>,
        ),
    >,
) {
    const COLLISION_DISTANCE: f32 = 16.0; // Pixels - increased for reliable collision detection

    for (
        player_entity,
        mut flickering,
        player_transform,
        mut flicker_ability,
        player_stat_mod,
        is_stealthed,
    ) in query.iter_mut()
    {
        debug_log!(
            "COLLISION: Player {:?} in phase {:?}",
            player_entity,
            flickering.phase
        );

        // Only check during dashing phase with a target
        if !matches!(flickering.phase, FlickerPhase::Dashing) {
            debug_log!("COLLISION: Skipping - not in dashing phase");
            continue;
        }

        let Some(target_entity) = flickering.current_target else {
            debug_log!("COLLISION: No target set");
            continue;
        };

        let Ok((enemy_entity, enemy_transform, enemy_facing)) =
            enemy_query.get(target_entity)
        else {
            debug_log!("COLLISION: Target enemy {:?} not found (likely died), will be cleared next tick", target_entity);
            continue;
        };

        let player_pos = player_transform.translation.truncate();
        let enemy_pos = enemy_transform.translation.truncate();
        let distance = player_pos.distance(enemy_pos);

        debug_log!("COLLISION: Player at {:?}, enemy {:?} at {:?}, distance: {:.1}, collision_dist: {:.1}",
                  player_pos, enemy_entity, enemy_pos, distance, COLLISION_DISTANCE);

        // Check for collision with generous distance
        if distance <= COLLISION_DISTANCE {
            debug_log!(
                "COLLISION: Hit detected! Transitioning to damage phase"
            );
            // Transition to damage phase
            flickering.phase = FlickerPhase::Damage;
            flickering.phase_start_tick = flickering.tick;

            
            // Apply damage if not already done
            if !flickering.damage_applied {
                // Debit energy chunk at the start of Damage phase
                let before = flicker_ability.energy;
                let after = (before - flicker_strike_metadata().chunk_cost).max(0.0);
                flicker_ability.energy = after;
                debug_log!(
                    "COLLISION: Debiting energy chunk: {:.2} -> {:.2}",
                    before,
                    after
                );

                let base_damage = match flickering.weapon_type {
                    WeaponType::Sword => PlayerMeleeWeapon::Sword.base_damage(),
                    WeaponType::Hammer => {
                        PlayerMeleeWeapon::Hammer.base_damage()
                    },
                    _ => 10.0,
                };

                let mut damage = base_damage;

                // Apply player damage modifiers
                if let Some(stat_mod) = player_stat_mod {
                    damage *= stat_mod.damage;
                }

                // Check for backstab (enemy facing away from player)
                let is_backstab = match *enemy_facing {
                    crate::game::gentstate::Facing::Left => {
                        enemy_transform.translation.x
                            < player_transform.translation.x
                    },
                    crate::game::gentstate::Facing::Right => {
                        enemy_transform.translation.x
                            > player_transform.translation.x
                    },
                };

                debug_log!("COLLISION: Creating damage source - base: {:.1}, modified: {:.1}, backstab: {}",
                          base_damage, damage, is_backstab);

                // Determine the weapon hit slot for sparks
                let weapon_slot = match flickering.weapon_type {
                    WeaponType::Sword => "SwordHit",
                    WeaponType::Hammer => "HammerHit",
                    _ => "SwordHit",
                };
                debug_log!(
                    "COLLISION: Setting weapon slot: {}",
                    weapon_slot
                );

                // Spawn DamageSource for sparks/audio
                let damage_source_entity = commands
                    .spawn((
                        // Give a few ticks of lifetime to ensure the combat pipeline
                        // sees this entity regardless of system ordering on the current frame
                        DamageSource::new(3, player_entity, damage)
                            .with_max_targets(1),
                        SparkSource::Default, // Ensure default sparks spawn
                        WeaponHitSlot {
                            slot_name: weapon_slot.to_string(),
                        },
                        // Place directly at the enemy’s position for reliable overlap tests
                        Transform::from_translation(
                            enemy_transform.translation,
                        ),
                        GlobalTransform::default(),
                        theseeker_engine::physics::Collider::cuboid(8.0, 8.0),
                        // Use canonical collision groups for player attacks
                        theseeker_engine::physics::groups::player_attack(),
                    ))
                    .id();

                if is_backstab {
                    commands.entity(damage_source_entity).insert(Backstab);
                    debug_log!(
                        "COLLISION: Added backstab component to damage source"
                    );
                }
                // If the player is currently stealthed, mark this damage as stealthed
                if is_stealthed {
                    commands.entity(damage_source_entity).insert(Stealthed);
                    debug_log!("COLLISION: Added Stealthed component to damage source for lifesteal and 2x damage");
                }

                // Add to damaged set
                flickering.damaged_entities.insert(enemy_entity);
                flickering.damage_applied = true;

                debug_log!(
                    "COLLISION: Damage source {:?} created for enemy {:?}",
                    damage_source_entity,
                    enemy_entity
                );
            } else {
                debug_log!("COLLISION: Damage already applied, skipping");
            }
        } else {
            debug_log!("COLLISION: Not close enough yet");
        }
    }
}

fn flicker_striking_animation_system(
    mut query: Query<(&FlickerStriking, &Gent, &Facing), With<Player>>,
    mut gfx_query: Query<&mut ScriptPlayer<SpriteAnimation>>,
) {
    for (flickering, gent, facing) in query.iter_mut() {
        if let Ok(mut script_player) = gfx_query.get_mut(gent.e_gfx) {
            let current_anim = script_player
                .current_key()
                .map(|s| s.as_ref())
                .unwrap_or("None");
            debug_log!(
                "ANIMATION: Phase {:?}, variant {:?}, tick {}, anim: {}",
                flickering.phase,
                flickering.current_variant,
                flickering.tick,
                current_anim
            );

            // Update direction slots based on facing
            set_direction_slots(&mut script_player, facing);

            // Update animation based on phase and variant
            let anim_key = get_animation_key(
                &flickering.weapon_type,
                &flickering.current_variant,
            );

            // Set animation slot for phase
            match flickering.phase {
                FlickerPhase::Intro => {
                    debug_log!("ANIMATION: Setting intro slots (expected frame 0 for intro)");
                    script_player.set_slot("FlickerIntro", true);
                    script_player.set_slot("FlickerDashing", false);
                    script_player.set_slot("FlickerDamage", false);
                    script_player.set_slot("FlickerOutro", false);
                },
                FlickerPhase::Dashing => {
                    debug_log!("ANIMATION: Setting dashing slots (expected frame 1 for sword, 2 for hammer)");
                    script_player.set_slot("FlickerIntro", false);
                    script_player.set_slot("FlickerDashing", true);
                    script_player.set_slot("FlickerDamage", false);
                    script_player.set_slot("FlickerOutro", false);
                },
                FlickerPhase::Damage => {
                    debug_log!("ANIMATION: Setting damage slots (expected frame 2 for sword, 3-4 for hammer)");
                    script_player.set_slot("FlickerIntro", false);
                    script_player.set_slot("FlickerDashing", false);
                    script_player.set_slot("FlickerDamage", true);
                    script_player.set_slot("FlickerOutro", false);
                },
                FlickerPhase::Outro => {
                    debug_log!("ANIMATION: Setting outro slots (expected frames 3-6 for sword, 5-8 for hammer)");
                    script_player.set_slot("FlickerIntro", false);
                    script_player.set_slot("FlickerDashing", false);
                    script_player.set_slot("FlickerDamage", false);
                    script_player.set_slot("FlickerOutro", true);
                },
            }

            // Only play new animation if variant changed
            if script_player.current_key() != Some(&anim_key) {
                debug_log!(
                    "ANIMATION: Switching from {} to {}",
                    script_player
                        .current_key()
                        .map(|s| s.as_ref())
                        .unwrap_or("None"),
                    anim_key
                );
                script_player.play_key(&anim_key);
            } else {
                debug_log!(
                    "ANIMATION: Keeping current animation: {}",
                    anim_key
                );
            }
        }
    }
}

fn flicker_striking_outro_system(
    mut commands: Commands,
    query: Query<(Entity, &FlickerStriking), With<Player>>,
) {
    for (entity, flickering) in query.iter() {
        // Remove OverridesLocomotion during outro
        if matches!(flickering.phase, FlickerPhase::Outro) {
            commands
                .entity(entity)
                .remove::<OverridesLocomotion>()
                .remove::<InAir>();
        }
    }
}

fn flicker_striking_exit_system(
    mut commands: Commands,
    mut _query: Query<
        (Entity, &FlickerStriking),
        (With<Player>, Without<FlickerStriking>),
    >,
    mut removed: RemovedComponents<FlickerStriking>,
    mut gfx_query: Query<&mut ScriptPlayer<SpriteAnimation>>,
    gent_query: Query<&Gent>,
) {
    for entity in removed.read() {
        debug_log!(
            "EXIT: FlickerStriking removed from entity {:?}",
            entity
        );

        // Clean up any lingering components when FlickerStriking is removed
        super::super::queue_state_cleanup(
            &mut commands,
            entity,
            super::super::cleanup_flicker_strike,
        );

        // Clear all flicker animation slots
        if let Ok(gent) = gent_query.get(entity) {
            if let Ok(mut script_player) = gfx_query.get_mut(gent.e_gfx) {
                debug_log!(
                    "EXIT: Clearing flicker animation slots, anim was: {}",
                    script_player
                        .current_key()
                        .map(|s| s.as_ref())
                        .unwrap_or("None")
                );
                script_player.set_slot("FlickerIntro", false);
                script_player.set_slot("FlickerDashing", false);
                script_player.set_slot("FlickerDamage", false);
                script_player.set_slot("FlickerOutro", false);
            }
        }
    }
}

fn select_variant_from_angle(angle: f32) -> FlickerVariant {
    let degrees = angle.to_degrees();

    // Normalize to 0-360
    let normalized = if degrees < 0.0 {
        degrees + 360.0
    } else {
        degrees
    };

    match normalized {
        337.5..=360.0 | 0.0..=22.5 => FlickerVariant::Forward,
        22.5..=67.5 => FlickerVariant::FrontUpward,
        67.5..=112.5 => FlickerVariant::Upward,
        112.5..=157.5 => FlickerVariant::FrontUpward, // Will be flipped
        157.5..=202.5 => FlickerVariant::Forward,     // Will be flipped
        202.5..=247.5 => FlickerVariant::FrontDownward, // Will be flipped
        247.5..=292.5 => FlickerVariant::Downward,
        292.5..=337.5 => FlickerVariant::FrontDownward,
        _ => FlickerVariant::Forward,
    }
}

fn get_animation_key(
    weapon_type: &WeaponType,
    variant: &FlickerVariant,
) -> String {
    let weapon_prefix = match weapon_type {
        WeaponType::Sword => "Sword",
        WeaponType::Hammer => "Hammer",
        _ => "Sword",
    };

    let variant_suffix = match variant {
        FlickerVariant::Forward => "Forward",
        FlickerVariant::Upward => "Upward",
        FlickerVariant::Downward => "Downward",
        FlickerVariant::FrontUpward => "FrontUpward",
        FlickerVariant::FrontDownward => "FrontDownward",
    };

    format!(
        "anim.player.{}Flicker{}",
        weapon_prefix, variant_suffix
    )
}
