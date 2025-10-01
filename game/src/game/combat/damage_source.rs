use super::{DamageInfo, Health};
use crate::game::enemy::Enemy;
use crate::game::gentstate::Facing;
use crate::game::physics::Knockback;
use crate::game::player::crits::Crits;
use crate::game::player::passives::{PassiveContext, PassiveContextInputs, PassiveEvent};
use crate::game::player::skills::types::SkillId;
use crate::game::player::states::InAir;
use crate::game::player::{
    Passives, PlayerAction, PlayerStatMod,
};
use crate::game::effects::chilled::ChilledEffect;
use crate::prelude::*;
use bevy::ecs::system::Command;
use bevy::ecs::system::ParamSet;
use bevy_rapier2d::prelude::Sensor;
use leafwing_input_manager::prelude::ActionState;
use std::mem;
use theseeker_engine::gent::Gent;
use theseeker_engine::physics::{
    Collider, ColliderShapeAccess, CollisionGroups as InteractionGroups,
    PhysicsWorld, ENEMY, PLAYER_ATTACK,
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
            damaged_set: Default::default(),
            target_set: Default::default(),
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

#[derive(Bundle)]
pub struct DamageSourceBundle {
    pub damage_source: DamageSource,
    pub collider: Collider,
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

/// Tags a damage source with the originating skill to provide richer passive events.
#[derive(Component, Debug, Clone, Copy)]
pub struct DamageSourceSkill(pub SkillId);

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
        Option<&InteractionGroups>,
    )>,
    damageable_query: Query<
        (&GlobalTransform, &Collider),
        (With<Collider>, With<Health>, With<Gent>),
    >,
    spatial_query: PhysicsWorld,
) {
    for (_entity, transform, mut damage_source, collider, maybe_groups) in
        damage_query.iter_mut()
    {
        let collision_groups = maybe_groups.copied().unwrap_or(
            InteractionGroups::new(PLAYER_ATTACK, ENEMY),
        );

        let intersections = spatial_query.intersect(
            transform.translation().xy(),
            collider.shape(),
            collision_groups,
            Some(damage_source.owner), // Exclude owner, not damage source
        );

        let mut targets = intersections
            .into_iter()
            .filter_map(|colliding_entity| {
                if let Ok((damageable_transform, _damageable_collider)) =
                    damageable_query.get(colliding_entity)
                {
                    let dist = damageable_transform
                        .translation()
                        .xy()
                        .distance_squared(transform.translation().xy());
                    Some((colliding_entity, dist))
                } else {
                    None
                }
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
            if damage_source.damaged_set.contains(entity) {
                continue;
            };
            if damageable_query.get(*entity).is_err() {
                continue;
            }
            damage_source.target_set.insert(*entity);
        }
    }
}

pub fn apply_damage_modifications(
    mut damage_query: Query<
        (Entity, &mut DamageSource, Has<Hit>),
        Without<Gent>,
    >,
    mut owner_query: Query<
        (
            Option<&Passives>,
            Option<&Health>,
            Option<&Transform>,
            Option<&theseeker_engine::physics::LinearVelocity>,
            Option<&ActionState<PlayerAction>>,
            Option<&crate::game::player::EnemiesNearby>,
            Option<&crate::game::player::Grounded>,
            Option<&InAir>,
            Option<&crate::game::player::BuffTick>,
            Option<&crate::game::player::EnemyProximitySensor>,
            Option<&crate::game::player::states::Running>,
            Option<&crate::game::player::LastGroundedPosition>,
        ),
        With<Gent>,
    >,
    mut commands: Commands,
) {
    for (entity, mut damage_source, has_hit) in damage_query.iter_mut() {
        if damage_source.target_set.is_empty() {
            continue;
        }

        // Reset damage to base to prevent compounding on persistent entities (like whirling)
        damage_source.damage = damage_source.base_damage;

        if let Ok((
            maybe_passives,
            maybe_health,
            _maybe_transform,
            maybe_velocity,
            maybe_action_state,
            maybe_enemies_nearby,
            maybe_grounded,
            maybe_in_air,
            maybe_buff_tick,
            maybe_enemy_proximity,
            maybe_running,
            maybe_last_grounded_pos,
        )) = owner_query.get_mut(damage_source.owner)
        {
            if let Some(passives) = maybe_passives {
                let mut modifiers =
                    crate::game::player::passives::DamageModifiers::default();
                modifiers.current_target_count = damage_source.target_set.len();
                let _default_health = Health { current: 0, max: 0 };

                let enemies_nearby_count =
                    if let Some(proximity) = maybe_enemy_proximity {
                        proximity.enemies_in_melee_range.len() as u32
                    } else if let Some(enemies) = maybe_enemies_nearby {
                        enemies.0
                    } else {
                        0
                    };

                let pctx = PassiveContext::from_inputs(PassiveContextInputs {
                    health: maybe_health,
                    velocity: maybe_velocity.map(|v| v.0),
                    enemies_nearby: Some(enemies_nearby_count),
                    grounded: maybe_grounded.is_some(),
                    in_air: maybe_in_air.is_some(),
                    buff_stacks: maybe_buff_tick.map(|b| b.stacks),
                    closest_enemy_distance: maybe_enemy_proximity
                        .map(|s| s.closest_distance),
                    is_running: maybe_running.is_some(),
                    target_position: None,
                    last_grounded_position: maybe_last_grounded_pos
                        .map(|p| p.position),
                    movement_input: maybe_action_state
                        .map(|a| a.clamped_value(&PlayerAction::Move)),
                    jump_pressed: maybe_action_state
                        .map(|a| a.pressed(&PlayerAction::Jump)),
                    enemies_near_target: Some(0),
                    target_health_pct: None,
                    rotation_active: false,
                });

                use crate::game::player::passives::get_passive_implementation;
                let mut implementations: Vec<
                    Box<dyn crate::game::player::passives::PassiveEffect>,
                > = passives
                    .equipped
                    .iter()
                    .map(|p| get_passive_implementation(p))
                    .collect();
                implementations.sort_by_key(|p| -p.priority());
                for p in implementations.iter() {
                    p.modify_damage(&mut modifiers, &pctx);
                }
                if modifiers.can_backstab {
                    // Also make this insertion resilient to prior queued despawn.
                    commands.queue(InsertIfExists::new(entity, Backstab));
                }
                // Apply modifier to the already-reset damage
                if modifiers.damage_multiplier != 1.0 {
                    damage_source.damage *= modifiers.damage_multiplier;
                }
            }
        }
        if !has_hit {
            // Use a safe insert that checks existence at apply time to avoid
            // panics when a despawn was queued earlier in the tick for this entity.
            commands.queue(InsertIfExists::new(entity, Hit));
        }
    }
}

pub fn apply_damage(
    mut damage_query: Query<(
        Entity,
        &mut DamageSource,
        &GlobalTransform,
        Has<Stealthed>,
        Has<Backstab>,
        Option<&Pushback>,
        Option<&DamageSourceSkill>,
    )>,
    mut queries: ParamSet<(
        Query<
            (
                Entity,
                &mut Health,
                &GlobalTransform,
                &Facing,
                Option<&PlayerStatMod>,
                Has<crate::game::enemy::Defense>,
                Has<crate::game::player::Player>,
            ),
            With<Gent>,
        >,
        Query<
            (
                Option<&mut Crits>,
                Option<&Passives>,
                Option<&Health>,
                Option<&theseeker_engine::physics::LinearVelocity>,
                Option<&ActionState<PlayerAction>>,
                Option<&crate::game::player::EnemiesNearby>,
                Option<&crate::game::player::Grounded>,
                Option<&InAir>,
                Option<&crate::game::player::BuffTick>,
                Option<&crate::game::player::EnemyProximitySensor>,
                Option<&crate::game::player::states::Running>,
                Option<&crate::game::player::LastGroundedPosition>,
            ),
            With<Gent>,
        >,
    )>,
    enemy_query: Query<(Entity, &Transform), With<Enemy>>,
    mut damage_events: EventWriter<DamageInfo>,
    mut passive_events: EventWriter<PassiveEvent>,
    mut commands: Commands,
) {
    for (
        damage_entity,
        mut damage_source,
        transform,
        is_stealthed,
        can_backstab,
        maybe_pushback,
        skill_tag,
    ) in damage_query.iter_mut()
    {
        let target_set = mem::take(&mut damage_source.target_set);
        let damaged_set = mem::take(&mut damage_source.damaged_set);

        for target in target_set.difference(&damaged_set) {
            // First, gather owner data for passive calculations
            let mut damage = damage_source.damage;
            let mut is_critical = false;
            let mut target_pos = Vec2::ZERO;
            let mut target_health_pct: Option<f32> = None;

            // Get target position first (we'll need it for passives)
            if let Ok((_, t_health, t_transform, _, _, _, _)) =
                queries.p0().get(*target)
            {
                target_pos = t_transform.translation().truncate();
                if t_health.max > 0 {
                    target_health_pct =
                        Some(t_health.current as f32 / t_health.max as f32);
                }
            }

            // Calculate enemies near this target for Pack Killer
            // Using same range as big spider defense trigger (24 pixels)
            const ENEMY_PROXIMITY_RADIUS: f32 = 24.0;
            let enemies_near_current_target = enemy_query
                .iter()
                .filter(|(e, enemy_tf)| {
                    // Don't count the target itself
                    *e != *target
                        && enemy_tf.translation.truncate().distance(target_pos)
                            <= ENEMY_PROXIMITY_RADIUS
                })
                .count() as u32;

            // Access owner query to get crit and passive data
            if let Ok((
                maybe_crits,
                maybe_passives,
                maybe_health,
                maybe_velocity,
                maybe_action_state,
                maybe_enemies_nearby,
                maybe_grounded,
                maybe_in_air,
                maybe_buff_tick,
                maybe_enemy_proximity,
                maybe_running,
                maybe_last_grounded_pos,
            )) = queries.p1().get_mut(damage_source.owner)
            {
                if let Some(mut crits) = maybe_crits {
                    crits.hit_count += 1;

                    if crits.next_hit_is_critical {
                        is_critical = true;
                        damage *= crits.crit_damage_multiplier;
                        crits.next_hit_is_critical = false;
                        passive_events.write(PassiveEvent::CriticalHit {
                            damage_source: damage_entity,
                            source_skill: skill_tag.map(|tag| tag.0),
                        });
                    }

                    passive_events.write(PassiveEvent::HitCountAdvanced {
                        owner: damage_source.owner,
                        hit_count: crits.hit_count,
                    });
                }
                // Apply per-target passive modifiers (has target position, proximity, and health pct)
                if let Some(passives) = maybe_passives {
                    let mut modifiers =
                        crate::game::player::passives::DamageModifiers::default(
                        );
                    let _default_health = Health { current: 0, max: 0 };
                    let enemies_nearby_count =
                        if let Some(proximity) = maybe_enemy_proximity {
                            proximity.enemies_in_melee_range.len() as u32
                        } else if let Some(enemies) = maybe_enemies_nearby {
                            enemies.0
                        } else {
                            0
                        };
                    let pctx = PassiveContext::from_inputs(PassiveContextInputs {
                        health: maybe_health,
                        velocity: maybe_velocity.map(|v| v.0),
                        enemies_nearby: Some(enemies_nearby_count),
                        grounded: maybe_grounded.is_some(),
                        in_air: maybe_in_air.is_some(),
                        buff_stacks: maybe_buff_tick.map(|b| b.stacks),
                        closest_enemy_distance: maybe_enemy_proximity
                            .map(|s| s.closest_distance),
                        is_running: maybe_running.is_some(),
                        target_position: Some(target_pos),
                        last_grounded_position: maybe_last_grounded_pos
                            .map(|p| p.position),
                        movement_input: maybe_action_state
                            .map(|a| a.clamped_value(&PlayerAction::Move)),
                        jump_pressed: maybe_action_state
                            .map(|a| a.pressed(&PlayerAction::Jump)),
                        enemies_near_target: Some(enemies_near_current_target),
                        target_health_pct,
                        rotation_active: false,
                    });
                    use crate::game::player::passives::get_passive_implementation;
                    let mut impls: Vec<
                        Box<dyn crate::game::player::passives::PassiveEffect>,
                    > = passives
                        .equipped
                        .iter()
                        .map(|p| get_passive_implementation(p))
                        .collect();
                    impls.sort_by_key(|p| -p.priority());
                    for p in impls.iter() {
                        p.modify_damage(&mut modifiers, &pctx);
                    }
                    if modifiers.damage_multiplier != 1.0 {
                        damage *= modifiers.damage_multiplier;
                    }
                }
            }

            // Precompute owner passive flags needed during application
            let owner_has_shadow_cloak = queries
                .p1()
                .get(damage_source.owner)
                .ok()
                .and_then(
                    |(
                        _maybe_crits,
                        maybe_passives,
                        _maybe_health,
                        _maybe_velocity,
                        _maybe_action_state,
                        _maybe_enemies_nearby,
                        _maybe_grounded,
                        _maybe_in_air,
                        _maybe_buff_tick,
                        _maybe_enemy_proximity,
                        _maybe_running,
                        _maybe_last_grounded_pos,
                    )| maybe_passives,
                )
                .map(|p| p.contains(&crate::game::player::Passive::ShadowCloak))
                .unwrap_or(false);

            // Now apply damage to target
            if let Ok((
                t_entity,
                mut health,
                t_transform,
                t_facing,
                maybe_player_statmod,
                is_defending,
                is_player,
            )) = queries.p0().get_mut(*target)
            {
                let mut was_backstab = false;
                if is_stealthed {
                    // If Shadow Cloak is equipped on the owner, suppress stealth double damage
                    if !owner_has_shadow_cloak {
                        damage *= 2.0;
                    }
                }

                if can_backstab {
                    let is_backstab = match *t_facing {
                        Facing::Left => {
                            t_transform.translation().x
                                > transform.translation().x
                        },
                        Facing::Right => {
                            t_transform.translation().x
                                < transform.translation().x
                        },
                    };
                    if is_backstab {
                        damage *= 2.;
                        was_backstab = true;
                    }
                }

                if is_defending {
                    // Only big spiders can defend, and they receive 0 damage when defending
                    damage = 0.0;
                }

                // Only apply chilled effect to players
                if is_player {
                    if let Some(chilled_effect) = &damage_source.chilled_effect
                    {
                        commands
                            .entity(t_entity)
                            .insert(chilled_effect.clone());
                    }
                }

                if let Some(statmod) = maybe_player_statmod {
                    damage /= statmod.defense;
                }

                health.current = health.current.saturating_sub(damage as u32);

                let damage_info = DamageInfo {
                    owner: damage_source.owner,
                    source: damage_entity,
                    target: t_entity,
                    amount: damage,
                    crit: is_critical,
                    stealthed: is_stealthed,
                    backstab: was_backstab,
                };
                damage_source.damaged_set.insert(t_entity);
                damage_events.write(damage_info);

                if let Some(pushback) = maybe_pushback {
                    commands.entity(t_entity).insert(pushback.0);
                }
            }
        }
        damage_source.damaged_set.extend(damaged_set);
    }
}

pub(crate) fn emit_passive_events_from_damage_hit(
    query: Query<Entity, Added<Hit>>,
    mut passive_events: EventWriter<PassiveEvent>,
) {
    for damage_entity in query.iter() {
        passive_events.write(PassiveEvent::DamageHit {
            damage_source: damage_entity,
        });
    }
}

pub(crate) fn emit_passive_events_from_damage(
    mut damage_events: EventReader<DamageInfo>,
    mut passive_events: EventWriter<PassiveEvent>,
    health_query: Query<&Health>,
) {
    for dmg in damage_events.read() {
        passive_events.write(PassiveEvent::DamageDealt(*dmg));
        passive_events.write(PassiveEvent::DamageTaken(*dmg));
        // Emit a specialized event when a backstab kills the target
        if dmg.backstab {
            if let Ok(health) = health_query.get(dmg.target) {
                if health.current == 0 {
                    passive_events
                        .write(PassiveEvent::BackstabKill { owner: dmg.owner });
                }
            }
        }
    }
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
            commands.entity(entity).despawn();
        }
    }
}
