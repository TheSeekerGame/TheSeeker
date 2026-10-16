use crate::game::combat::{self, DamageInfo, DamageSource, Health};
use crate::game::gentstate::Dead;
use crate::game::player::{
    InputBuffer, Player, RecoveryCollisionWorld, SafeGroundedPosition,
    foreground::transition_action,
    skills::portable_tent::{PendingTentEntry, TentInteriorOccupant},
    states::{Idle, Ready, transition_locomotion},
    PlayerAction,
};
use crate::game::world_objects::{CapsuleDropPlatformState, CapsuleOccupant};
use crate::levelgen::{CompiledLevel, SpawnKind};
use crate::prelude::*;
use bevy::ecs::{lifecycle::HookContext, world::DeferredWorld};
use leafwing_input_manager::action_state::ActionState;
use theseeker_engine::gent::Gent;
use theseeker_engine::physics::{CollisionGroups, Group, LinearVelocity, Collider, groups};
use theseeker_engine::presentation::PresentationJump;

pub const SPIKE_DAMAGE: f32 = 20.0;
pub const SPIKE_RESPAWN_DELAY_TICKS: u32 = 96;

#[derive(Component)]
pub struct SpikeHazard;

#[derive(Component, Default, Debug)]
pub struct SpikeDamageCooldown {
    pub remaining_ticks: u32,
}

impl SpikeDamageCooldown {
    pub const DURATION_TICKS: u32 = 48;

    pub fn reset(&mut self) {
        self.remaining_ticks = Self::DURATION_TICKS;
    }

    pub fn activate() -> Self {
        Self { remaining_ticks: Self::DURATION_TICKS }
    }
}

#[derive(Component, Clone, Copy, Debug)]
#[component(on_discard = restore_after_recovery)]
pub struct SpikeRespawnPending {
    pub ticks_remaining: u32,
    pub respawn_destination: Vec2,
    pub last_position: Vec2,
    pub last_z: f32,
    pub original_collision_groups: CollisionGroups,
}

pub struct SpikeHazardPlugin;

impl Plugin for SpikeHazardPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(GameTickUpdate, (
            prepare_spike_contacts.before(combat::damage_source::determine_damage_targets),
            suppress_recovery_input.after(crate::game::player::PlayerStateSet::Input)
                .before(crate::game::player::PlayerStateSet::Behavior),
            process_spike_hits.after(combat::damage_source::apply_damage),
            advance_spike_respawns.after(process_spike_hits)
                .after(crate::game::player::PlayerStateSet::Collisions),
        ));
    }
}

fn suppress_recovery_input(
    mut players: Query<(&mut ActionState<PlayerAction>, &mut InputBuffer,
        &mut crate::game::player::states::HorizontalInputIntent), With<SpikeRespawnPending>>,
) {
    for (mut input, mut buffer, mut intent) in &mut players {
        input.reset_all();
        buffer.clear_all();
        *intent = default();
    }
}

/// Persistent hazards retry contact; the player's cooldown owns repeat-hit timing.
fn prepare_spike_contacts(
    mut cooldowns: Query<(Entity, Option<&mut SpikeDamageCooldown>), With<Player>>,
    mut hazards: Query<&mut DamageSource, With<SpikeHazard>>,
    mut commands: Commands,
) {
    for (entity, cooldown) in &mut cooldowns {
        if let Some(mut cooldown) = cooldown {
            cooldown.remaining_ticks = cooldown.remaining_ticks.saturating_sub(1);
        } else {
            // Initialize before damage application so adjacent spikes share one immunity window.
            commands.entity(entity).insert(SpikeDamageCooldown::default());
        }
    }
    for mut hazard in &mut hazards { hazard.damaged_set.clear(); }
}

pub(crate) fn process_spike_hits(
    mut damage_events: MessageReader<DamageInfo>,
    spike_sources: Query<(), With<SpikeHazard>>,
    mut players: Query<(Entity, &Gent, &Transform, &mut LinearVelocity,
        Option<&SafeGroundedPosition>, &Health, Option<&mut CollisionGroups>,
        Option<&TentInteriorOccupant>, Option<&mut InputBuffer>),
        (With<Player>, Without<Dead>, Without<SpikeRespawnPending>)>,
    compiled_level: Option<Res<CompiledLevel>>,
    mut commands: Commands,
) {
    let mut processed_targets = HashSet::new();
    for damage in damage_events.read() {
        if damage.amount <= 0.0 || !spike_sources.contains(damage.source)
            || !processed_targets.insert(damage.target) { continue; }
        let Ok((entity, gent, transform, mut velocity, checkpoint, health,
            collision, occupant, buffer)) = players.get_mut(damage.target) else { continue; };
        if health.current == 0 { continue; }
        let current = transform.translation.truncate();
        let destination = checkpoint
            .filter(|checkpoint| checkpoint.space == occupant.map(|occupant| occupant.kind))
            .map(|checkpoint| checkpoint.position)
            .or_else(|| compiled_level.as_deref().filter(|_| occupant.is_none()).and_then(|level| level.spawns.iter()
                .find(|spawn| spawn.kind == SpawnKind::Player).map(|spawn| spawn.position)))
            .unwrap_or(current);
        let original_collision_groups = recovery_collision_groups(collision.as_deref().copied());
        if let Some(mut collision) = collision { *collision = CollisionGroups::new(Group::NONE, Group::NONE); }
        velocity.0 = Vec2::ZERO;
        if let Some(mut buffer) = buffer { buffer.clear_all(); }
        transition_action(&mut commands, entity, Ready);
        commands.entity(entity)
            .remove::<(crate::game::physics::Knockback, PendingTentEntry,
                theseeker_engine::physics::inside::PlayerInsideEnemy)>()
            .insert((SpikeRespawnPending {
                ticks_remaining: SPIKE_RESPAWN_DELAY_TICKS,
                respawn_destination: destination,
                last_position: current,
                last_z: transform.translation.z,
                original_collision_groups,
            }, CapsuleDropPlatformState::default()));
        set_gent_visibility(&mut commands, gent, false);
    }
}

fn recovery_collision_groups(collision: Option<CollisionGroups>) -> CollisionGroups {
    let collision = collision.unwrap_or_else(groups::player_body);
    if collision.memberships().intersects(theseeker_engine::physics::ENEMY_INSIDE) {
        CollisionGroups::new(theseeker_engine::physics::PLAYER, collision.filters())
    } else { collision }
}

fn advance_spike_respawns(
    mut players: Query<(Entity, &Gent, &mut SpikeRespawnPending, &mut Transform,
        &mut LinearVelocity, &Collider, &Health, Option<&TentInteriorOccupant>,
        &CapsuleDropPlatformState, Option<&mut InputBuffer>), With<Player>>,
    world: RecoveryCollisionWorld,
    time: Res<GameTime>,
    mut commands: Commands,
) {
    for (entity, gent, mut pending, mut transform, mut velocity, collider,
        health, occupant, platforms, buffer) in &mut players {
        if health.current == 0 {
            commands.entity(entity).remove::<SpikeRespawnPending>();
            continue;
        }
        pending.ticks_remaining = pending.ticks_remaining.saturating_sub(1);
        transform.translation = pending.last_position.extend(pending.last_z);
        velocity.0 = Vec2::ZERO;
        if let Some(mut buffer) = buffer { buffer.clear_all(); }
        if pending.ticks_remaining > 0 { continue; }
        let Some(destination) = world.supported_destination(pending.respawn_destination,
            collider, occupant, platforms) else { continue; };
        transform.translation = destination.extend(pending.last_z);
        commands.entity(entity).insert((
            SafeGroundedPosition { position: destination, space: occupant.map(|occupant| occupant.kind) },
            PresentationJump(time.tick()), CapsuleDropPlatformState::default(),
        )).remove::<SpikeRespawnPending>();
        if let Some(capsule) = world.capsule_occupant(destination, occupant) {
            commands.entity(entity).insert(capsule);
        } else {
            commands.entity(entity).remove::<CapsuleOccupant>();
        }
        transition_locomotion(&mut commands, entity, Idle);
        set_gent_visibility(&mut commands, gent, true);
    }
}

/// Cancellation, death, and despawn all release the collision and visibility override.
fn restore_after_recovery(mut world: DeferredWorld, context: HookContext) {
    let Some(pending) = world.get::<SpikeRespawnPending>(context.entity).copied() else { return; };
    let graphics = world.get::<Gent>(context.entity).map(|gent| [gent.e_gfx, gent.e_effects_gfx]);
    world.commands().queue(move |world: &mut World| {
        if world.get::<SpikeRespawnPending>(context.entity).is_some() { return; }
        if let Ok(mut player) = world.get_entity_mut(context.entity) {
            player.insert(pending.original_collision_groups);
        }
        restore_graphics_visibility(world, graphics);
    });
}

fn restore_graphics_visibility(world: &mut World, graphics: Option<[Entity; 2]>) {
    for gfx in graphics.into_iter().flatten() {
        if let Ok(mut gfx) = world.get_entity_mut(gfx) { gfx.insert(Visibility::Visible); }
    }
}

fn set_gent_visibility(commands: &mut Commands, gent: &Gent, visible: bool) {
    let visibility = if visible { Visibility::Visible } else { Visibility::Hidden };
    commands.entity(gent.e_gfx).try_insert(visibility);
    commands.entity(gent.e_effects_gfx).try_insert(visibility);
}

#[cfg(test)]
mod tests;
