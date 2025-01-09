pub mod arc_attack;
pub mod particles;

use std::mem;

use arc_attack::Arrow;
use rapier2d::prelude::InteractionGroups;
use theseeker_engine::gent::Gent;
use theseeker_engine::physics::{
    update_sprite_colliders, Collider, PhysicsWorld, GROUND, PLAYER_ATTACK,
};

use super::enemy::{Defense, EnemyGfx, EnemyStateSet};
use super::gentstate::{Dead, Facing};
use super::physics::Knockback;
use super::player::player_weapon::CurrentWeapon;
use super::player::{
    on_crit_cooldown_reduce, on_hit_exit_stealthing,
    on_stealth_hit_cooldown_reset, Passive, Passives, Player, PlayerConfig,
    PlayerGfx, PlayerStateSet, StatusModifier,
};
use crate::game::attack::arc_attack::{arc_projectile, Projectile};
use crate::game::attack::particles::AttackParticlesPlugin;
use crate::prelude::*;
use crate::{camera::CameraShake, game::player::PlayerStatMod};

pub struct AttackPlugin;

impl Plugin for AttackPlugin {
    fn build(&self, app: &mut App) {
        app.register_type::<Crits>();
        app.add_plugins(AttackParticlesPlugin);
        app.add_gametick_event::<DamageInfo>();
        app.init_resource::<KillCount>();
        app.add_systems(
            GameTickUpdate,
            (
                // (determine_attack_targets, apply_attack_modifications, apply_attack_damage)
                // (track_crits, on_hit_player_pushback).in_set(OnAttackFirstHitSet)
                // (lifesteal, kill_on_damage, damage_flash).in_set(RespondToDamageInfoSet)
                arc_projectile,
                (
                    determine_attack_targets,
                    apply_attack_modifications,
                    // DamageInfo event emitted here
                    apply_attack_damage,
                    // OnAttackFirstHitSet
                    track_crits,
                    on_hit_cam_shake,
                    on_hit_self_pushback,
                    on_hit_lifesteal,
                    on_crit_cooldown_reduce,
                    on_stealth_hit_cooldown_reset,
                    on_hit_exit_stealthing,
                )
                    .chain(),
                (
                    kill_on_damage,
                    damage_flash,
                    apply_damage_flash,
                )
                    .chain()
                    .in_set(RespondToDamageInfoSet)
                    .after(apply_attack_damage),
                // cleanup
                attack_tick,
                despawn_projectile,
                despawn_player_arrows,
                attack_cleanup,
            )
                .chain()
                .after(update_sprite_colliders)
                .after(PlayerStateSet::Behavior)
                .after(EnemyStateSet::Behavior)
                .before(PlayerStateSet::Collisions)
                .before(EnemyStateSet::Collisions),
        );
    }
}

/// Effects that care about the first time an attack hits should go here
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct OnAttackFirstHitSet;

/// Effects that are applied when an instance of damage is dealt should go here
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct RespondToDamageInfoSet;

#[derive(Component)]
pub struct Health {
    pub current: u32,
    pub max: u32,
}

/// Component applied to Gfx entity sibling of Gent which has been damaged
#[derive(Component)]
pub struct DamageFlash {
    pub current_ticks: u32,
    pub max_ticks: u32,
}

#[derive(Bundle)]
pub struct AttackBundle {
    pub attack: Attack,
    pub collider: Collider,
}

#[derive(Component, Debug)]
pub struct Attack {
    pub current_lifetime: u32,
    pub max_lifetime: u32,
    pub damage: f32,
    /// Maximum number of targets that can be hit by this attack at once.
    pub max_targets: u32,
    pub attacker: Entity,

    /// Unique entities damaged by this attack.
    pub damaged_set: HashSet<Entity>,

    /// Unique entities that are in contact this tick and are valid targets.
    pub target_set: HashSet<Entity>,

    pub status_mod: Option<StatusModifier>,
}
impl Attack {
    /// Lifetime is in game ticks
    pub fn new(lifetime: u32, attacker: Entity, damage: f32) -> Self {
        Attack {
            current_lifetime: 0,
            max_lifetime: lifetime,
            damage,
            max_targets: 3,
            attacker,
            damaged_set: Default::default(),
            target_set: Default::default(),
            status_mod: None,
        }
    }

    pub fn set_stat_mod(mut self, modif: StatusModifier) -> Self {
        self.status_mod = Some(modif);
        self
    }

    pub fn with_damage(mut self, damage: f32) -> Self {
        self.damage = damage;
        self
    }

    pub fn with_max_targets(mut self, max_targets: u32) -> Self {
        self.max_targets = max_targets;
        self
    }
}

/// Event sent when damage is applied
#[derive(Event, Clone, Copy, PartialEq)]
pub struct DamageInfo {
    /// Entity that attacked
    pub attacker: Entity,
    /// Entity that dealt damage
    pub source: Entity,
    /// Entity that got damaged
    pub target: Entity,
    /// Amount of damage that was actually applied
    pub amount: f32,
    pub crit: bool,
    pub stealthed: bool,
}

/// Component added to attack entity to indicate it causes Knockback to the Gent it damages
#[derive(Component, Default, Deref)]
pub struct Pushback(pub Knockback);

/// Component added to attack entity to indicate it causes Knockback to the attacker
#[derive(Component, Default, Deref)]
pub struct SelfPushback(pub Knockback);

/// Component added to attack entity to indicate it was initiated by a Stealthing Gent
#[derive(Component)]
pub struct Stealthed;

/// Component added to an Attack entity the first time it hits something
#[derive(Component)]
pub struct Hit;

/// Component added to an Attack entity that indicates it *Can* backstab
#[derive(Component)]
pub struct Backstab;

/// Component added to an Attack entity to indicate that it is a crit
#[derive(Component, Debug)]
pub struct Crit;

// checks nearest entities, modifies attacks targets
pub fn determine_attack_targets(
    mut attack_query: Query<(
        Entity,
        &GlobalTransform,
        &mut Attack,
        &Collider,
    )>,
    damageable_query: Query<
        &GlobalTransform,
        (With<Collider>, With<Health>, With<Gent>),
    >,
    spatial_query: Res<PhysicsWorld>,
) {
    for (entity, transform, mut attack, collider) in attack_query.iter_mut() {
        let mut newly_collided: HashSet<Entity> = HashSet::default();
        let intersections = spatial_query.intersect(
            transform.translation().xy(),
            collider.0.shape(),
            collider
                .0
                .collision_groups()
                .with_filter(collider.0.collision_groups().filter | GROUND),
            Some(entity),
        );
        let mut targets = intersections
            .into_iter()
            // Filters out everything that's not damageable or one of the nearest max_targets entities to attack
            .filter_map(|colliding_entity| {
                if let Ok(damageable_transform) =
                    damageable_query.get(colliding_entity)
                {
                    newly_collided.insert(entity);
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

        // Get the closest targets
        let valid_targets = targets
            .into_iter()
            .take(attack.max_targets as usize - attack.damaged_set.len())
            .map(|(e, _)| e)
            .collect::<Vec<_>>();

        for entity in valid_targets.iter() {
            // if we already damaged this entity
            if attack.damaged_set.contains(entity) {
                continue;
            };

            // if the entity is not damageable
            if damageable_query.get(*entity).is_err() {
                continue;
            }

            attack.target_set.insert(*entity);
        }
    }
}

// could switch to command/component based approach for attack modifications
pub fn apply_attack_modifications(
    mut attack_query: Query<
        (Entity, &mut Attack, Has<Crit>, Has<Hit>),
        Without<Gent>,
    >,
    mut attacker_query: Query<
        (Option<&mut Crits>, Option<&Passives>),
        With<Gent>,
    >,
    mut commands: Commands,
) {
    for (entity, mut attack, is_crit, has_hit) in attack_query.iter_mut() {
        // if there are no targets we dont want to do anything
        if attack.target_set.is_empty() {
            continue;
        }
        // if there is an attacker, apply relevant buffs
        if let Ok((maybe_crits, maybe_passives)) =
            attacker_query.get_mut(attack.attacker)
        {
            // passives
            if let Some(passives) = maybe_passives {
                // backstab
                // check this later on application of damage for each enemy
                if passives.contains(&Passive::IceDagger) {
                    commands.entity(entity).insert(Backstab);
                }
            }
            // TODO: attack should keep its original damage and modify only the multipliers
            // crit multiplier
            //
            if let Some(mut crit) = maybe_crits {
                if crit.next_hit_is_critical && !is_crit {
                    commands.entity(entity).insert(Crit);
                    attack.damage *= crit.crit_damage_multiplier;
                    crit.next_hit_is_critical = false;
                }

                // increment hit count the first time we hit something with an attack
                if !has_hit {
                    crit.hit_count += 1;
                }
            }
        }
        if !has_hit {
            commands.entity(entity).insert(Hit);
        }
    }
}

// Now we only have the attack and target
pub fn apply_attack_damage(
    mut attack_query: Query<(
        Entity,
        &mut Attack,
        &GlobalTransform,
        Has<Stealthed>,
        Has<Backstab>,
        Has<Crit>,
        Option<&Pushback>,
    )>,
    mut target_query: Query<
        (
            Entity,
            &mut Health,
            &GlobalTransform,
            &Facing,
            Option<&PlayerStatMod>,
            Has<Defense>,
        ),
        With<Gent>,
    >,
    mut damage_events: EventWriter<DamageInfo>,
    mut commands: Commands,
) {
    for (
        a_entity,
        mut attack,
        transform,
        is_stealthed,
        can_backstab,
        is_crit,
        maybe_pushback,
    ) in attack_query.iter_mut()
    {
        // we take this and dont put it back each tick
        let target_set = mem::take(&mut attack.target_set);
        // this gets put back
        let damaged_set = mem::take(&mut attack.damaged_set);
        for target in target_set.difference(&damaged_set) {
            if let Ok((
                t_entity,
                mut health,
                t_transform,
                t_facing,
                maybe_player_statmod,
                is_defending,
            )) = target_query.get_mut(*target)
            {
                let mut damage = attack.damage;
                // modify damage based on target specific qualities
                // target defenses...
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
                        damage *= 3.;
                    }
                }
                if is_defending {
                    //TODO: switch to defense modifiers
                    damage /= 4.;
                }

                // TODO:
                // Apply Stat Modifier (if exists)
                if let Some(stat_modifier) = &attack.status_mod {
                    commands.entity(t_entity).insert(stat_modifier.clone());
                }

                // apply player defense modifier if it exists
                if let Some(statmod) = maybe_player_statmod {
                    damage /= statmod.defense;
                }

                // apply damage to the targets health
                health.current = health.current.saturating_sub(damage as u32);
                let damage_info = DamageInfo {
                    attacker: attack.attacker,
                    source: a_entity,
                    target: t_entity,
                    amount: damage,
                    crit: is_crit,
                    stealthed: is_stealthed,
                };
                attack.damaged_set.insert(t_entity);
                damage_events.send(damage_info);

                // apply Knockback
                if let Some(pushback) = maybe_pushback {
                    commands.entity(t_entity).insert(pushback.0);
                }
            }
        }
        attack.damaged_set.extend(damaged_set);
    }
}

pub fn despawn_projectile(
    query: Query<(Entity, &Attack), (With<Projectile>, With<Hit>)>,
    mut commands: Commands,
) {
    for (entity, attack) in query.iter() {
        if attack.damaged_set.len() == attack.max_targets as usize {
            // Note: purposefully does not despawn child entities, nor remove the
            // reference, so that child particle systems have the option of lingering
            commands.entity(entity).despawn();
        }
    }
}

pub fn despawn_player_arrows(
    query: Query<(Entity, &Transform, &Collider), With<Arrow>>,
    spatial_query: Res<PhysicsWorld>,
    mut commands: Commands,
) {
    for (entity, transform, collider) in query.iter() {
        let is_arrow_intersecting_with_ground = !spatial_query
            .intersect(
                transform.translation.xy(),
                collider.0.shape(),
                InteractionGroups {
                    memberships: PLAYER_ATTACK,
                    filter: GROUND,
                },
                None,
            )
            .is_empty();

        if is_arrow_intersecting_with_ground {
            // Note: purposefully does not despawn child entities, nor remove the
            // reference, so that child particle systems have the option of lingering
            commands.entity(entity).despawn();
        }
    }
}

pub fn kill_on_damage(
    query: Query<(Entity, &Health), With<Gent>>,
    mut damage_events: EventReader<DamageInfo>,
    mut commands: Commands,
) {
    for damage_info in damage_events.read() {
        if let Ok((entity, health)) = query.get(damage_info.target) {
            if health.current == 0 {
                commands.entity(entity).insert(Dead::default());
            }
        }
    }
}

/// Applies camera shaker on first hit if attacker is player
fn on_hit_cam_shake(
    query: Query<&Attack, Added<Hit>>,
    p_query: Query<Entity, With<Player>>,
    config: Res<PlayerConfig>,
    weapon: CurrentWeapon,
    mut commands: Commands,
) {
    for attack in query.iter() {
        if let Ok(_entity) = p_query.get(attack.attacker) {
            let camera_shake = if weapon.is_wielding_hammer() {
                CameraShake::new(
                    config.hammer_on_hit_screenshake_strength,
                    config.hammer_on_hit_screenshake_duration_secs,
                    config.hammer_on_hit_screenshake_frequency,
                )
            } else {
                CameraShake::new(
                    config.default_on_hit_screenshake_strength,
                    config.default_on_hit_screenshake_duration_secs,
                    config.default_on_hit_screenshake_frequency,
                )
            };

            commands.insert_resource(camera_shake);
        }
    }
}

/// Applies pushback to attacker on first hit of an attack with SelfPushback
fn on_hit_self_pushback(
    query: Query<(&Attack, &SelfPushback), Added<Hit>>,
    mut commands: Commands,
) {
    for (attack, self_pushback) in query.iter() {
        commands.entity(attack.attacker).insert(self_pushback.0);
    }
}

/// Heals attacker on first hit of a Stealthed attack
fn on_hit_lifesteal(
    query: Query<(&Attack, Has<Crit>), (Added<Hit>, With<Stealthed>)>,
    mut health_query: Query<&mut Health, Without<Attack>>,
) {
    for (attack, is_crit) in query.iter() {
        if let Ok(mut health) = health_query.get_mut(attack.attacker) {
            // heal by 100 percent or 20 percent max health
            let stealth_lifesteal = if is_crit { 1. } else { 0.2 };
            health.current = u32::min(
                health.current.saturating_add(
                    (stealth_lifesteal * health.max as f32) as u32,
                ),
                health.max,
            );
        }
    }
}

fn apply_damage_flash(
    sprite_query: Query<
        Entity,
        (
            With<Sprite>,
            Or<(With<EnemyGfx>, With<PlayerGfx>)>,
            Without<Gent>,
        ),
    >,
    gent_query: Query<&Gent>,
    mut damage_events: EventReader<DamageInfo>,
    mut commands: Commands,
) {
    for damage_info in damage_events.read() {
        if let Ok(gent) = gent_query.get(damage_info.target) {
            if let Ok(entity) = sprite_query.get(gent.e_gfx) {
                commands.entity(entity).insert(DamageFlash {
                    current_ticks: 0,
                    max_ticks: 8,
                });
            }
        }
    }
}

fn damage_flash(
    mut query: Query<(Entity, &mut Sprite, &mut DamageFlash)>,
    mut commands: Commands,
) {
    for (entity, mut sprite, mut damage_flash) in query.iter_mut() {
        sprite.color = Color::rgb(2.5, 2.5, 2.5);

        if damage_flash.current_ticks == damage_flash.max_ticks {
            commands.entity(entity).remove::<DamageFlash>();
            sprite.color = Color::rgb(1., 1., 1.);
        }
        damage_flash.current_ticks += 1;
    }
}

fn attack_tick(mut query: Query<&mut Attack>) {
    for mut attack in query.iter_mut() {
        attack.current_lifetime += 1;
    }
}

fn attack_cleanup(query: Query<(Entity, &Attack)>, mut commands: Commands) {
    for (entity, attack) in query.iter() {
        if attack.current_lifetime >= attack.max_lifetime {
            commands.entity(entity).despawn();
        }
    }
}

/// Resource which tracks total number of enemies killed
/// incremented in despawn_dead()
#[derive(Resource, Debug, Default, Deref, DerefMut)]
pub struct KillCount(pub u32);

fn track_crits(mut query: Query<(&mut Crits, Option<&Passives>, &Health)>) {
    for (mut crits, maybe_passives, health) in query.iter_mut() {
        if crits.hit_count != 0
            && (crits.hit_count % 17 == 0 || crits.hit_count % 19 == 0)
        {
            crits.next_hit_is_critical = true;
        }
        if let Some(passives) = maybe_passives {
            if passives.contains(&Passive::FlamingHeart)
                && health.current < health.max / 5
                && (crits.hit_count % 3 == 0 || crits.hit_count % 5 == 0)
            {
                crits.next_hit_is_critical = true;
            }
        }
    }
}

/// Allows the entity to apply critical strikes.
/// Crits in TheSeeker are special, and trigger once
/// every 17th and 19th successful hits
#[derive(Component, Default, Debug, Reflect)]
pub struct Crits {
    next_hit_is_critical: bool,
    /// Counts number of successful hits
    hit_count: u32,
    /// Yes
    crit_damage_multiplier: f32,
}

impl Crits {
    pub fn new(multiplier: f32) -> Self {
        Self {
            next_hit_is_critical: false,
            hit_count: 0,
            crit_damage_multiplier: multiplier,
        }
    }
}
