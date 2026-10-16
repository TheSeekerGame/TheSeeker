//! Shared action execution and typed actor decisions.

pub mod action;
pub mod catalog;
pub mod controller;
pub mod hopper;
pub mod definition;
pub mod observations;
pub mod motion;
pub mod lunge;
pub mod projectile;
pub(crate) mod population_probe;
pub mod sensors;
pub mod tick_strike;

pub use controller::ActorController;
pub use definition::EnemyArchetype;
pub use observations::*;

use std::sync::Arc;

use bevy::prelude::*;
use bevy_common_assets::toml::TomlAssetPlugin;
use theseeker_engine::animation::PlaybackState;
use theseeker_engine::assets::AssetsSet;
use theseeker_engine::time::GameTickUpdate;

use crate::appstate::AppState;
use crate::game::gentstate::Dead;
use controller::{decide, Activity, DecisionInput};

#[derive(Resource, Default)]
pub struct LevelSeed(pub u32);

/// The definition an actor spawned from. Its body and lifecycle stay fixed while its controller
/// adopts newer actions.
#[derive(Component, Deref, Clone)]
pub struct ActorDefinition(pub Arc<catalog::ResolvedActor>);

#[derive(Component, Default)]
#[component(on_discard = action::cleanup_action_slot::<ActorAction>)]
pub struct ActorAction(pub Option<action::ActiveAction>, pub action::ActionEffects);

pub struct ActorPlugin;

impl Plugin for ActorPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<LevelSeed>();
        app.init_resource::<catalog::ActorCatalog>();
        app.add_plugins(population_probe::PopulationProbePlugin);
        app.add_plugins((
            TomlAssetPlugin::<catalog::AuthoredActor>::new(&["arch.toml"]),
            TomlAssetPlugin::<action::ActionDefinition>::new(&["action.toml"]),
        ));
        app.add_systems(OnExit(AppState::AssetsLoading), catalog::resolve_catalog
            .after(AssetsSet::ResolveKeys).before(AssetsSet::Colliders));
        app.add_systems(Update, catalog::resolve_catalog.run_if(in_state(AppState::InGame)));
        app.add_systems(GameTickUpdate, (advance_actor_actions, actor_decisions).chain().in_set(crate::game::enemy::EnemyStateSet::Decisions));
    }
}

#[derive(Bundle)]
pub struct ActorBundle {
    pub controller: ActorController,
    pub action: ActorAction,
    pub target_sensor: TargetSensor,
    pub ground_sensor: GroundSensor,
    pub range_sensor: RangeSensor,
    pub turn_cooldown: TurnCooldown,
    pub definition: ActorDefinition,
    pub health_sensor: HealthSensor,
    pub perceived_target_sensor: PerceivedTargetSensor,
    pub perceived_ground_sensor: PerceivedGroundSensor,
    pub perceived_range_sensor: PerceivedRangeSensor,
}

impl ActorBundle {
    pub fn new(definition: Arc<catalog::ResolvedActor>, entity: Entity, level_seed: u32) -> Self {
        let bits = entity.to_bits();
        let seed = ((bits ^ (bits >> 32)) as u32) ^ level_seed;
        Self {
            controller: ActorController::new(definition.clone(), seed),
            action: ActorAction::default(), definition: ActorDefinition(definition),
            target_sensor: TargetSensor::default(), ground_sensor: GroundSensor::default(),
            range_sensor: RangeSensor::default(), turn_cooldown: TurnCooldown::default(),
            health_sensor: HealthSensor::default(), perceived_target_sensor: PerceivedTargetSensor::default(),
            perceived_ground_sensor: PerceivedGroundSensor::default(), perceived_range_sensor: PerceivedRangeSensor::default(),
        }
    }
}

pub(super) fn advance_actor_actions(
    mut actors: Query<(&mut ActorAction, Option<&PlaybackState>), (Without<AiPaused>, Without<Dead>)>,
    time: Res<theseeker_engine::time::GameTime>,
) {
    for (mut action, signals) in &mut actors {
        action.1 = action.0.as_mut().map(|active| active.advance(time.tick(), signals, false)).unwrap_or_default();
    }
}

pub(super) fn actor_decisions(
    mut query: Query<(
        &mut ActorController, &ActorAction, &PerceivedTargetSensor, &PerceivedGroundSensor,
        &HealthSensor, Option<&PlaybackState>, Option<&DistanceJitter>, Option<&PatrolTimeScale>,
    ), (Without<AiPaused>, Without<Dead>)>,
) {
    for (mut actor, action, target, ground, health, signals, jitter, scale) in &mut query {
        actor.advance_decision_clocks();
        let input = decision_input(&actor, action, target, ground, health, signals, jitter);
        let activity = decide(&actor, input);
        if activity == Activity::Recover && actor.activity != activity {
            actor.begin_recovery(action.0.as_ref().map(|action| action.definition.timing.recovery_ticks).unwrap_or(0));
        }
        actor.transition(activity);
        if activity == Activity::Patrol {
            actor.tick_patrol(scale.map(|scale| scale.0).unwrap_or(1.0));
        }
        actor.elapsed = actor.elapsed.saturating_add(1);
    }
}

fn decision_input(
    actor: &ActorController,
    action: &ActorAction,
    target: &PerceivedTargetSensor,
    ground: &PerceivedGroundSensor,
    health: &HealthSensor,
    signals: Option<&PlaybackState>,
    jitter: Option<&DistanceJitter>,
) -> DecisionInput {
    let stats = actor.definition.stats();
    let melee = stats.melee_range * jitter.map(|jitter| jitter.melee).unwrap_or(1.0);
    let vision = stats.vision_range * jitter.map(|jitter| jitter.vision).unwrap_or(1.0);
    let playback = action.0.as_ref().map(|active| active.playback).unwrap_or(actor.playback);
    let cue = |name| signals.is_some_and(|signals| signals.has_cue_for(playback, theseeker_engine::animation::cue_id(name)));
    DecisionInput {
        in_vision: target.entity.is_some() && target.dist2 < vision * vision,
        in_melee: target.entity.is_some() && target.dist2 < melee * melee,
        outside_vision: target.entity.is_none() || target.dist2 > vision * vision,
        outside_melee: target.entity.is_none() || target.dist2 > melee * melee,
        grounded: ground.on, dead: health.zero,
        action_done: action.0.as_ref().is_some_and(|action| action.outcome.is_some()),
        attack_done: cue("attack_done"), prepare_done: cue("prepare_done"), eat_done: cue("eat_done"),
    }
}

#[cfg(test)]
pub(crate) mod fixtures;

impl action::ActionSlot for ActorAction {
    fn take_action(&mut self) -> Option<action::ActiveAction> { self.0.take() }
}

#[cfg(test)]
mod measurements;
