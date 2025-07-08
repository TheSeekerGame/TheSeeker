//! Core AI brain system that evaluates FSM rules.
//! 
//! ## Evaluation Order (per enemy per tick)
//! 
//! 1. Decrement cooldowns
//! 2. Clear previous actions  
//! 3. Evaluate transition rules (logic track, then movement)
//! 4. Process state actions (on_enter if new state, plus tick actions)
//! 5. Increment timers AFTER processing (ensures frame-0 timing)
//! 
//! ## Timing Counters
//! 
//! - **timers[track]**: Ticks in current state (resets on transition)
//! - **anim_tick**: Ticks in animation loop (resets via AnimLoop sensor)  
//! - **state_tick**: Ticks since state OR animation change (for patrol phases)

use bevy::prelude::*;
use super::brain::{CompiledFsm, CompiledFsmInner, CompiledRule, FromPattern, CompiledAction, CompiledCondition};

// Import the components from the ai module
use crate::ai::{FsmInstance, TargetSensor, GroundSensor, RangeSensor};

/// Core AI brain system - evaluates FSM rules and queues actions
pub fn ai_brain_system(
    mut query: Query<(
        Entity,
        &mut FsmInstance,
        &TargetSensor,
        &GroundSensor,
        &RangeSensor,
        &crate::ai::components::HealthSensor,
    )>,
    compiled_assets: Res<Assets<CompiledFsm>>,
) {
    for (entity, mut fsm, target_sensor, ground_sensor, range_sensor, health_sensor) in query.iter_mut() {
        // decrement cooldown timers each tick
        for c in &mut fsm.cooldowns { *c = c.saturating_sub(1); }
        
        // Get the compiled FSM data
        let Some(compiled) = compiled_assets.get(&fsm.brain) else { 
            warn!("Entity {:?}: No compiled FSM found for handle {:?}", entity, fsm.brain);
            continue; 
        };
        
        // Clear previous frame's actions
        fsm.actions.clear();
        
        // NOTE: Do NOT advance timers here – they are incremented **after** rule evaluation
        // and state-action processing to guarantee that `TimerGt(0)` is false on a freshly
        // entered state (spec §6).
        
        let prev_states = [fsm.logic, fsm.movement];
        
        // Evaluate LOGIC track first, capture whether a *terminal* rule fired.
        let logic_terminal = evaluate_track_rules(
            &compiled.inner.logic_rules,
            fsm.logic,
            &mut *fsm,
            target_sensor,
            ground_sensor,
            range_sensor,
            health_sensor,
            true,
        );

        // Only evaluate MOVEMENT track if no terminal rule was hit in the logic track.
        if !logic_terminal {
            evaluate_track_rules(
                &compiled.inner.movement_rules,
                fsm.movement,
                &mut *fsm,
                target_sensor,
                ground_sensor,
                range_sensor,
                health_sensor,
                false,
            );
        }
        
        // Reset state_tick if either state changed
        if fsm.logic != prev_states[0] || fsm.movement != prev_states[1] {
            fsm.state_tick = 0;
        }
        
        // Process state actions for both tracks (logic first, then movement)
        let anim_tick = fsm.anim_tick;
        let state_tick = fsm.state_tick;

        let tracks_data = [
            (fsm.logic, prev_states[0], &compiled.inner.logic_state_actions),
            (fsm.movement, prev_states[1], &compiled.inner.movement_state_actions),
        ];

        for (current_state, prev_state, all_actions) in tracks_data {
            if let Some(actions_for_state) = all_actions.get(current_state as usize) {
                // Queue on_enter actions if the state just changed.
                if current_state != prev_state {
                    fsm.actions.extend(actions_for_state.on_enter.iter().cloned());
                }

                // Always queue tick actions (handles delayed variants inside helper).
                process_tick_actions(
                    &mut fsm.actions,
                    &actions_for_state.tick,
                    anim_tick,
                    state_tick,
                );
            }
        }
        
        // Increment tick counters AFTER processing actions (§6)
        fsm.anim_tick = fsm.anim_tick.saturating_add(1);
        fsm.state_tick = fsm.state_tick.saturating_add(1);

        // Finally advance per-track timers. These were deliberately left untouched until now
        // so that rules evaluated above saw the value **before** the increment.
        fsm.timers[0] = fsm.timers[0].saturating_add(1);
        fsm.timers[1] = fsm.timers[1].saturating_add(1);
    }
}

/// Process tick actions with delayed action support
fn process_tick_actions(
    action_queue: &mut Vec<CompiledAction>,
    tick_actions: &[CompiledAction],
    anim_tick: u16,
    state_tick: u16,
) {
    for action in tick_actions {
        match action {
            CompiledAction::Delayed { tick, inner } => {
                if *tick == anim_tick {
                    action_queue.push((**inner).clone());
                }
            },
            CompiledAction::StateDelayed { tick, inner } => {
                if *tick == state_tick {
                    action_queue.push((**inner).clone());
                }
            },
            other => action_queue.push(other.clone()),
        }
    }
}

/// Evaluate rules for a single track (deterministic priority order)
fn evaluate_track_rules(
    rules: &[CompiledRule],
    current_state: u16,
    fsm: &mut FsmInstance,
    target_sensor: &TargetSensor,
    ground_sensor: &GroundSensor,
    range_sensor: &RangeSensor,
    health_sensor: &crate::ai::components::HealthSensor,
    is_logic_track: bool,
) -> bool {
    for rule in rules {
        // Rules are pre-sorted by priority; first match wins.
        if !rule.from_pattern.matches(current_state) {
            continue;
        }

        let conditions_met = rule.conditions.iter().all(|cond| {
            evaluate_condition(cond, fsm, target_sensor, ground_sensor, range_sensor, health_sensor, is_logic_track)
        });

        if !conditions_met {
            continue;
        }

        // Apply state transition (self-transition allowed).
        if let Some(new_state) = rule.target_state {
            if is_logic_track {
                fsm.logic = new_state;
                fsm.timers[0] = 0;
            } else {
                fsm.movement = new_state;
                fsm.timers[1] = 0;
            }
        }

        // Queue actions for actuator.
        fsm.actions.extend(rule.actions.iter().cloned());

        // Return whether this rule was marked terminal – caller decides what to do.
        return rule.terminal;
    }

    // No rule fired.
    false
}

/// Evaluate a single condition
fn evaluate_condition(
    condition: &CompiledCondition,
    fsm: &mut FsmInstance,
    target_sensor: &TargetSensor,
    ground_sensor: &GroundSensor,
    range_sensor: &RangeSensor,
    health_sensor: &crate::ai::components::HealthSensor,
    is_logic_track: bool,
) -> bool {
    match condition {
        CompiledCondition::Always => true,
        
        CompiledCondition::DistanceLt(distance) => {
            let distance_sq = distance * distance;
            target_sensor.entity.is_some() && target_sensor.dist2 < distance_sq
        },
        
        CompiledCondition::DistanceGt(distance) => {
            let distance_sq = distance * distance;
            // Missing target = infinite distance (enables clean Patrol transition)
            match target_sensor.entity {
                None => true,
                Some(_) => target_sensor.dist2 > distance_sq,
            }
        },
        
        CompiledCondition::TimerGt(ticks) => {
            let timer_index = if is_logic_track { 0 } else { 1 };
            fsm.timers[timer_index] > *ticks
        },
        
        CompiledCondition::IsGrounded(expected) => {
            ground_sensor.on == *expected
        },
        
        CompiledCondition::HealthZero => {
            health_sensor.zero
        },
        
        CompiledCondition::RngChance(p) => {
            let rnd = (fsm.rng_state >> 24) as u8;
            // Advance LCG (parameters as per Numerical Recipes)
            fsm.rng_state = fsm.rng_state.wrapping_mul(1664525).wrapping_add(1013904223);
            rnd < *p
        },
        
        CompiledCondition::Slot { id, expected } => {
            let on = ((fsm.slot_bits >> id) & 1) == 1;
            on == *expected
        },
        
        CompiledCondition::CooldownReady(id) => {
            fsm.cooldowns.get(*id as usize).copied().unwrap_or(0) == 0
        },
    }
} 