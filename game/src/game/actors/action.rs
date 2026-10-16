//! Actor-independent action progress and immutable mechanic definitions.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use theseeker_engine::animation::{cue_id, PlaybackState, PlaybackEventKind, PlaybackId};
use theseeker_engine::assets::{animation::SpriteAnimation, attack::AttackDescriptor, projectile::ProjectileDescriptor};

#[derive(Asset, TypePath, Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ActionDefinition {
    pub id: String,
    pub base: Option<String>,
    pub mechanic: Option<Mechanic>,
    pub presentation: Option<PresentationBinding>,
    pub timing: Option<ActionTiming>,
    pub damage: Option<f32>,
    pub motion: Option<LungeMotion>,
    pub controls: Option<ActionControls>,
    pub hits: Option<super::lunge::LungeHits>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Mechanic {
    Projectile { descriptor: String },
    Contact { descriptor: String },
    Lunge,
    Guard,
    Jump,
    TentacleStrike { phases: super::tick_strike::TickStrike },
    TrapStrike,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PresentationBinding {
    pub animation: String,
    pub section: Option<String>,
    pub impact_section: Option<String>,
    #[serde(default)]
    pub offset: [f32; 2],
    pub facing_slots: Option<super::definition::EnemyFacingSlotsSetup>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ActionTiming {
    pub release: Option<String>,
    pub cancel: Option<String>,
    pub complete: Option<String>,
    pub duration_ticks: Option<u32>,
    /// Zero repeats continuously until the controller interrupts the action.
    #[serde(default)]
    pub repetitions: u16,
    #[serde(default)]
    pub recovery_ticks: u32,
}

#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ActionControls {
    #[serde(default)]
    pub motion_x: bool,
    #[serde(default)]
    pub motion_y: bool,
    #[serde(default)]
    pub facing: bool,
    #[serde(default)]
    pub presentation: bool,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct LungeMotion {
    pub speed: f32,
    pub direction: Option<[f32; 2]>,
    pub impact_ticks: Option<u32>,
}

/// Displacement in world pixels for one simulation tick.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct TickDisplacement(pub Vec2);

/// Facing selected by a foreground execution, independent of live input intent.
#[derive(Component)]
pub struct ActionFacing {
    pub owner: u64,
    pub forward: f32,
}

impl ActionFacing {
    pub fn facing(&self) -> crate::game::gentstate::Facing {
        use crate::game::gentstate::Facing;
        if self.forward > 0.0 { Facing::Right } else { Facing::Left }
    }
}

impl LungeMotion {
    pub fn displacement(self, target_direction: Vec2) -> TickDisplacement {
        let direction = self.direction.map(|direction| Vec2::new(direction[0] * target_direction.x.signum(), direction[1]))
            .unwrap_or_else(|| target_direction.normalize_or_zero());
        TickDisplacement(direction * self.speed)
    }
}

/// Output axes claimed by one action execution. Unclaimed axes retain locomotion.
#[derive(Debug, Clone, Copy)]
pub struct MotionIntent {
    pub owner: u64,
    pub controls: ActionControls,
    pub displacement: TickDisplacement,
}

impl MotionIntent {
    pub fn apply(self, owner: u64, velocity: &mut theseeker_engine::physics::LinearVelocity) {
        if self.owner != owner { return; }
        if self.controls.motion_x { velocity.0.x = self.displacement.0.x; }
        if self.controls.motion_y { velocity.0.y = self.displacement.0.y; }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum ActionPresentation { Execute, Impact }

pub struct PresentationRequest<'a> {
    pub owner: u64,
    pub binding: &'a PresentationBinding,
    pub clip: Arc<SpriteAnimation>,
    pub phase: ActionPresentation,
    pub forward: f32,
}

impl PresentationRequest<'_> {
    pub fn commit(self, player: &mut theseeker_engine::script::ScriptPlayer<SpriteAnimation>) -> PlaybackId {
        let section = match self.phase {
            ActionPresentation::Execute => self.binding.section.as_deref(),
            ActionPresentation::Impact => self.binding.impact_section.as_deref(),
        };
        player.acquire_presentation(self.owner);
        if let Some(slots) = &self.binding.facing_slots {
            player.set_presentation_slot(self.owner, &slots.right, self.forward > 0.0);
            player.set_presentation_slot(self.owner, &slots.left, self.forward <= 0.0);
        }
        player.play_snapshot(&self.binding.animation, self.clip, section);
        PlaybackId(player.execution_id())
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct ActionCues {
    pub release: Option<theseeker_engine::animation::CueId>,
    pub cancel: Option<theseeker_engine::animation::CueId>,
    pub complete: Option<theseeker_engine::animation::CueId>,
}

impl From<&ActionTiming> for ActionCues {
    fn from(timing: &ActionTiming) -> Self {
        Self {
            release: timing.release.as_deref().map(cue_id),
            cancel: timing.cancel.as_deref().map(cue_id),
            complete: timing.complete.as_deref().map(cue_id),
        }
    }
}

pub struct ResolvedAction {
    pub id: String,
    pub mechanic: Mechanic,
    pub presentation: Option<PresentationBinding>,
    pub clip: Option<Arc<SpriteAnimation>>,
    pub timing: ActionTiming,
    pub cues: ActionCues,
    pub damage: Option<f32>,
    pub motion: Option<LungeMotion>,
    pub controls: ActionControls,
    pub hits: Option<super::lunge::LungeHits>,
    pub projectile: Option<ProjectileDescriptor>,
    pub projectile_clips: Option<ProjectileClips>,
    pub attack: Option<AttackDescriptor>,
}

#[derive(Clone)]
pub struct ProjectileClips {
    pub idle: Arc<SpriteAnimation>,
    pub pop: Arc<SpriteAnimation>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActionOutcome {
    Completed,
    Interrupted,
}

#[derive(Default)]
pub struct ActionEffects {
    pub releases: usize,
    pub restart_clip: bool,
    pub outcome: Option<ActionOutcome>,
}

#[derive(Component)]
pub struct ActionRequest {
    pub actor: Entity,
    pub forward: f32,
    pub definition: Arc<ResolvedAction>,
    pub target: Option<Entity>,
    pub damage: f32,
}

impl ActionRequest {
    pub fn activate(self, playback: PlaybackId) -> ActiveAction {
        let mut action = ActiveAction::new(self.definition, playback);
        action.owner = Some(self.actor);
        action.forward = self.forward;
        action.target = self.target;
        action.damage = self.damage;
        action
    }
}

#[derive(Component)]
pub struct ActiveAction {
    pub id: u64,
    pub owner: Option<Entity>,
    pub forward: f32,
    pub target: Option<Entity>,
    pub damage: f32,
    pub lunge: Option<super::lunge::LungeRuntime>,
    pub definition: Arc<ResolvedAction>,
    pub playback: PlaybackId,
    pub age: u32,
    pub cancel_open: bool,
    pub owned_entities: Vec<Entity>,
    pub outcome: Option<ActionOutcome>,
    cycles: u16,
    releases: usize,
    last_tick: Option<u64>,
}

impl ActiveAction {
    pub fn new(definition: Arc<ResolvedAction>, playback: PlaybackId) -> Self {
        Self {
            id: next_execution_id(),
            lunge: None, owner: None, forward: 1.0, target: None, damage: definition.damage.unwrap_or(0.0),
            definition, playback, age: 0, cancel_open: false,
            owned_entities: Vec::new(), outcome: None,
            cycles: 0, releases: 0, last_tick: None,
        }
    }

    pub fn motion_intent(&self, displacement: TickDisplacement) -> MotionIntent {
        MotionIntent { owner: self.id, controls: self.definition.controls, displacement }
    }

    pub fn claim_facing(&self, commands: &mut Commands) {
        if !self.definition.controls.facing { return; }
        if let Some(owner) = self.owner {
            commands.entity(owner).insert(ActionFacing { owner: self.id, forward: self.forward });
        }
    }

    pub fn presentation_request(&self, phase: ActionPresentation) -> Option<PresentationRequest<'_>> {
        if !self.definition.controls.presentation { return None; }
        Some(PresentationRequest { owner: self.id, phase, forward: self.forward,
            binding: self.definition.presentation.as_ref()?, clip: self.definition.clip.clone()? })
    }

    pub fn present(&mut self, player: &mut theseeker_engine::script::ScriptPlayer<SpriteAnimation>, phase: ActionPresentation) {
        if let Some(request) = self.presentation_request(phase) {
            self.playback = request.commit(player);
        }
    }

    pub fn advance(&mut self, tick: u64, signals: Option<&PlaybackState>, cancel: bool) -> ActionEffects {
        if self.last_tick == Some(tick) || self.outcome.is_some() {
            return ActionEffects::default();
        }
        self.last_tick = Some(tick);
        self.age = self.age.saturating_add(1);
        let mut effects = self.consume_signals(signals);
        let impacted = self.lunge.as_ref().is_some_and(|lunge| lunge.contact.is_some());
        let timed_out = !impacted && self.definition.timing.duration_ticks.is_some_and(|duration| self.age >= duration);
        let impact_done = self.lunge.as_mut().is_some_and(|lunge| lunge.tick_impact(self.definition.motion.and_then(|motion| motion.impact_ticks)));
        if cancel && self.cancel_open {
            effects.outcome = Some(ActionOutcome::Interrupted);
        } else if timed_out || impact_done {
            effects.outcome = Some(ActionOutcome::Completed);
        }
        if effects.outcome == Some(ActionOutcome::Interrupted) {
            effects.releases = 0;
            effects.restart_clip = false;
        }
        self.outcome = effects.outcome;
        effects
    }

    fn consume_signals(&mut self, signals: Option<&PlaybackState>) -> ActionEffects {
        let mut effects = ActionEffects::default();
        let Some(signals) = signals else { return effects; };
        let playback = self.playback;
        for event in signals.ordered.iter().filter(|event| event.playback == playback) {
            self.consume_event(&event.kind, &mut effects);
        }
        effects
    }

    fn consume_event(&mut self, event: &PlaybackEventKind, effects: &mut ActionEffects) {
        match event {
            PlaybackEventKind::Cue(id) => {
                self.consume_cue(*id, effects);
            },
            PlaybackEventKind::Completed => self.complete_cycle(effects),
            PlaybackEventKind::Interrupted => effects.outcome = Some(ActionOutcome::Interrupted),
            _ => {},
        }
    }

    fn consume_cue(&mut self, cue: theseeker_engine::animation::CueId, effects: &mut ActionEffects) {
        let timing = &self.definition.timing;
        if self.definition.cues.release == Some(cue)
            && (timing.repetitions == 0 || self.releases < timing.repetitions as usize)
        {
            self.releases += 1;
            effects.releases += 1;
        }
        self.cancel_open |= self.definition.cues.cancel == Some(cue);
        if self.definition.cues.complete == Some(cue) {
            self.complete_cycle(effects);
        }
    }

    fn complete_cycle(&mut self, effects: &mut ActionEffects) {
        if effects.restart_clip || effects.outcome.is_some() { return; }
        self.cycles = self.cycles.saturating_add(1);
        let repetitions = self.definition.timing.repetitions;
        if repetitions > 0 && self.cycles >= repetitions {
            effects.outcome = Some(ActionOutcome::Completed);
        } else {
            effects.restart_clip = true;
        }
    }

    pub fn interrupt(&mut self, commands: &mut Commands) {
        self.outcome = Some(ActionOutcome::Interrupted);
        self.cleanup(commands);
    }

    pub fn cleanup(&mut self, commands: &mut Commands) {
        for entity in self.owned_entities.drain(..) {
            commands.entity(entity).try_despawn();
        }
        if let Some(owner) = self.owner {
            let execution = self.id;
            commands.queue(move |world: &mut World| release_action_facing(world, owner, execution));
        }
    }
}

fn release_action_facing(world: &mut World, owner: Entity, execution: u64) {
    if world.get::<ActionFacing>(owner).is_some_and(|facing| facing.owner == execution) {
        world.entity_mut(owner).remove::<ActionFacing>();
    }
}

impl std::fmt::Debug for ActiveAction {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.debug_struct("ActiveAction").field("id", &self.id)
            .field("definition", &self.definition.id).field("age", &self.age)
            .field("outcome", &self.outcome).finish()
    }
}

/// Component storage for the actor's single foreground execution.
pub trait ActionSlot {
    fn take_action(&mut self) -> Option<ActiveAction>;
}

pub fn cleanup_action_slot<T: Component<Mutability = bevy::ecs::component::Mutable> + ActionSlot>(
    mut world: bevy::ecs::world::DeferredWorld,
    context: bevy::ecs::lifecycle::HookContext,
) {
    let Some(mut action) = world.get_mut::<T>(context.entity).and_then(|mut slot| slot.take_action()) else { return; };
    action.interrupt(&mut world.commands());
    let Some(gfx) = world.get::<theseeker_engine::gent::Gent>(context.entity).map(|gent| gent.e_gfx) else { return; };
    let dead = world.get::<crate::game::gentstate::Dead>(context.entity).is_some();
    let released = world.get_mut::<theseeker_engine::script::ScriptPlayer<SpriteAnimation>>(gfx)
        .is_some_and(|mut player| if dead { player.revoke_presentation(action.id) } else { player.release_presentation(action.id) });
    if released {
        if let Some(mut offset) = world.get_mut::<theseeker_engine::gent::TransformGfxFromGent>(gfx) { offset.offset = None; }
    }
}

pub(crate) fn next_execution_id() -> u64 {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}
