use super::animation_lifecycle::{animation_duration_ticks, FacingSlots};
use super::flying_motor::BatDeath;
use super::*;
use crate::game::actors::definition::{
    EnemyDeathMode, EnemyDeathSetup,
};

#[derive(Component, Default)]
pub struct OpiTombstone;

#[derive(Component)]
pub(crate) struct Decay;

const ENEMY_CORPSE_DESPAWN_RANGE: f32 = 1280.;

#[derive(Resource, Default)]
pub(super) struct CorpseDespawnAnchor {
    last_live_player_pos: Option<Vec2>,
}

impl CorpseDespawnAnchor {
    fn update_live_player_pos(&mut self, player_pos: Option<Vec2>) {
        if let Some(player_pos) = player_pos {
            self.last_live_player_pos = Some(player_pos);
        }
    }

    fn can_despawn_at(&self, corpse_pos: Vec2) -> bool {
        self.last_live_player_pos.is_some_and(|player_pos| {
            corpse_pos.distance(player_pos) > ENEMY_CORPSE_DESPAWN_RANGE
        })
    }
}

#[derive(Component)]
pub(super) struct DeathAnimTimer {
    pub(super) remaining: u32,
}

#[derive(Component, Clone, Debug)]
pub(super) struct DeathPolicy {
    mode: EnemyDeathMode,
    anim_key: Option<String>,
    left_anim_key: Option<String>,
    right_anim_key: Option<String>,
    decay_key: Option<String>,
    fall_collider_half_extents: Option<Vec2>,
    fall_gfx_offset_y: Option<f32>,
}

impl DeathPolicy {
    pub(super) fn from_setup(setup: Option<&EnemyDeathSetup>) -> Self {
        setup.map_or_else(
            || Self {
                mode: EnemyDeathMode::Decay,
                anim_key: Some("Die".to_string()),
                left_anim_key: None,
                right_anim_key: None,
                decay_key: Some("Decay".to_string()),
                fall_collider_half_extents: None,
                fall_gfx_offset_y: None,
            },
            |setup| Self {
                mode: setup.mode,
                anim_key: setup.anim_key.clone(),
                left_anim_key: setup.left_anim_key.clone(),
                right_anim_key: setup.right_anim_key.clone(),
                decay_key: setup.decay_key.clone(),
                fall_collider_half_extents: setup
                    .fall_collider_half_extents
                    .map(Vec2::from_array),
                fall_gfx_offset_y: setup.fall_gfx_offset_y,
            },
        )
    }

    fn fall_collider_half_extents(&self) -> Option<Vec2> {
        self.fall_collider_half_extents
    }

    fn fall_gfx_offset_y(&self) -> Option<f32> {
        self.fall_gfx_offset_y
    }

    fn death_anim_key(&self, facing: Facing) -> Option<&str> {
        match facing {
            Facing::Left => self.left_anim_key.as_deref(),
            Facing::Right => self.right_anim_key.as_deref(),
        }
        .or(self.anim_key.as_deref())
    }
}

#[derive(Bundle)]
struct DeadEnemyRetainBundle {
    transform: Transform,
    children: Children,
    dead: Dead,
    death_timer: DeathAnimTimer,
    enemy: Enemy,
    facing: Facing,
    facing_slots: FacingSlots,
    bat_death: BatDeath,
    tentacle: Tentacle,
    tentacle_state: TentacleState,
    gent: Gent,
    definition: crate::game::actors::ActorDefinition,
    spawn_lease: super::spawn::SpawnDirectorOwned,
    velocity: LinearVelocity,
    visibility: Visibility,
    inherited_visibility: InheritedVisibility,
    view_visibility: ViewVisibility,
    death_policy: DeathPolicy,
    state_despawn_marker: StateDespawnMarker,
}

pub(crate) fn dead(
    mut query: Query<(Entity, &mut Dead, &mut LinearVelocity), With<Enemy>>,
    mut kill_count: ResMut<KillCount>,
    mut commands: Commands,
) {
    for (entity, mut dead, mut velocity) in query.iter_mut() {
        if dead.ticks == 0 {
            **kill_count += 1;
            velocity.0 = Vec2::ZERO;
            commands.entity(entity).retain::<DeadEnemyRetainBundle>();
        }
        dead.ticks += 1;
    }
}

/// A full animation key, or an actor's binding for one.
fn resolve_enemy_anim_key(actor: Option<&crate::game::actors::ActorDefinition>, key: &str) -> Option<String> {
    if key.starts_with("anim.") {
        return Some(key.to_string());
    }
    actor.and_then(|actor| actor.archetype.anim.get(key).cloned())
}

fn transition_enemy_to_decay(commands: &mut Commands, entity: Entity) {
    commands
        .entity(entity)
        .remove::<Enemy>()
        .remove::<Dead>()
        .remove::<DeathAnimTimer>()
        .remove::<Collider>()
        .remove::<ChildOf>()
        .insert(Decay);
}

pub(super) fn enemy_death_animation(
    i_query: Query<
        (
            Entity,
            &Gent,
            &DeathPolicy,
            Option<&Facing>,
            Option<&crate::game::actors::ActorDefinition>,
            Option<&Tentacle>,
        ),
        (Added<Dead>, With<Enemy>),
    >,
    mut gfx_query: Query<&mut ScriptPlayer<SpriteAnimation>, With<EnemyGfx>>,
    mut commands: Commands,
    preloaded: Res<PreloadedAssets>,
    anim_assets: Res<Assets<SpriteAnimation>>,
) {
    for (
        entity,
        gent,
        policy,
        facing,
        actor,
        is_tentacle,
    ) in i_query.iter()
    {
        if is_tentacle.is_some() {
            continue;
        }

        let facing = facing.copied().unwrap_or(Facing::Right);
        let Some(death_key_name) = policy.death_anim_key(facing) else {
            despawn_enemy_with_gfx(&mut commands, entity, gent);
            continue;
        };
        let death_key = resolve_enemy_anim_key(actor, death_key_name);
        let Some(death_key) = death_key else {
            warn!(
                "Enemy death animation key '{death_key_name}' is not registered"
            );
            despawn_enemy_with_gfx(&mut commands, entity, gent);
            continue;
        };

        play_enemy_death_animation(
            gent,
            policy.mode,
            death_key.as_str(),
            &mut gfx_query,
        );

        if policy.mode == EnemyDeathMode::DeathFall {
            commands.entity(entity).insert(BatDeath::new(
                policy.fall_collider_half_extents(),
                policy.fall_gfx_offset_y(),
            ));
        } else {
            let duration = animation_duration_ticks(
                &death_key,
                &preloaded,
                &anim_assets,
                None,
                DEFAULT_DEATH_TICKS,
            );
            commands.entity(entity).insert(DeathAnimTimer {
                remaining: duration,
            });
        }
    }
}

fn play_enemy_death_animation(
    gent: &Gent,
    mode: EnemyDeathMode,
    death_key: &str,
    gfx_query: &mut Query<&mut ScriptPlayer<SpriteAnimation>, With<EnemyGfx>>,
) {
    let Ok(mut enemy) = gfx_query.get_mut(gent.e_gfx) else {
        return;
    };

    if mode == EnemyDeathMode::DeathFall {
        enemy.set_slot("HitGround", false);
        enemy.set_slot("AttackImpact", false);
    }
    enemy.play_key(death_key);
}

pub(super) fn advance_death_animation(
    mut commands: Commands,
    mut query: Query<
        (
            Entity,
            Option<&mut DeathAnimTimer>,
            &DeathPolicy,
            Option<&BatDeath>,
            Option<&Gent>,
            Option<&CorpseLoot>,
        ),
        With<Dead>,
    >,
    corpse_loot_particles: Option<
        Res<crate::graphics::corpse_loot_particles::CorpseLootParticleEffectHandle>,
    >,
) {
    for (
        entity,
        timer,
        policy,
        bat_death,
        gent,
        corpse_loot,
    ) in
        query.iter_mut()
    {
        if advance_death_fall(&mut commands, entity, policy.mode, bat_death) {
            continue;
        }

        let Some(mut timer) = timer else {
            continue;
        };
        if death_timer_finished(&mut timer) {
            finish_enemy_death(
                &mut commands,
                entity,
                policy.mode,
                gent,
                corpse_loot,
                corpse_loot_particles.as_deref(),
            );
        }
    }
}

fn advance_death_fall(
    commands: &mut Commands,
    entity: Entity,
    mode: EnemyDeathMode,
    bat_death: Option<&BatDeath>,
) -> bool {
    if mode != EnemyDeathMode::DeathFall {
        return false;
    }
    let Some(bat_death) = bat_death else {
        return true;
    };
    if !bat_death.has_landed() {
        return true;
    }

    transition_enemy_to_decay(commands, entity);
    true
}

fn death_timer_finished(timer: &mut DeathAnimTimer) -> bool {
    timer.remaining = timer.remaining.saturating_sub(1);
    timer.remaining == 0
}

fn finish_enemy_death(
    commands: &mut Commands,
    entity: Entity,
    mode: EnemyDeathMode,
    gent: Option<&Gent>,
    corpse_loot: Option<&CorpseLoot>,
    corpse_loot_particles: Option<
        &crate::graphics::corpse_loot_particles::CorpseLootParticleEffectHandle,
    >,
) {
    match mode {
        EnemyDeathMode::Despawn => finish_enemy_despawn(commands, entity, gent),
        EnemyDeathMode::Tombstone => finish_enemy_tombstone(
            commands,
            entity,
            gent,
            corpse_loot,
            corpse_loot_particles,
        ),
        EnemyDeathMode::Decay | EnemyDeathMode::DeathFall => {
            transition_enemy_to_decay(commands, entity);
        },
    }
}

fn finish_enemy_despawn(
    commands: &mut Commands,
    entity: Entity,
    gent: Option<&Gent>,
) {
    if let Some(gent) = gent {
        despawn_enemy_with_gfx(commands, entity, gent);
    } else {
        despawn_entity_if_exists(commands, entity);
    }
}

fn finish_enemy_tombstone(
    commands: &mut Commands,
    entity: Entity,
    gent: Option<&Gent>,
    corpse_loot: Option<&CorpseLoot>,
    corpse_loot_particles: Option<
        &crate::graphics::corpse_loot_particles::CorpseLootParticleEffectHandle,
    >,
) {
    let Some(gent) = gent else {
        despawn_entity_if_exists(commands, entity);
        return;
    };

    activate_tombstone_corpse_loot(
        commands,
        gent.e_gfx,
        corpse_loot,
        corpse_loot_particles,
    );
    commands
        .entity(gent.e_gfx)
        .remove::<TransformGfxFromGent>()
        .insert(OpiTombstone);
    despawn_entity_if_exists(commands, gent.e_effects_gfx);
    despawn_entity_if_exists(commands, entity);
}

fn activate_tombstone_corpse_loot(
    commands: &mut Commands,
    visual_entity: Entity,
    corpse_loot: Option<&CorpseLoot>,
    corpse_loot_particles: Option<
        &crate::graphics::corpse_loot_particles::CorpseLootParticleEffectHandle,
    >,
) {
    let Some(corpse_loot) = corpse_loot.copied() else {
        return;
    };

    let activated = activate_corpse_loot(
        commands,
        visual_entity,
        corpse_loot,
        corpse_loot_particles,
    );
    commands.entity(visual_entity).insert(activated);
}

pub(super) fn enemy_decay_animation(
    i_query: Query<
        (
            Entity,
            &Gent,
            Option<&DeathPolicy>,
            Option<&crate::game::actors::ActorDefinition>,
        ),
        (Added<Decay>, With<Gent>),
    >,
    mut gfx_query: Query<&mut ScriptPlayer<SpriteAnimation>, With<EnemyGfx>>,
) {
    for (_entity, gent, policy, actor) in i_query.iter() {
        let decay_key = policy
            .and_then(|policy| policy.decay_key.as_deref())
            .unwrap_or("Decay");
        let Some(decay_key) = resolve_enemy_anim_key(actor, decay_key) else {
            warn!("Enemy decay animation key '{decay_key}' is not registered");
            continue;
        };

        if let Ok(mut enemy) = gfx_query.get_mut(gent.e_gfx) {
            enemy.play_key(decay_key.as_str());
            enemy.set_slot("DecayRate", false);
        }
    }
}

pub(super) fn enemy_decay_visibility(
    decay_query: Query<&Gent, With<Decay>>,
    mut gfx_query: Query<
        (
            &mut ScriptPlayer<SpriteAnimation>,
            &ViewVisibility,
        ),
        With<EnemyGfx>,
    >,
) {
    for gent in decay_query.iter() {
        if let Ok((mut enemy, visible)) = gfx_query.get_mut(gent.e_gfx) {
            let _ = visible; // Decay animation speed is fixed; visibility ignored.
            enemy.set_slot("DecayRate", false);
        }
    }
}

pub(super) fn update_corpse_despawn_anchor(
    mut anchor: ResMut<CorpseDespawnAnchor>,
    player_q: Query<&Transform, (With<Player>, Without<Dead>)>,
) {
    anchor.update_live_player_pos(
        player_q.iter().next().map(|t| t.translation.xy()),
    );
}

pub(super) fn despawn_decay_when_far(
    mut commands: Commands,
    decay_q: Query<(Entity, &Gent, &Transform), With<Decay>>,
    anchor: Res<CorpseDespawnAnchor>,
) {
    for (entity, gent, transform) in decay_q.iter() {
        if anchor.can_despawn_at(transform.translation.xy()) {
            despawn_enemy_with_gfx(&mut commands, entity, gent);
        }
    }
}

pub(super) fn despawn_opi_tombstones_when_far(
    mut commands: Commands,
    corpse_q: Query<(Entity, &Transform), With<OpiTombstone>>,
    anchor: Res<CorpseDespawnAnchor>,
) {
    for (entity, transform) in corpse_q.iter() {
        if anchor.can_despawn_at(transform.translation.xy()) {
            commands.entity(entity).despawn();
        }
    }
}
