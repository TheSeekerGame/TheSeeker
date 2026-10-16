//! Instant item consumption and animation-driven deployment share inventory costs.

use std::sync::Arc;
use bevy::prelude::*;
use theseeker_engine::{animation::{cue_id, CueId, PlaybackState}, assets::animation::SpriteAnimation,
    gent::Gent, physics::LinearVelocity, script::ScriptPlayer, time::GameTickUpdate};
use crate::game::{combat::Health, gentstate::Dead};
use super::super::{foreground::{self, ActionMut, ForegroundAction}, MaterialInventory, Player, PlayerStateSet};
use super::super::material_inventory::{CraftMaterial};

/// Withdrawn stock cannot also be spent by crafting or another skill.
#[derive(Debug)]
pub struct ItemReservation { key: CraftMaterial, amount: u32 }

impl ItemReservation {
    pub fn reserve(inventory: &mut MaterialInventory, material: CraftMaterial, amount: u32) -> Option<Self> {
        let key = material;
        (amount > 0 && inventory.remove_from_backpack(key, amount)).then_some(Self { key, amount })
    }

    pub fn refund(self, inventory: &mut MaterialInventory) { inventory.add_to_backpack(self.key, self.amount); }
    pub fn consume(self) {}
}

/// The same spawn function can be called immediately or at a deployment cue.
pub type ItemSpawner = fn(&mut Commands, Entity, Vec2);
pub type PlacementCheck = fn(&World, Entity, Vec2) -> bool;

#[derive(Debug)]
pub struct DeploymentDefinition {
    pub material: CraftMaterial,
    pub amount: u32,
    pub animation_key: String,
    pub clip: Arc<SpriteAnimation>,
    pub release: String,
    pub spawn: ItemSpawner,
    pub validate: PlacementCheck,
}

impl DeploymentDefinition {
    pub fn validate(&self) -> Result<(), String> {
        if self.amount == 0 { return Err("deployment requires a positive item quantity".into()); }
        if self.animation_key.is_empty() { return Err("deployment requires an animation key".into()); }
        let section = self.clip.playback.ok_or("deployment requires an explicit once clip")?;
        section.validate()?;
        if section.mode != theseeker_engine::assets::animation::ClipMode::Once {
            return Err("deployment playback must complete".into());
        }
        let settings = &self.clip.settings.extended;
        if section.start < settings.frame_min || section.end > settings.frame_max {
            return Err("deployment section lies outside the clip's frames".into());
        }
        if self.release.is_empty() || !self.clip.events.iter().any(|event| {
            let frame = event.on.frame_enter.or(event.on.frame_exit).or_else(||
                event.on.bookmark.as_ref().and_then(|name| self.clip.frame_bookmarks.get(name).copied()));
            event.emit == self.release && (event.on.anim_finished
                || frame.is_some_and(|frame| frame >= section.start && frame <= section.end))
        }) {
            return Err(format!("deployment clip '{}' has no reachable '{}' cue", self.animation_key, self.release));
        }
        Ok(())
    }
}

#[derive(Debug)]
pub struct Deploying {
    pub definition: Arc<DeploymentDefinition>,
    pub position: Vec2,
    pub(crate) reservation: Option<ItemReservation>,
    started: bool,
    release_queued: bool,
    release: CueId,
}

/// Validation and stock reservation happen together at the foreground commit boundary.
#[expect(dead_code, reason = "Animated deployment is designed in docs/architecture/actors.md, but no item uses it yet.")]
pub fn request_deployment(commands: &mut Commands, owner: Entity, definition: Arc<DeploymentDefinition>, position: Vec2) {
    commands.queue(move |world: &mut World| { start_deployment(world, owner, definition, position); });
}

pub fn start_deployment(world: &mut World, owner: Entity, definition: Arc<DeploymentDefinition>, position: Vec2) -> bool {
    if let Err(error) = definition.validate() {
        warn!("Cannot activate item deployment: {error}");
        return false;
    }
    if !position.is_finite() || !can_deploy(world, owner) { return false; }
    if !(definition.validate)(world, owner, position) { return false; }
    let Some(mut inventory) = world.get_mut::<MaterialInventory>(owner) else { return false; };
    let Some(reservation) = ItemReservation::reserve(&mut inventory, definition.material, definition.amount) else { return false; };
    let release = cue_id(&definition.release);
    foreground::start_action(world, owner, Deploying { definition, position,
        reservation: Some(reservation), started: false, release_queued: false, release });
    true
}

fn can_deploy(world: &World, owner: Entity) -> bool {
    if world.get::<Player>(owner).is_none() || world.get::<LinearVelocity>(owner).is_none() { return false; }
    if world.get::<Dead>(owner).is_some() || world.get::<Health>(owner).is_some_and(|health| health.current == 0) { return false; }
    if world.get::<ForegroundAction>(owner).is_some_and(|action| action.state::<super::super::states::Elevator>().is_some()) { return false; }
    world.get::<Gent>(owner).is_some_and(|gent| world.get::<ScriptPlayer<SpriteAnimation>>(gent.e_gfx).is_some())
}

pub struct ItemUsePlugin;
impl Plugin for ItemUsePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(GameTickUpdate, tick_deployment.in_set(PlayerStateSet::Behavior)
            .in_set(super::super::PlayerBehaviorSet::Execution)
            .after(theseeker_engine::animation::AnimationSet::Signals));
    }
}

fn tick_deployment(
    mut query: Query<(Entity, ActionMut<Deploying>, &Gent, &mut LinearVelocity), (With<Player>, Without<Dead>)>,
    mut gfx: Query<&mut ScriptPlayer<SpriteAnimation>>,
    playback: Query<&PlaybackState>,
    mut commands: Commands,
) {
    for (entity, mut deployment, gent, mut velocity) in &mut query {
        velocity.0 = Vec2::ZERO;
        if !deployment.started && !start_deployment_playback(&mut deployment, gent, &mut gfx) {
            deployment.finish(&mut commands, entity);
            continue;
        }
        let Some(events) = deployment.execution().playback(playback.get(gent.e_gfx).ok()) else { continue; };
        let finished = events.finished;
        let release = events.ordered.iter().any(|event| event.playback == events.playback
            && matches!(event.kind, theseeker_engine::animation::PlaybackEventKind::Cue(cue) if cue == deployment.release));
        if release && !deployment.release_queued {
            deployment.release_queued = true;
            let execution = deployment.execution().id;
            commands.queue(move |world: &mut World| release_deployment(world, entity, execution));
        }
        if finished { deployment.finish(&mut commands, entity); }
    }
}

fn start_deployment_playback(
    deployment: &mut foreground::ActionMutItem<'_, '_, Deploying>,
    gent: &Gent,
    gfx: &mut Query<&mut ScriptPlayer<SpriteAnimation>>,
) -> bool {
    let Ok(mut player) = gfx.get_mut(gent.e_gfx) else { return false; };
    let definition = deployment.definition.clone();
    player.acquire_presentation(deployment.execution().id);
    player.play_snapshot(&definition.animation_key, definition.clip.clone(), None);
    deployment.execution_mut().playback = theseeker_engine::animation::PlaybackId(player.execution_id());
    deployment.started = true;
    true
}

fn release_deployment(world: &mut World, owner: Entity, execution: u64) {
    if !can_deploy(world, owner) { return; }
    let Some(action) = world.get::<ForegroundAction>(owner).filter(|action| action.id == execution) else { return; };
    let Some(deployment) = action.state::<Deploying>() else { return; };
    let definition = deployment.definition.clone();
    let position = deployment.position;
    if !(definition.validate)(world, owner, position) {
        foreground::start_action(world, owner, super::super::states::Ready);
        return;
    }
    let reservation = world.get_mut::<ForegroundAction>(owner)
        .and_then(|mut action| action.state_mut::<Deploying>().and_then(|deployment| deployment.reservation.take()));
    let Some(reservation) = reservation else { return; };
    reservation.consume();
    (definition.spawn)(&mut world.commands(), owner, position);
}

#[cfg(test)]
mod tests;
