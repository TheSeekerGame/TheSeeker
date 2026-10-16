use super::{DamageInfo, DamageWindow, Health, Invulnerable};
use crate::game::effects::chilled::ChilledEffect;
use crate::game::enemy::Enemy;
use crate::game::gentstate::{Dead, Facing};
use crate::game::physics::Knockback;
use crate::game::player::crits::Crits;
use crate::game::player::passives::{
    PassiveContext, PassiveEvent, PassiveOwner, PassiveOwnerItem,
};
use crate::game::player::skills::types::SkillId;
use crate::game::player::{Passives, PlayerStatMod};
use crate::game::spikes::{SpikeDamageCooldown, SpikeHazard};
use theseeker_engine::profile::TickProfiler;
use crate::levelgen::breakable_wall::BreakableWall;
use crate::prelude::*;
use bevy::ecs::system::Command;
use bevy::ecs::system::ParamSet;
use std::mem;
use theseeker_engine::gent::Gent;
use theseeker_engine::physics::{
    Collider, CollisionGroups, PhysicsWorld, Sensor, ENEMY, PLAYER_ATTACK,
};

/// Component representing an entity that can deal damage (attacks, projectiles, etc.)
#[derive(Component, Debug)]
pub struct DamageSource {
    pub current_lifetime: u32,
    pub max_lifetime: u32,
    pub damage: f32,
    /// Base damage used as the reset point before per-tick/target modifications
    pub base_damage: f32,
    pub max_targets: u32,
    pub owner: Entity,
    pub damaged_set: HashSet<Entity>,
    /// Entities in contact this tick that are valid damage targets
    pub target_set: HashSet<Entity>,
    pub chilled_effect: Option<ChilledEffect>,
}

impl DamageSource {
    pub fn new(lifetime: u32, owner: Entity, damage: f32) -> Self {
        DamageSource {
            current_lifetime: 0,
            max_lifetime: lifetime,
            damage,
            base_damage: damage,
            max_targets: 3,
            owner,
            damaged_set: HashSet::default(),
            target_set: HashSet::default(),
            chilled_effect: None,
        }
    }

    pub fn with_chilled_effect(mut self, effect: ChilledEffect) -> Self {
        self.chilled_effect = Some(effect);
        self
    }

    pub fn with_max_targets(mut self, max_targets: u32) -> Self {
        self.max_targets = max_targets;
        self
    }
}

/// Knockback applied to damaged targets
#[derive(Component, Default, Deref)]
pub struct Pushback(pub Knockback);

/// Knockback applied to the damage source owner
#[derive(Component, Default, Deref)]
pub struct SelfPushback(pub Knockback);

/// Marks damage from a stealthed entity; damage systems interpret this (e.g., doubling)
#[derive(Component)]
pub struct Stealthed;

#[derive(Component)]
pub struct Hit;

#[derive(Component)]
pub struct Backstab;

#[derive(Component, Debug, Clone, Copy, Deref)]
pub struct BackstabMultiplier(pub f32);

/// Tags a damage source with the originating skill to provide richer passive events.
#[derive(Component, Debug, Clone, Copy)]
pub struct DamageSourceSkill(pub SkillId);

/// Allows the damage source to hit its owner (skip owner exclusion during target queries).
#[derive(Component, Debug, Clone, Copy)]
pub struct AllowOwnerHit;

/// Despawns owner-bound damage when owner is missing or dead.
#[derive(Component, Debug, Clone, Copy)]
pub struct DespawnOnOwnerInvalid;

/// Marks a downward-oriented attack (for pogo mechanics, etc.)
#[derive(Component)]
pub struct DownwardAttack;

/// Inserts a bundle only if the entity still exists when applied.
struct InsertIfExists<T: bevy::ecs::bundle::Bundle + 'static> {
    entity: Entity,
    bundle: T,
}

impl<T: bevy::ecs::bundle::Bundle + 'static> InsertIfExists<T> {
    fn new(entity: Entity, bundle: T) -> Self {
        Self { entity, bundle }
    }
}

impl<T: bevy::ecs::bundle::Bundle + 'static> Command for InsertIfExists<T> {
    type Out = ();
    fn apply(self, world: &mut World) {
        if let Ok(mut e) = world.get_entity_mut(self.entity) {
            e.insert(self.bundle);
        }
    }
}

pub(crate) fn tag_damage_source_sensors(
    mut commands: Commands,
    query: Query<Entity, Added<DamageSource>>,
) {
    for entity in &query {
        commands.entity(entity).insert(Sensor);
    }
}

pub fn determine_damage_targets(
    mut damage_query: Query<(
        Entity,
        &GlobalTransform,
        &mut DamageSource,
        &Collider,
        Option<&CollisionGroups>,
        Option<&AllowOwnerHit>,
    )>,
    damageable_query: Query<
        (
            &GlobalTransform,
            &Collider,
            Option<&DamageWindow>,
        ),
        (
            With<Collider>,
            With<Health>,
            With<Gent>,
            Without<Dead>,
            Without<crate::game::spikes::SpikeRespawnPending>,
        ),
    >,
    breakable_query: Query<(&GlobalTransform, &Collider), With<BreakableWall>>,
    spatial_query: PhysicsWorld,
    mut profiler: TickProfiler,
) {
    let _profile = profiler.scope("damage_targets");
    for (
        _entity,
        transform,
        mut damage_source,
        collider,
        maybe_groups,
        allow_owner_hit,
    ) in damage_query.iter_mut()
    {
        let collision_groups = maybe_groups.copied().unwrap_or(
            CollisionGroups::new(PLAYER_ATTACK, ENEMY),
        );

        let intersections = spatial_query.intersect(
            transform.translation().xy(),
            collider,
            collision_groups,
            if allow_owner_hit.is_some() {
                None
            } else {
                Some(damage_source.owner) // Exclude owner, not damage source
            },
        );

        let mut targets = intersections
            .into_iter()
            .filter_map(|colliding_entity| {
                damage_target_distance(
                    colliding_entity,
                    transform.translation().xy(),
                    &damageable_query,
                    &breakable_query,
                )
            })
            .collect::<Vec<_>>();
        targets.sort_by(|(_, dist1), (_, dist2)| dist1.total_cmp(dist2));

        let valid_targets: Vec<_> = {
            let max_allowed = damage_source.max_targets as usize;
            let already_damaged = damage_source.damaged_set.len();
            let remaining = max_allowed.saturating_sub(already_damaged);
            targets.iter().take(remaining).map(|(e, _)| e).collect()
        };

        for entity in valid_targets {
            insert_new_damage_target(&mut damage_source, entity);
        }
    }
}

fn damage_target_distance(
    entity: Entity,
    source_pos: Vec2,
    damageable_query: &Query<
        (
            &GlobalTransform,
            &Collider,
            Option<&DamageWindow>,
        ),
        (
            With<Collider>,
            With<Health>,
            With<Gent>,
            Without<Dead>,
            Without<crate::game::spikes::SpikeRespawnPending>,
        ),
    >,
    breakable_query: &Query<
        (&GlobalTransform, &Collider),
        With<BreakableWall>,
    >,
) -> Option<(Entity, f32)> {
    if let Ok((transform, _collider, damage_window)) =
        damageable_query.get(entity)
    {
        return damageable_target_distance(entity, source_pos, transform, damage_window);
    }

    breakable_query
        .get(entity)
        .ok()
        .map(|(transform, _)| (entity, target_distance(source_pos, transform)))
}

fn damageable_target_distance(
    entity: Entity,
    source_pos: Vec2,
    transform: &GlobalTransform,
    damage_window: Option<&DamageWindow>,
) -> Option<(Entity, f32)> {
    if damage_window.map(|window| !window.open).unwrap_or(false) {
        return None;
    }

    Some((entity, target_distance(source_pos, transform)))
}

fn target_distance(source_pos: Vec2, transform: &GlobalTransform) -> f32 {
    transform.translation().xy().distance_squared(source_pos)
}

fn insert_new_damage_target(
    damage_source: &mut DamageSource,
    entity: &Entity,
) {
    if damage_source.damaged_set.contains(entity) {
        return;
    }

    damage_source.target_set.insert(*entity);
}

pub fn apply_damage_modifications(
    mut damage_query: Query<
        (Entity, &mut DamageSource, Has<Hit>),
        Without<Gent>,
    >,
    owner_query: Query<(&Passives, Option<&Health>, PassiveOwner), With<Gent>>,
    mut commands: Commands,
    mut profiler: TickProfiler,
) {
    let _profile = profiler.scope("damage_modifiers");
    for (entity, mut damage_source, has_hit) in damage_query.iter_mut() {
        if damage_source.target_set.is_empty() {
            continue;
        }

        // Reset damage to base to prevent compounding on persistent entities (like whirling)
        damage_source.damage = damage_source.base_damage;

        if let Ok((passives, health, owner)) = owner_query.get(damage_source.owner) {
            let mut modifiers = crate::game::player::passives::DamageModifiers::default();
            apply_passive_damage_modifiers(passives, &mut modifiers, &owner.context(health));
            queue_backstab_marker(entity, &modifiers, &mut commands);
            apply_damage_multiplier(
                &mut damage_source.damage,
                modifiers.damage_multiplier,
            );
        }
        if !has_hit {
            // Use a safe insert that checks existence at apply time to avoid
            // panics when a despawn was queued earlier in the tick for this entity.
            commands.queue(InsertIfExists::new(entity, Hit));
        }
    }
}

fn apply_passive_damage_modifiers(
    passives: &Passives,
    modifiers: &mut crate::game::player::passives::DamageModifiers,
    context: &PassiveContext<'_>,
) {
    for passive in crate::game::player::passives::ordered_passives(passives).into_iter().flatten() {
        passive.modify_damage(modifiers, context);
    }
}

fn queue_backstab_marker(
    entity: Entity,
    modifiers: &crate::game::player::passives::DamageModifiers,
    commands: &mut Commands,
) {
    if !modifiers.can_backstab {
        return;
    }

    commands.queue(InsertIfExists::new(
        entity,
        (
            Backstab,
            BackstabMultiplier(modifiers.backstab_multiplier),
        ),
    ));
}

fn apply_damage_multiplier(damage: &mut f32, multiplier: f32) {
    if multiplier != 1.0 {
        *damage *= multiplier;
    }
}

#[derive(Default)]
struct TargetDamageSnapshot {
    position: Vec2,
    health_pct: Option<f32>,
}

impl TargetDamageSnapshot {
    fn new(health: &Health, transform: &GlobalTransform) -> Self {
        Self {
            position: transform.translation().truncate(),
            health_pct: target_health_pct(health),
        }
    }
}

fn target_health_pct(health: &Health) -> Option<f32> {
    if health.max == 0 {
        return None;
    }

    Some(health.current as f32 / health.max as f32)
}

fn is_enemy_near_target(
    enemy: Entity,
    target: Entity,
    enemy_transform: &Transform,
    target_pos: Vec2,
    radius: f32,
) -> bool {
    enemy != target
        && enemy_transform.translation.truncate().distance(target_pos) <= radius
}

fn target_damage_snapshot(
    target: Entity,
    target_query: &mut Query<
        (
            Entity,
            &mut Health,
            &GlobalTransform,
            &Facing,
            Option<&PlayerStatMod>,
            Has<Invulnerable>,
            Has<crate::game::player::Player>,
            Option<&mut SpikeDamageCooldown>,
        ),
        With<Gent>,
    >,
) -> TargetDamageSnapshot {
    target_query
        .get(target)
        .ok()
        .map(|(_, health, transform, _, _, _, _, _)| TargetDamageSnapshot::new(health, transform))
        .unwrap_or_default()
}

fn enemy_near_target_candidate(
    candidate: &(Entity, &Transform),
    target: Entity,
    target_pos: Vec2,
    radius: f32,
) -> bool {
    let (enemy, enemy_transform) = *candidate;
    is_enemy_near_target(enemy, target, enemy_transform, target_pos, radius)
}

fn has_shadow_cloak(passives: Option<&Passives>) -> bool {
    passives
        .map(|passives| {
            passives.contains(&crate::game::player::Passive::ShadowCloak)
        })
        .unwrap_or(false)
}

fn apply_crit_damage(
    maybe_crits: Option<Mut<'_, Crits>>,
    damage: &mut f32,
    is_critical: &mut bool,
    damage_entity: Entity,
    source_skill: Option<&DamageSourceSkill>,
    owner: Entity,
    passive_events: &mut MessageWriter<PassiveEvent>,
) {
    let Some(mut crits) = maybe_crits else {
        return;
    };

    crits.hit_count += 1;
    if crits.next_hit_is_critical {
        *is_critical = true;
        *damage *= crits.crit_damage_multiplier;
        crits.next_hit_is_critical = false;
        passive_events.write(PassiveEvent::CriticalHit {
            damage_source: damage_entity,
            source_skill: source_skill.map(|tag| tag.0),
        });
    }

    passive_events.write(PassiveEvent::HitCountAdvanced {
        owner,
        hit_count: crits.hit_count,
    });
}

type DamageTargetItem<'a> = (
    Entity,
    Mut<'a, Health>,
    &'a GlobalTransform,
    &'a Facing,
    Option<&'a PlayerStatMod>,
    bool,
    bool,
    Option<Mut<'a, SpikeDamageCooldown>>,
);

type OwnerDamageItem<'w, 's> = (
    Option<Mut<'w, Crits>>,
    Option<&'w Passives>,
    Option<&'w Health>,
    PassiveOwnerItem<'w, 's>,
);

struct DamageApplication<'a> {
    owner: Entity,
    source: Entity,
    source_transform: &'a GlobalTransform,
    is_stealthed: bool,
    can_backstab: bool,
    backstab_multiplier: Option<&'a BackstabMultiplier>,
    pushback: Option<&'a Pushback>,
    chilled_effect: Option<&'a ChilledEffect>,
    is_spike_hazard: bool,
    owner_has_shadow_cloak: bool,
    is_critical: bool,
}

struct OwnerDamageAdjustment<'a, 's> {
    owner: Option<OwnerDamageItem<'a, 's>>,
    damage_entity: Entity,
    source_skill: Option<&'a DamageSourceSkill>,
    owner_entity: Entity,
    target_snapshot: &'a TargetDamageSnapshot,
    enemies_near_target: u32,
}

fn apply_owner_damage_adjustments(
    adjustment: OwnerDamageAdjustment<'_, '_>,
    damage: &mut f32,
    is_critical: &mut bool,
    passive_events: &mut MessageWriter<PassiveEvent>,
) -> bool {
    let Some((crits, passives, health, owner)) = adjustment.owner else {
        return false;
    };

    let owner_has_shadow_cloak = has_shadow_cloak(passives);
    apply_crit_damage(
        crits,
        damage,
        is_critical,
        adjustment.damage_entity,
        adjustment.source_skill,
        adjustment.owner_entity,
        passive_events,
    );
    if let Some(passives) = passives {
        let target = adjustment.target_snapshot;
        let context = owner.context(health).at_target(
            target.position,
            target.health_pct,
            adjustment.enemies_near_target,
        );
        let mut modifiers = crate::game::player::passives::DamageModifiers::default();
        apply_passive_damage_modifiers(passives, &mut modifiers, &context);
        apply_damage_multiplier(damage, modifiers.damage_multiplier);
    }
    owner_has_shadow_cloak
}

fn apply_gent_target_if_present(
    target: Option<DamageTargetItem<'_>>,
    damage: f32,
    application: &DamageApplication<'_>,
    damaged_set: &mut HashSet<Entity>,
    damage_events: &mut MessageWriter<DamageInfo>,
    commands: &mut Commands,
) -> bool {
    let Some(target) = target else {
        return false;
    };

    apply_damage_to_gent_target(
        target,
        damage,
        application,
        damaged_set,
        damage_events,
        commands,
    );
    true
}

fn apply_damage_to_gent_target(
    target: DamageTargetItem<'_>,
    mut damage: f32,
    application: &DamageApplication<'_>,
    damaged_set: &mut HashSet<Entity>,
    damage_events: &mut MessageWriter<DamageInfo>,
    commands: &mut Commands,
) {
    let (
        target_entity,
        mut health,
        target_transform,
        target_facing,
        maybe_player_statmod,
        is_invulnerable,
        is_player,
        maybe_spike_cooldown,
    ) = target;

    apply_stealth_damage(
        &mut damage,
        application.is_stealthed,
        application.owner_has_shadow_cloak,
    );
    let was_backstab = apply_backstab_damage(
        &mut damage,
        application.can_backstab,
        target_facing,
        target_transform,
        application.source_transform,
        application.backstab_multiplier,
    );

    let is_spike_hit = application.is_spike_hazard && is_player;
    let spike_immunity_active =
        spike_immunity_active(is_spike_hit, maybe_spike_cooldown.as_deref());
    zero_damage_when(&mut damage, spike_immunity_active || is_invulnerable);
    insert_chilled_effect(
        is_player,
        application.chilled_effect,
        target_entity,
        commands,
    );
    apply_player_defense(&mut damage, maybe_player_statmod);

    health.current = health.current.saturating_sub(damage as u32);
    write_damage_info(
        damage_events,
        damaged_set,
        DamageInfo {
            owner: application.owner,
            source: application.source,
            target: target_entity,
            amount: damage,
            crit: application.is_critical,
            stealthed: application.is_stealthed,
            backstab: was_backstab,
        },
    );
    reset_or_insert_spike_cooldown(
        is_spike_hit,
        spike_immunity_active,
        damage,
        maybe_spike_cooldown,
        target_entity,
        commands,
    );
    insert_pushback(target_entity, application.pushback, commands);
}

fn apply_stealth_damage(
    damage: &mut f32,
    is_stealthed: bool,
    owner_has_shadow_cloak: bool,
) {
    if is_stealthed && !owner_has_shadow_cloak {
        *damage *= 2.0;
    }
}

fn apply_backstab_damage(
    damage: &mut f32,
    can_backstab: bool,
    target_facing: &Facing,
    target_transform: &GlobalTransform,
    source_transform: &GlobalTransform,
    multiplier: Option<&BackstabMultiplier>,
) -> bool {
    if !can_backstab {
        return false;
    }
    if !is_backstab_hit(target_facing, target_transform, source_transform) {
        return false;
    }

    *damage *= multiplier.map(|m| m.0).unwrap_or(2.0);
    true
}

fn is_backstab_hit(
    target_facing: &Facing,
    target_transform: &GlobalTransform,
    source_transform: &GlobalTransform,
) -> bool {
    match *target_facing {
        Facing::Left => target_transform.translation().x
            > source_transform.translation().x,
        Facing::Right => target_transform.translation().x
            < source_transform.translation().x,
    }
}

fn spike_immunity_active(
    is_spike_hit: bool,
    spike_cooldown: Option<&SpikeDamageCooldown>,
) -> bool {
    is_spike_hit
        && spike_cooldown
            .map(|cooldown| cooldown.remaining_ticks > 0)
            .unwrap_or(false)
}

fn zero_damage_when(damage: &mut f32, condition: bool) {
    if condition {
        *damage = 0.0;
    }
}

fn insert_chilled_effect(
    is_player: bool,
    chilled_effect: Option<&ChilledEffect>,
    target: Entity,
    commands: &mut Commands,
) {
    let (true, Some(chilled_effect)) = (is_player, chilled_effect) else {
        return;
    };

    commands.entity(target).insert(chilled_effect.clone());
}

fn apply_player_defense(damage: &mut f32, statmod: Option<&PlayerStatMod>) {
    if let Some(statmod) = statmod {
        *damage /= statmod.defense;
    }
}

fn reset_or_insert_spike_cooldown(
    is_spike_hit: bool,
    spike_immunity_active: bool,
    damage: f32,
    maybe_spike_cooldown: Option<Mut<'_, SpikeDamageCooldown>>,
    target: Entity,
    commands: &mut Commands,
) {
    if !(is_spike_hit && !spike_immunity_active && damage > 0.0) {
        return;
    }
    if let Some(mut cooldown) = maybe_spike_cooldown {
        cooldown.reset();
        return;
    }

    commands.entity(target).insert(SpikeDamageCooldown::activate());
}

fn insert_pushback(
    target: Entity,
    pushback: Option<&Pushback>,
    commands: &mut Commands,
) {
    if let Some(pushback) = pushback {
        commands.entity(target).insert(pushback.0);
    }
}

fn emit_breakable_wall_damage_if_present(
    target: Entity,
    is_breakable_wall: bool,
    application: &DamageApplication<'_>,
    damage: f32,
    damaged_set: &mut HashSet<Entity>,
    damage_events: &mut MessageWriter<DamageInfo>,
) {
    if !is_breakable_wall {
        return;
    }

    write_damage_info(
        damage_events,
        damaged_set,
        DamageInfo {
            owner: application.owner,
            source: application.source,
            target,
            amount: damage,
            crit: false,
            stealthed: application.is_stealthed,
            backstab: false,
        },
    );
}

fn write_damage_info(
    damage_events: &mut MessageWriter<DamageInfo>,
    damaged_set: &mut HashSet<Entity>,
    damage_info: DamageInfo,
) {
    damaged_set.insert(damage_info.target);
    damage_events.write(damage_info);
}

#[expect(clippy::too_many_arguments, reason = "Bevy system dependencies are expressed as explicit system parameters.")]
pub fn apply_damage(
    mut damage_query: Query<(
        Entity,
        &mut DamageSource,
        &GlobalTransform,
        Has<Stealthed>,
        Has<Backstab>,
        Option<&BackstabMultiplier>,
        Option<&Pushback>,
        Option<&DamageSourceSkill>,
        Has<SpikeHazard>,
    )>,
    mut queries: ParamSet<(
        Query<
            (
                Entity,
                &mut Health,
                &GlobalTransform,
                &Facing,
                Option<&PlayerStatMod>,
                Has<Invulnerable>,
                Has<crate::game::player::Player>,
                Option<&mut SpikeDamageCooldown>,
            ),
            With<Gent>,
        >,
        Query<
            (Option<&mut Crits>, Option<&Passives>, Option<&Health>, PassiveOwner),
            With<Gent>,
        >,
    )>,
    enemy_query: Query<(Entity, &Transform), (With<Enemy>, Without<Dead>)>,
    breakable_walls: Query<(), With<BreakableWall>>,
    mut damage_events: MessageWriter<DamageInfo>,
    mut passive_events: MessageWriter<PassiveEvent>,
    mut commands: Commands,
    mut profiler: TickProfiler,
) {
    let _profile = profiler.scope("damage_apply");
    for (
        damage_entity,
        mut damage_source,
        transform,
        is_stealthed,
        can_backstab,
        maybe_backstab_multiplier,
        maybe_pushback,
        skill_tag,
        is_spike_hazard,
    ) in damage_query.iter_mut()
    {
        let target_set = mem::take(&mut damage_source.target_set);
        let damaged_set = mem::take(&mut damage_source.damaged_set);

        for target in target_set.difference(&damaged_set) {
            // First, gather owner data for passive calculations
            let mut damage = damage_source.damage;
            let mut is_critical = false;
            let target_snapshot =
                target_damage_snapshot(*target, &mut queries.p0());

            // Calculate enemies near this target for Pack Killer
            // Using same range as big spider defense trigger (24 pixels)
            const ENEMY_PROXIMITY_RADIUS: f32 = 24.0;
            let enemies_near_current_target = enemy_query
                .iter()
                .filter(|candidate| enemy_near_target_candidate(
                    candidate,
                    *target,
                    target_snapshot.position,
                    ENEMY_PROXIMITY_RADIUS,
                ))
                .count() as u32;

            // Access owner query to get crit and passive data
            let owner_has_shadow_cloak = apply_owner_damage_adjustments(
                OwnerDamageAdjustment {
                    owner: queries.p1().get_mut(damage_source.owner).ok(),
                    damage_entity,
                    source_skill: skill_tag,
                    owner_entity: damage_source.owner,
                    target_snapshot: &target_snapshot,
                    enemies_near_target: enemies_near_current_target,
                },
                &mut damage,
                &mut is_critical,
                &mut passive_events,
            );

            let chilled_effect = damage_source.chilled_effect.clone();
            let application = DamageApplication {
                owner: damage_source.owner,
                source: damage_entity,
                source_transform: transform,
                is_stealthed,
                can_backstab,
                backstab_multiplier: maybe_backstab_multiplier,
                pushback: maybe_pushback,
                chilled_effect: chilled_effect.as_ref(),
                is_spike_hazard,
                owner_has_shadow_cloak,
                is_critical,
            };

            let target_applied = apply_gent_target_if_present(
                queries.p0().get_mut(*target).ok(),
                damage,
                &application,
                &mut damage_source.damaged_set,
                &mut damage_events,
                &mut commands,
            );

            emit_breakable_wall_damage_if_present(
                *target,
                !target_applied && breakable_walls.contains(*target),
                &application,
                damage,
                &mut damage_source.damaged_set,
                &mut damage_events,
            );
        }
        damage_source.damaged_set.extend(damaged_set);
    }
}

pub(crate) fn emit_passive_events_from_damage_hit(
    query: Query<Entity, Added<Hit>>,
    mut passive_events: MessageWriter<PassiveEvent>,
) {
    for damage_entity in query.iter() {
        passive_events.write(PassiveEvent::DamageHit {
            damage_source: damage_entity,
        });
    }
}

pub(crate) fn emit_passive_events_from_damage(
    mut damage_events: MessageReader<DamageInfo>,
    mut passive_events: MessageWriter<PassiveEvent>,
    health_query: Query<&Health>,
) {
    for dmg in damage_events.read() {
        passive_events.write(PassiveEvent::DamageDealt(*dmg));
        passive_events.write(PassiveEvent::DamageTaken(*dmg));
        emit_backstab_kill_event(dmg, &health_query, &mut passive_events);
    }
}

fn emit_backstab_kill_event(
    damage: &DamageInfo,
    health_query: &Query<&Health>,
    passive_events: &mut MessageWriter<PassiveEvent>,
) {
    if !damage.backstab {
        return;
    }
    let Ok(health) = health_query.get(damage.target) else {
        return;
    };
    if health.current != 0 {
        return;
    }

    passive_events.write(PassiveEvent::BackstabKill {
        owner: damage.owner,
    });
}

pub(crate) fn on_hit_self_pushback(
    query: Query<(&DamageSource, &SelfPushback), Added<Hit>>,
    mut commands: Commands,
) {
    for (damage_source, self_pushback) in query.iter() {
        // Insert knockback on owner if it still exists by apply time.
        commands.queue(InsertIfExists::new(
            damage_source.owner,
            (self_pushback.0,),
        ));
    }
}

pub(crate) fn damage_source_tick(mut query: Query<&mut DamageSource>) {
    for mut damage_source in query.iter_mut() {
        damage_source.current_lifetime += 1;
    }
}

pub(crate) fn damage_source_cleanup(
    query: Query<(Entity, &DamageSource)>,
    mut commands: Commands,
) {
    for (entity, damage_source) in query.iter() {
        if damage_source.current_lifetime >= damage_source.max_lifetime {
            commands.entity(entity).try_despawn();
        }
    }
}

pub(crate) fn cleanup_owner_bound_damage_sources(
    query: Query<
        (Entity, &DamageSource),
        With<DespawnOnOwnerInvalid>,
    >,
    owners: Query<(), Without<Dead>>,
    mut commands: Commands,
) {
    for (entity, damage_source) in query.iter() {
        if owners.get(damage_source.owner).is_err() {
            commands.entity(entity).try_despawn();
        }
    }
}
