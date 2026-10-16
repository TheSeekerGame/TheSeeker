use theseeker_engine::effects::scale_recovery::ScaleRecovery;
use crate::game::player::foreground::ActionMut;
use std::cmp::Ordering;

use crate::game::combat::effects::DamageFlash;
use crate::game::player::spawns::item_drops::{
    enemy_drop_material, spawn_material_drop_burst, DropAssetHandles,
    DropLaunchMode, ENEMY_CORPSE_LOOT_PROBABILITY,
};
use crate::game::player::states::{transition_action, Gathering};
use crate::game::player::{
    CraftMaterial, Grounded, Player, PlayerAction, PlayerStateSet,
};
use crate::game::spikes::SpikeHazard;
use crate::graphics::corpse_loot_particles::{
    spawn_corpse_loot_particle_emitter, CorpseLootParticleEffectHandle,
};
use crate::prelude::*;
use leafwing_input_manager::action_state::ActionState;
use rand::Rng;
use theseeker_engine::gent::Gent;
use theseeker_engine::physics::{
    groups, LinearVelocity, PhysicsWorld,
};
use theseeker_engine::time::GameTickUpdate;

use super::flying_motor::BatDeath;
use super::{Decay, Enemy, EnemyStateSet, OpiTombstone};

const CORPSE_LOOT_HITS_REQUIRED: u32 = 3;
const CORPSE_GATHER_RANGE: f32 = 22.0;
const CORPSE_DROP_PICKUP_DELAY_TICKS: u32 = 48;
const CORPSE_DROP_SPAWN_Y_OFFSET: f32 = 10.0;
const CORPSE_STRETCH_TICKS: u32 = 8;
const CORPSE_MAX_SQUEEZE_X: f32 = 0.7;
const CORPSE_MAX_STRETCH_Y: f32 = 1.3;

pub struct CorpseLootPlugin;

#[derive(Component, Debug, Clone, Copy)]
pub(crate) struct CorpseLoot {
    pub material: CraftMaterial,
    pub drop_count: u32,
    pub hits_remaining: u32,
    pub visual_entity: Entity,
    pub particle_entity: Option<Entity>,
}

#[derive(Component, Debug, Clone, Copy)]
struct CorpseLootStretch(ScaleRecovery);

impl Plugin for CorpseLootPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            GameTickUpdate,
            (
                attach_corpse_loot_on_death.after(super::dead),
                activate_decay_corpse_loot,
                animate_corpse_loot_stretch
                    .run_if(any_with_component::<CorpseLootStretch>),
            )
                .in_set(EnemyStateSet::Animation)
                .run_if(in_state(AppState::InGame)),
        );
        app.add_systems(
            GameTickUpdate,
            gather_nearby_corpse_loot
                .after(PlayerStateSet::Behavior)
                .run_if(in_state(AppState::InGame)),
        );
        app.add_systems(
            GameTickUpdate,
            release_corpse_loot_on_spike_contact
                .after(super::bat_death_fall)
                .in_set(EnemyStateSet::Collisions)
                .run_if(in_state(AppState::InGame)),
        );
    }
}

pub(crate) fn activate_corpse_loot(
    commands: &mut Commands,
    visual_entity: Entity,
    mut corpse_loot: CorpseLoot,
    particles: Option<&CorpseLootParticleEffectHandle>,
) -> CorpseLoot {
    corpse_loot.visual_entity = visual_entity;
    if corpse_loot.particle_entity.is_none() {
        corpse_loot.particle_entity = particles.map(|particles| {
            spawn_corpse_loot_particle_emitter(
                commands,
                visual_entity,
                &particles.0,
            )
        });
    }
    corpse_loot
}

fn attach_corpse_loot_on_death(
    mut commands: Commands,
    enemy_q: Query<
        (
            Entity,
            &Gent,
            Option<&crate::game::actors::ActorDefinition>,
        ),
        (With<Enemy>, Added<crate::game::gentstate::Dead>),
    >,
) {
    let mut rng = rand::rng();

    for (entity, gent, actor) in enemy_q.iter() {
        let Some(arch_id) = actor.map(|actor| actor.archetype.id.as_str()) else {
            continue;
        };

        let Some(material) = enemy_drop_material(arch_id) else {
            continue;
        };

        if rng.random::<f32>() > ENEMY_CORPSE_LOOT_PROBABILITY {
            continue;
        }

        commands.entity(entity).insert(CorpseLoot {
            material,
            drop_count: 1,
            hits_remaining: CORPSE_LOOT_HITS_REQUIRED,
            visual_entity: gent.e_gfx,
            particle_entity: None,
        });
    }
}

fn activate_decay_corpse_loot(
    mut commands: Commands,
    mut query: Query<(&Gent, &mut CorpseLoot), (Added<Decay>, With<CorpseLoot>)>,
    particles: Option<Res<CorpseLootParticleEffectHandle>>,
) {
    for (gent, mut corpse_loot) in query.iter_mut() {
        *corpse_loot = activate_corpse_loot(
            &mut commands,
            gent.e_gfx,
            *corpse_loot,
            particles.as_deref(),
        );
    }
}

fn apply_corpse_loot_stretch(
    entity: Entity, transform: &mut Transform, existing: Option<&CorpseLootStretch>, commands: &mut Commands,
) {
    let recovery = ScaleRecovery::start(transform, existing.map(|stretch| stretch.0),
        Vec2::new(CORPSE_MAX_SQUEEZE_X, CORPSE_MAX_STRETCH_Y));
    commands.entity(entity).insert(CorpseLootStretch(recovery));
}

fn animate_corpse_loot_stretch(
    mut commands: Commands, mut query: Query<(Entity, &mut Transform, &mut CorpseLootStretch)>,
) {
    for (entity, mut transform, mut stretch) in &mut query {
        if stretch.0.tick(&mut transform, CORPSE_STRETCH_TICKS,
            Vec2::new(CORPSE_MAX_SQUEEZE_X, CORPSE_MAX_STRETCH_Y)) {
            commands.entity(entity).remove::<CorpseLootStretch>();
        }
    }
}

fn gather_nearby_corpse_loot(
    mut commands: Commands,
    mut player_q: Query<
        (
            Entity,
            &ActionState<PlayerAction>,
            &GlobalTransform,
            Has<Grounded>,
            Option<ActionMut<Gathering>>,
            &mut LinearVelocity,
        ),
        With<Player>,
    >,
    mut corpse_loot_q: ParamSet<(
        Query<(Entity, &CorpseLoot), Or<(With<Decay>, With<OpiTombstone>)>>,
        Query<&mut CorpseLoot>,
    )>,
    visual_positions: Query<&GlobalTransform>,
    mut visual_q: Query<
        (
            &GlobalTransform,
            &mut Transform,
            Option<&CorpseLootStretch>,
        ),
        With<Sprite>,
    >,
    drop_assets: Option<Res<DropAssetHandles>>,
) {
    let Ok((
        player_entity,
        actions,
        player_transform,
        is_grounded,
        maybe_gathering,
        mut velocity,
    )) = player_q.single_mut()
    else {
        return;
    };
    if !actions.just_pressed(&PlayerAction::Interact) {
        return;
    }
    if !is_grounded {
        return;
    }

    let player_pos = player_transform.translation().truncate();
    let target_entity = {
        let corpse_candidates = corpse_loot_q.p0();
        let Some(target_entity) = corpse_candidates
            .iter()
            .filter_map(|candidate| corpse_gather_candidate(
                candidate,
                player_pos,
                &visual_positions,
            ))
            .min_by(|(_, a), (_, b)| {
                a.partial_cmp(b).unwrap_or(Ordering::Equal)
            })
            .map(|(entity, _)| entity)
        else {
            return;
        };
        target_entity
    };

    let mut corpse_loot_access = corpse_loot_q.p1();
    let Ok(mut corpse_loot) = corpse_loot_access.get_mut(target_entity) else {
        return;
    };
    let visual_entity = corpse_loot.visual_entity;
    let Ok((visual_transform, mut local_transform, stretch)) =
        visual_q.get_mut(visual_entity)
    else {
        return;
    };

    commands.entity(visual_entity).insert(DamageFlash {
        current_ticks: 0,
        max_ticks: 8,
    });
    apply_corpse_loot_stretch(
        visual_entity,
        &mut local_transform,
        stretch,
        &mut commands,
    );

    velocity.0 = Vec2::ZERO;
    if let Some(mut gathering) = maybe_gathering {
        gathering.tick = 0;
    } else {
        transition_action(&mut commands, player_entity, Gathering::default());
    }

    corpse_loot.hits_remaining = corpse_loot.hits_remaining.saturating_sub(1);
    if corpse_loot.hits_remaining > 0 {
        return;
    }

    release_corpse_loot(
        &mut commands,
        target_entity,
        &mut corpse_loot,
        visual_transform.translation().truncate(),
        drop_assets.as_deref(),
    );
}

fn corpse_gather_candidate(
    candidate: (Entity, &CorpseLoot),
    player_pos: Vec2,
    visual_positions: &Query<&GlobalTransform>,
) -> Option<(Entity, f32)> {
    let (entity, corpse_loot) = candidate;
    corpse_loot_distance_squared(corpse_loot, player_pos, visual_positions)
        .map(|dist_sq| (entity, dist_sq))
}

pub(crate) fn corpse_loot_distance_squared(
    corpse_loot: &CorpseLoot,
    player_pos: Vec2,
    visual_positions: &Query<&GlobalTransform>,
) -> Option<f32> {
    let visual_transform = visual_positions.get(corpse_loot.visual_entity).ok()?;
    let dist_sq = visual_transform
        .translation()
        .truncate()
        .distance_squared(player_pos);
    (dist_sq <= CORPSE_GATHER_RANGE * CORPSE_GATHER_RANGE)
        .then_some(dist_sq)
}

fn release_corpse_loot_on_spike_contact(
    mut commands: Commands,
    mut corpse_q: Query<
        (Entity, &Transform, &BatDeath, &mut CorpseLoot),
        With<crate::game::gentstate::Dead>,
    >,
    spike_q: Query<(), With<SpikeHazard>>,
    visual_positions: Query<&GlobalTransform>,
    drop_assets: Option<Res<DropAssetHandles>>,
    spatial_query: PhysicsWorld,
) {
    for (entity, transform, death_fall, mut corpse_loot) in corpse_q.iter_mut() {
        let position = transform.translation.xy();
        let collider = death_fall.probe_collider();
        let touches_spike = spatial_query
            .intersect(
                position,
                &collider,
                groups::player_body(),
                Some(entity),
            )
            .into_iter()
            .any(|hit| spike_q.get(hit).is_ok());
        if !touches_spike {
            continue;
        }

        let visual_position = visual_positions
            .get(corpse_loot.visual_entity)
            .map(|transform| transform.translation().truncate())
            .unwrap_or(position);
        release_corpse_loot(
            &mut commands,
            entity,
            &mut corpse_loot,
            visual_position,
            drop_assets.as_deref(),
        );
    }
}

fn release_corpse_loot(
    commands: &mut Commands,
    corpse_entity: Entity,
    corpse_loot: &mut CorpseLoot,
    visual_position: Vec2,
    drop_assets: Option<&DropAssetHandles>,
) {
    let drop_count = std::mem::take(&mut corpse_loot.drop_count);
    if drop_count == 0 {
        return;
    }

    if let Some(drop_assets) = drop_assets {
        spawn_material_drop_burst(
            commands,
            drop_assets,
            corpse_loot.material,
            drop_count,
            visual_position + Vec2::Y * CORPSE_DROP_SPAWN_Y_OFFSET,
            DropLaunchMode::Random,
            CORPSE_DROP_PICKUP_DELAY_TICKS,
        );
    }

    if let Some(particle_entity) = corpse_loot.particle_entity.take() {
        commands.entity(particle_entity).despawn();
    }
    commands.entity(corpse_entity).remove::<CorpseLoot>();
}
