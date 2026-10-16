//! The player's single foreground execution, typed mechanic views, and scoped cleanup.

use std::{marker::PhantomData, ops::{Deref, DerefMut}};
use bevy::{ecs::{lifecycle::HookContext, query::QueryData, world::DeferredWorld}, prelude::*};
use theseeker_engine::{animation::{PlaybackId, PlaybackState}, assets::animation::SpriteAnimation,
    effects::GhostingSource, gent::{Gent, TransformGfxFromGent}, script::ScriptPlayer};
use super::skills::item_use::Deploying;
use super::{states::*, skills::{channeled::ChanneledSkill, types::SkillId}};
use crate::game::{actors::action::{next_execution_id, ActionControls}, gentstate::Dead};

#[derive(Debug)]
pub enum ActionRuntime {
    Ready(Ready), Elevator(Elevator), Gathering(Gathering), Attacking(Attacking),
    Dashing(Dashing), Whirling(Whirling), BurningDashing(BurningDashing),
    FlickerStriking(FlickerStriking), DashStrike(DashStrike), Spitting(Spitting), Deploying(Deploying),
}

/// Tags are query indexes; all mechanic data belongs to `ForegroundAction`.
#[derive(Component)]
#[component(storage = "SparseSet")]
pub struct ActionTag<T: ActionState>(PhantomData<T>);

pub trait ActionState: Send + Sync + std::fmt::Debug + 'static {
    fn into_runtime(self) -> ActionRuntime;
    fn get(runtime: &ActionRuntime) -> Option<&Self>;
    fn get_mut(runtime: &mut ActionRuntime) -> Option<&mut Self>;
}

macro_rules! action_states {
    ($($ty:ident),+ $(,)?) => {
        $(impl ActionState for $ty {
            fn into_runtime(self) -> ActionRuntime { ActionRuntime::$ty(self) }
            fn get(runtime: &ActionRuntime) -> Option<&Self> {
                if let ActionRuntime::$ty(state) = runtime { Some(state) } else { None }
            }
            fn get_mut(runtime: &mut ActionRuntime) -> Option<&mut Self> {
                if let ActionRuntime::$ty(state) = runtime { Some(state) } else { None }
            }
        })+
        fn remove_action_tags(entity: &mut EntityWorldMut) {
            entity.remove::<($(ActionTag<$ty>,)+)>();
        }
    };
}
action_states!(Ready, Elevator, Gathering, Attacking, Dashing, Whirling,
    BurningDashing, FlickerStriking, DashStrike, Spitting, Deploying);

#[derive(Component, Debug)]
#[component(on_discard = cleanup_execution)]
pub struct ForegroundAction {
    pub id: u64,
    pub playback: PlaybackId,
    pub controls: ActionControls,
    runtime: ActionRuntime,
    owned_entities: Vec<Entity>,
}

impl ForegroundAction {
    pub fn state<T: ActionState>(&self) -> Option<&T> { T::get(&self.runtime) }
    pub fn state_mut<T: ActionState>(&mut self) -> Option<&mut T> { T::get_mut(&mut self.runtime) }
    fn shared_action(&self) -> Option<&crate::game::actors::action::ActiveAction> {
        match &self.runtime {
            ActionRuntime::Spitting(state) => state.action.as_ref(),
            ActionRuntime::DashStrike(state) => state.action.as_ref(),
            _ => None,
        }
    }

    pub fn playback_id(&self) -> PlaybackId {
        self.shared_action().map_or(self.playback, |action| action.playback)
    }

    pub fn owns_motion(&self) -> bool {
        let controls = self.shared_action().map_or(self.controls, |action| action.definition.controls);
        controls.motion_x || controls.motion_y
    }
    pub fn own(&mut self, entity: Entity) { self.owned_entities.push(entity); }

    pub fn play(&mut self, player: &mut ScriptPlayer<SpriteAnimation>, key: &str) {
        player.acquire_presentation(self.id);
        player.play_key(key);
        self.playback = PlaybackId(player.execution_id());
    }

    pub fn playback<'a>(&self, events: Option<&'a PlaybackState>) -> Option<&'a PlaybackState> {
        events.filter(|events| events.playback == self.playback_id())
    }
}

#[derive(QueryData)]
pub struct ActionRef<T: ActionState> {
    execution_data: &'static ForegroundAction,
    tag: &'static ActionTag<T>,
}

#[derive(QueryData)]
#[query_data(mutable)]
pub struct ActionMut<T: ActionState> {
    execution_data: &'static mut ForegroundAction,
    tag: &'static ActionTag<T>,
}

impl<T: ActionState> Deref for ActionRefItem<'_, '_, T> {
    type Target = T;
    fn deref(&self) -> &T { self.execution_data.state().expect("foreground tag matches runtime") }
}
impl<T: ActionState> Deref for ActionMutItem<'_, '_, T> {
    type Target = T;
    fn deref(&self) -> &T { self.execution_data.state().expect("foreground tag matches runtime") }
}
impl<T: ActionState> DerefMut for ActionMutItem<'_, '_, T> {
    fn deref_mut(&mut self) -> &mut T { self.execution_data.state_mut().expect("foreground tag matches runtime") }
}
impl<T: ActionState> Deref for ActionMutReadOnlyItem<'_, '_, T> {
    type Target = T;
    fn deref(&self) -> &T { self.execution_data.state().expect("foreground tag matches runtime") }
}
impl<T: ActionState> ActionRefItem<'_, '_, T> {
    pub fn finish(&self, commands: &mut Commands, entity: Entity) { finish_action(commands, entity, self.execution_data.id); }
}
impl<T: ActionState> ActionMutItem<'_, '_, T> {
    pub fn execution(&self) -> &ForegroundAction { &self.execution_data }
    pub fn execution_mut(&mut self) -> &mut ForegroundAction { &mut self.execution_data }
    pub fn finish(&self, commands: &mut Commands, entity: Entity) { finish_action(commands, entity, self.execution_data.id); }
}

/// Commits one replacement; mechanic values cannot be inserted as ECS components.
pub fn transition_action<T: ActionState>(commands: &mut Commands, entity: Entity, state: T) {
    commands.queue(move |world: &mut World| start_action(world, entity, state));
}

pub fn finish_action(commands: &mut Commands, entity: Entity, execution: u64) {
    commands.queue(move |world: &mut World| {
        if world.get::<ForegroundAction>(entity).is_some_and(|action| action.id == execution) {
            start_action(world, entity, Ready);
        }
    });
}

pub fn start_action<T: ActionState>(world: &mut World, entity: Entity, state: T) {
    if world.get::<Dead>(entity).is_some()
        || world.get::<crate::game::combat::Health>(entity).is_some_and(|health| health.current == 0) { return; }
    let runtime = state.into_runtime();
    if world.get::<crate::game::spikes::SpikeRespawnPending>(entity).is_some()
        && !matches!(runtime, ActionRuntime::Ready(_)) { return; }
    let Ok(mut actor) = world.get_entity_mut(entity) else { return; };
    if let Some(old) = actor.take::<ForegroundAction>() {
        if adds_in_air(&old.runtime) { actor.remove::<InAir>(); }
    }
    remove_action_tags(&mut actor);
    actor.remove::<(GhostingSource, ChanneledSkill,
        super::skills::flicker_strike::FlickerOutroTransition)>();
    let controls = controls(&runtime);
    insert_effects(&mut actor, &runtime);
    actor.insert((ForegroundAction { id: next_execution_id(), playback: PlaybackId::default(),
        controls, runtime, owned_entities: Vec::new() }, ActionTag::<T>(PhantomData)));
}

fn adds_in_air(runtime: &ActionRuntime) -> bool {
    matches!(runtime, ActionRuntime::Dashing(_) | ActionRuntime::DashStrike(_)
        | ActionRuntime::BurningDashing(_) | ActionRuntime::FlickerStriking(_))
}

fn controls(runtime: &ActionRuntime) -> ActionControls {
    let motion = !matches!(runtime, ActionRuntime::Ready(_) | ActionRuntime::Attacking(_) | ActionRuntime::Whirling(_));
    ActionControls { motion_x: motion, motion_y: motion, facing: false,
        presentation: !matches!(runtime, ActionRuntime::Ready(_)) }
}

fn insert_effects(actor: &mut EntityWorldMut, runtime: &ActionRuntime) {
    let channel = match runtime {
        ActionRuntime::Whirling(_) => Some(SkillId::Whirl),
        ActionRuntime::BurningDashing(_) => Some(SkillId::BurningDash),
        ActionRuntime::FlickerStriking(_) => Some(SkillId::FlickerStrike),
        _ => None,
    };
    if let Some(skill) = channel { actor.insert(ChanneledSkill::new(skill)); }
    let ghost = match runtime {
        ActionRuntime::Dashing(dash) => Some((1, (dash.max_ticks * 5).max(29), 0.5)),
        ActionRuntime::DashStrike(_) => Some((1, 80, 0.5)),
        ActionRuntime::BurningDashing(_) => Some((8, 16, 0.5)),
        ActionRuntime::FlickerStriking(_) => Some((0, 100, 0.5)),
        _ => None,
    };
    if let Some((interval, lifetime, alpha)) = ghost {
        let stealth = actor.contains::<crate::game::effects::stealthed::StealthEffect>();
        let alpha = if stealth && matches!(runtime, ActionRuntime::Dashing(_) | ActionRuntime::DashStrike(_)) { 0.2 } else { alpha };
        actor.insert((InAir, GhostingSource { spawn_interval_ticks: interval,
            ghost_lifetime_ticks: lifetime, initial_alpha: alpha, ..Default::default() }));
    }
}

fn cleanup_execution(mut world: DeferredWorld, context: HookContext) {
    let Some(mut action) = world.get_mut::<ForegroundAction>(context.entity) else { return; };
    let id = action.id;
    let remove_in_air = adds_in_air(&action.runtime);
    let mut owned = std::mem::take(&mut action.owned_entities);
    let shared = match &mut action.runtime {
        ActionRuntime::Spitting(state) => state.action.take(),
        ActionRuntime::DashStrike(state) => state.action.take(),
        ActionRuntime::Whirling(state) => { owned.extend(state.damage_entity.take()); None },
        ActionRuntime::BurningDashing(state) => { owned.extend(state.damage_entity.take()); None },
        _ => None,
    };
    let reservation = match &mut action.runtime { ActionRuntime::Deploying(deployment) => deployment.reservation.take(), _ => None };
    let burning = matches!(action.runtime, ActionRuntime::BurningDashing(_));
    if let Some(reservation) = reservation {
        if let Some(mut inventory) = world.get_mut::<super::MaterialInventory>(context.entity) { reservation.refund(&mut inventory); }
    }
    let gfx = world.get::<Gent>(context.entity).map(|gent| gent.e_gfx);
    if let Some(mut shared) = shared {
        if shared.outcome.is_none() { shared.interrupt(&mut world.commands()); } else { shared.cleanup(&mut world.commands()); }
    }
    for entity in owned { world.commands().entity(entity).try_despawn(); }
    let dead = world.get::<Dead>(context.entity).is_some();
    if let Some(gfx) = gfx { release_presentation(&mut world, gfx, id, dead); }
    if burning { start_burning_cooldown(&mut world, context.entity); }
    world.commands().queue(move |world: &mut World| {
        let Ok(mut actor) = world.get_entity_mut(context.entity) else { return; };
        if actor.contains::<ForegroundAction>() { return; }
        remove_action_tags(&mut actor);
        actor.remove::<(GhostingSource, ChanneledSkill, super::skills::flicker_strike::FlickerOutroTransition)>();
        if remove_in_air { actor.remove::<InAir>(); }
        actor.world_scope(|world| start_action(world, context.entity, Ready));
    });
}

fn release_presentation(world: &mut DeferredWorld, gfx: Entity, id: u64, dead: bool) {
    let released = world.get_mut::<ScriptPlayer<SpriteAnimation>>(gfx).is_some_and(|mut player| {
        if dead { player.revoke_presentation(id) } else { player.release_presentation(id) }
    });
    if released {
        let base = world.get::<super::PlayerGfx>(gfx).map(|player| player.base_offset);
        if let Some(mut offset) = world.get_mut::<TransformGfxFromGent>(gfx) { offset.offset = base; }
    }
}

pub(crate) fn interrupt_stopped_playback(
    actions: Query<(Entity, &ForegroundAction, &Gent)>,
    playback: Query<&PlaybackState>,
    mut commands: Commands,
) {
    for (entity, action, gent) in &actions {
        let Ok(events) = playback.get(gent.e_gfx) else { continue; };
        if events.ordered.iter().any(|event| event.playback == action.playback_id()
            && matches!(event.kind, theseeker_engine::animation::PlaybackEventKind::Interrupted)) {
            finish_action(&mut commands, entity, action.id);
        }
    }
}

pub(crate) fn interrupt_on_death(
    trigger: On<Add, Dead>,
    actions: Query<(), With<ForegroundAction>>,
    mut commands: Commands,
) {
    if actions.contains(trigger.entity) {
        commands.entity(trigger.entity).remove::<ForegroundAction>();
    }
}

fn start_burning_cooldown(world: &mut DeferredWorld, entity: Entity) {
    let tick = world.get_resource::<theseeker_engine::time::GameTime>().map_or(0, theseeker_engine::time::GameTime::tick);
    if let Some(mut cooldowns) = world.get_resource_mut::<super::skills::cooldowns::Cooldowns>() {
        cooldowns.start(entity, SkillId::BurningDash, super::skills::types::burning_dash_metadata().delayed_cooldown, 1.0, tick);
    }
}

impl<T: ActionState> Copy for ActionRefItem<'_, '_, T> {}
impl<T: ActionState> Clone for ActionRefItem<'_, '_, T> { fn clone(&self) -> Self { *self } }

#[cfg(test)]
mod tests;
