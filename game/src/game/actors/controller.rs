//! Decisions retain only the memory needed to select an activity.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use super::catalog::ResolvedActor;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ControllerSpec {
    Combat {
        #[serde(default)]
        approach: Approach,
        #[serde(default)]
        committed: bool,
        #[serde(default)]
        defensive: bool,
        #[serde(default)]
        recovery: Recovery,
    },
    Wander,
    Pursuit,
    Hopper,
    Ambush,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Recovery { #[default] Pause, Align }

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Approach {
    #[default]
    Ground,
    Roll,
    Flying,
    Stationary,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Activity {
    #[default]
    Patrol,
    Approach,
    Attack,
    Defense,
    Recover,
    Idle,
    Jump,
    AirJump,
    Fall,
    Prepare,
    AttackLeft,
    AttackRight,
    EatLeft,
    EatRight,
    Death,
}

/// Facts reported by body-specific sensing and movement algorithms.
#[derive(Debug, Clone, Copy, Default)]
pub struct MotorObservations {
    pub aligned: bool,
    pub roll_ready: bool,
    pub idle_ready: bool,
    pub jump_done: bool,
    pub air_jump: bool,
    pub detected_left: bool,
    pub detected_right: bool,
    pub killed_target: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum PatrolPhase {
    #[default]
    Idle,
    Walk,
}

#[derive(Component)]
pub struct ActorController {
    pub definition: Arc<ResolvedActor>,
    pub hopper: super::hopper::HopperMemory,
    pub roam_target: Option<Vec2>,
    pub roam_wait: u16,
    pub activity: Activity,
    pub playback: theseeker_engine::animation::PlaybackId,
    pub previous_activity: Activity,
    pub alignment_ticks: u16,
    pub recovery_remaining: u32,
    pub entered: bool,
    pub elapsed: u32,
    pub patrol_phase: PatrolPhase,
    pub patrol_elapsed: u32,
    pub patrol_changed: bool,
    patrol_initial: bool,
    pub rng_state: u32,
    pub observations: MotorObservations,
}

impl ActorController {
    pub fn new(definition: Arc<ResolvedActor>, rng_state: u32) -> Self {
        let activity = match definition.behavior {
            ControllerSpec::Hopper | ControllerSpec::Ambush => Activity::Idle,
            ControllerSpec::Combat { .. } | ControllerSpec::Wander | ControllerSpec::Pursuit => Activity::Patrol,
        };
        Self {
            hopper: super::hopper::HopperMemory::default(), roam_target: None, roam_wait: 0,
            definition, activity, playback: theseeker_engine::animation::PlaybackId::default(), previous_activity: activity, alignment_ticks: 0, recovery_remaining: 0, entered: true, elapsed: 0,
            patrol_phase: PatrolPhase::Idle, patrol_elapsed: 0, patrol_changed: true, patrol_initial: true,
            rng_state, observations: MotorObservations::default(),
        }
    }

    pub fn transition(&mut self, activity: Activity) {
        if self.activity == activity {
            return;
        }
        self.previous_activity = self.activity;
        self.activity = activity;
        self.entered = true;
        self.elapsed = 0;
        if activity == Activity::Patrol {
            self.patrol_initial = false;
            self.patrol_phase = PatrolPhase::Idle;
            self.patrol_elapsed = 0;
            self.patrol_changed = true;
        }
    }

    pub fn aligns_during_recovery(&self) -> bool {
        matches!(self.definition.behavior, ControllerSpec::Combat { recovery: Recovery::Align, .. })
    }

    pub fn advance_decision_clocks(&mut self) {
        let aligning = self.activity == Activity::Approach
            || (self.activity == Activity::Recover && self.aligns_during_recovery());
        if aligning && self.observations.aligned {
            self.alignment_ticks = self.alignment_ticks.saturating_add(1);
        } else { self.alignment_ticks = 0; }
        self.recovery_remaining = self.recovery_remaining.saturating_sub(1);
    }

    pub fn begin_recovery(&mut self, action_recovery: u32) {
        if self.aligns_during_recovery() {
            self.recovery_remaining = action_recovery;
        } else {
            self.rng_state = self.rng_state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            self.recovery_remaining = 41 + self.rng_state % 25;
        }
    }

    fn aligned_for_attack(&self) -> bool {
        let ticks = self.definition.archetype.flying.and_then(|flying| flying.stalk)
            .and_then(|stalk| stalk.windup_ticks).unwrap_or(24);
        self.observations.aligned && self.alignment_ticks >= ticks
    }

    pub fn tick_patrol(&mut self, scale: f32) {
        self.patrol_changed = self.entered;
        let idle_ticks = ((75.0 * scale).round() as u32).max(1);
        let walk_ticks = ((225.0 * scale).round() as u32).max(1);
        let duration = match self.patrol_phase {
            PatrolPhase::Idle => idle_ticks + u32::from(!self.patrol_initial),
            PatrolPhase::Walk => walk_ticks + 1,
        };
        self.patrol_changed |= self.patrol_phase == PatrolPhase::Walk && self.patrol_elapsed == idle_ticks + 1;
        if self.patrol_elapsed == duration {
            self.patrol_phase = match self.patrol_phase {
                PatrolPhase::Idle => PatrolPhase::Walk,
                PatrolPhase::Walk => PatrolPhase::Idle,
            };
            self.patrol_initial = false;
            self.patrol_elapsed = 0;
            self.patrol_changed = true;
        }
        self.patrol_elapsed = self.patrol_elapsed.saturating_add(1);
    }
}

#[derive(Clone, Copy, Default)]
pub struct DecisionInput {
    pub in_vision: bool,
    pub in_melee: bool,
    pub outside_vision: bool,
    pub outside_melee: bool,
    pub grounded: bool,
    pub dead: bool,
    pub action_done: bool,
    pub attack_done: bool,
    pub prepare_done: bool,
    pub eat_done: bool,
}

pub fn decide(actor: &ActorController, input: DecisionInput) -> Activity {
    if input.dead {
        return Activity::Death;
    }
    match actor.definition.behavior {
        ControllerSpec::Combat { approach, committed, defensive, .. } => {
            combat_activity(actor, input, approach, committed, defensive)
        },
        ControllerSpec::Wander => Activity::Patrol,
        ControllerSpec::Pursuit => pursuit_activity(actor, input),
        ControllerSpec::Hopper => hopper_activity(actor, input),
        ControllerSpec::Ambush => ambush_activity(actor, input),
    }
}

fn pursuit_activity(actor: &ActorController, input: DecisionInput) -> Activity {
    if input.in_vision || input.in_melee { return Activity::Approach; }
    if input.outside_vision { return Activity::Patrol; }
    actor.activity
}

fn combat_activity(
    actor: &ActorController,
    input: DecisionInput,
    approach: Approach,
    committed: bool,
    defensive: bool,
) -> Activity {
    if approach == Approach::Flying {
        return flying_activity(actor, input);
    }
    if actor.activity == Activity::Attack && committed && !input.attack_done {
        return Activity::Attack;
    }
    if defensive && input.in_melee {
        return Activity::Defense;
    }
    if input.outside_vision {
        return Activity::Patrol;
    }
    if input.in_melee || (defensive && input.outside_melee && input.in_vision) {
        return ground_attack_activity(actor, approach);
    }
    if input.outside_melee && input.in_vision { return Activity::Approach; }
    actor.activity
}

fn ground_attack_activity(actor: &ActorController, approach: Approach) -> Activity {
    if approach == Approach::Roll && actor.activity == Activity::Approach
        && !actor.observations.roll_ready
    {
        return Activity::Approach;
    }
    Activity::Attack
}

fn flying_activity(actor: &ActorController, input: DecisionInput) -> Activity {
    match actor.activity {
        Activity::Attack if input.action_done => Activity::Recover,
        Activity::Attack => Activity::Attack,
        _ if input.outside_vision => Activity::Patrol,
        Activity::Approach if input.in_vision && actor.aligned_for_attack() => Activity::Attack,
        Activity::Recover if actor.recovery_remaining > 0 => Activity::Recover,
        Activity::Recover | Activity::Patrol if input.in_vision => Activity::Approach,
        activity => activity,
    }
}

fn hopper_activity(actor: &ActorController, input: DecisionInput) -> Activity {
    match actor.activity {
        Activity::Idle if !input.grounded => Activity::Fall,
        Activity::Idle if actor.observations.idle_ready => Activity::Jump,
        Activity::Jump | Activity::AirJump if actor.observations.jump_done => Activity::Fall,
        Activity::Fall if actor.observations.air_jump => Activity::AirJump,
        Activity::Fall if input.grounded => Activity::Idle,
        activity => activity,
    }
}

fn ambush_activity(actor: &ActorController, input: DecisionInput) -> Activity {
    match actor.activity {
        Activity::Idle if actor.observations.detected_left || actor.observations.detected_right => Activity::Prepare,
        Activity::Prepare if input.prepare_done => ambush_direction(actor),
        Activity::AttackLeft if input.attack_done => ambush_followup(actor, Activity::EatLeft),
        Activity::AttackRight if input.attack_done => ambush_followup(actor, Activity::EatRight),
        Activity::EatLeft | Activity::EatRight if input.eat_done => Activity::Idle,
        activity => activity,
    }
}

fn ambush_direction(actor: &ActorController) -> Activity {
    if actor.observations.detected_left {
        Activity::AttackLeft
    } else if actor.observations.detected_right {
        Activity::AttackRight
    } else {
        Activity::Idle
    }
}

fn ambush_followup(actor: &ActorController, eating: Activity) -> Activity {
    if actor.observations.killed_target { eating } else { Activity::Idle }
}
