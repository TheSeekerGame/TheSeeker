pub mod arc_attack;
pub mod particles;

use std::mem;

use theseeker_engine::gent::Gent;
use theseeker_engine::physics::{
    update_sprite_colliders, Collider, LinearVelocity, PhysicsWorld, GROUND,
};

use super::enemy::{Defense, EnemyGfx, EnemyStateSet};
use super::gentstate::{Dead, Facing};
use super::player::{
    Passive, Passives, PlayerGfx, PlayerPushback, PlayerStateSet, StatusModifier, Stealthing,
};
use crate::game::attack::arc_attack::{arc_projectile, Projectile};
use crate::game::attack::particles::AttackParticlesPlugin;
use crate::prelude::*;

pub struct AttackPlugin;

impl Plugin for AttackPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(AttackParticlesPlugin);
        app.add_gametick_event::<DamageInfo>();
        app.init_resource::<KillCount>();
        app.add_systems(
            GameTickUpdate,
            (
                arc_projectile,
                (
                    determine_attack_targets,
                    apply_attack_modifications,
                    apply_attack_damage,
                    kill_on_damage,
                    apply_damage_flash,
                )
                    .chain(),
                track_crits,
                attack_tick,
                despawn_projectile,
                attack_cleanup,
                damage_flash,
                knockback,
                on_hit_player_pushback.after(kill_on_damage),
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
#[derive(Component)]
pub struct Health {
    pub current: u32,
    pub max: u32,
}

///Component applied to Gfx entity sibling of Gent which has been damaged
#[derive(Component)]
pub struct DamageFlash {
    pub current_ticks: u32,
    pub max_ticks: u32,
}

#[derive(Bundle)]
pub struct AttackBundle {
    pub attack: Attack,
    pub collider: Collider,
    //transform?
}

#[derive(Component)]
pub struct Attack {
    pub current_lifetime: u32,
    pub max_lifetime: u32,
    pub damage: u32,
    /// Maximum number of targets that can be hit by this attack at once.
    pub max_targets: u32,
    pub attacker: Entity,

    /// Tracks which entities collided with the attack, and still remain in contact.
    /// Not stored in damage info, because the collided entities might be
    /// different from the entities that damage is applied. (due to max_targets)
    pub collided: HashSet<Entity>,

    /// Unique entities that were in contact with collider and took damage.
    /// and are still in contact with the attack collider.
    pub damaged_set: HashSet<Entity>,

    /// Unique entities that were in contact with collider and were valid targets.
    /// and are still in contact with the attack collider.
    pub target_set: HashSet<Entity>,

    ///TODO: multiple hits should be multiple attacks...
    /// used to track if multiple hits are in the same attack or not
    // pub new_group: bool,
    pub stealthed: bool,

    pub pushback: Option<PlayerPushback>,
    pub pushback_applied: bool,
    pub status_mod: Option<StatusModifier>,

    ///
    pub can_backstab: bool,

    /// set true if the hit can crit
    /// if the attack can crit, it will be Some, if the crit has been applied/tracked already it is
    /// True, if not False
    /// if the attack can not crit it will be None
    pub crit: Option<Crit>,
}
impl Attack {
    /// Lifetime is in game ticks
    pub fn new(lifetime: u32, attacker: Entity) -> Self {
        Attack {
            current_lifetime: 0,
            max_lifetime: lifetime,
            damage: 20,
            max_targets: 3,
            attacker,
            collided: Default::default(),
            damaged_set: Default::default(),
            target_set: Default::default(),
            stealthed: false,
            pushback: None,
            pushback_applied: false,
            status_mod: None,
            can_backstab: false,
            crit: None,
        }
    }

    pub fn set_stat_mod(mut self, modif: StatusModifier) -> Self {
        self.status_mod = Some(modif);
        self
    }

    pub fn set_stealth(mut self, stealth: bool) -> Self {
        self.stealthed = stealth;
        self
    }

    pub fn set_pushback(mut self, pushback: PlayerPushback) -> Self {
        self.pushback = Some(pushback);
        self
    }
}

/// Event sent when damage is applied
#[derive(Event, Clone, Copy, Hash, Eq, PartialEq)]
pub struct DamageInfo {
    /// Entity that dealt damage
    pub source: Entity,
    /// Entity that got damaged
    pub target: Entity,
    /// The tick it was damaged
    pub tick: u64,
    /// Amount of damage that was actually applied
    pub amount: u32,
    pub crit: bool,
}

///Component added to attack entity to indicate it causes knockback
#[derive(Component, Default)]
pub struct Pushback {
    pub direction: f32,
    pub strength: f32,
}

///Component added to an entity damaged by a pushback attack
#[derive(Component, Default, Debug)]
pub struct Knockback {
    pub ticks: u32,
    pub max_ticks: u32,
    pub direction: f32,
    pub strength: f32,
}

impl Knockback {
    pub fn new(direction: f32, strength: f32, max_ticks: u32) -> Self {
        Knockback {
            ticks: 0,
            max_ticks,
            direction,
            strength,
        }
    }
}

//checks nearest entities, modifies attacks targets
pub fn determine_attack_targets(
    mut attack_query: Query<(
        Entity,
        &GlobalTransform,
        &mut Attack,
        &Collider,
    )>,
    // damageable_query: Query<(Entity, &GlobalTransform), (With<Collider>, With<Health>, With<Gent>)>,
    damageable_query: Query<&GlobalTransform, (With<Collider>, With<Health>, With<Gent>)>,
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
        // let intersections_empty = intersections.is_empty();
        let mut targets = intersections
            .into_iter()
            // Filters out everything that's not damageable or one of the nearest max_targets entities to attack
            .filter_map(|colliding_entity| {
                if let Ok(damageable_transform) = damageable_query.get(colliding_entity) {
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
        // let targets_empty = targets.is_empty();

        // Get the closest targets
        let valid_targets = targets
            .into_iter()
            .take(attack.max_targets as usize - attack.damaged_set.len())
            .map(|(e, _)| e)
            .collect::<Vec<_>>();

        for entity in valid_targets.iter() {
            //if we already damaged this entity
            if attack.damaged_set.contains(entity) {
                continue;
            };

            //if the entity is not damageable
            if damageable_query.get(*entity).is_err() {
                continue;
            }
            // let Ok(dmgbl_trnsfrm) = damageable_query.get(*entity) else {
            //     continue;
            // };

            //TODO: make sure this is correct entity
            //TODO: change to target_set?
            // attack.damaged_set.insert(*entity);
            attack.target_set.insert(*entity);
        }
    }
}

//could switch to command/component based approach for attack modifications
pub fn apply_attack_modifications(
    mut attack_query: Query<&mut Attack, Without<Gent>>,
    mut attacker_query: Query<
        (
            Option<&mut Crits>,
            Option<&mut Stealthing>,
            // Option<&mut FocusAbility>,
            Option<&Passives>,
        ),
        With<Gent>,
    >,
) {
    for mut attack in attack_query.iter_mut() {
        //if there are no targets we dont want to do anything
        if attack.target_set.is_empty() {
            continue;
        }
        //if there is an attacker, apply relevant buffs
        // if let Ok((maybe_crits, maybe_focus, maybe_passives)) =
        if let Ok((maybe_crits, maybe_stealthing, maybe_passives)) =
            attacker_query.get_mut(attack.attacker)
        {
            //TODO: attack should keep its original damage and modify only the multipliers
            // crit multiplier
            if let Some(mut crit) = maybe_crits {
                println!("there is a crit component");
                if crit.next_hit_is_critical {
                    if let Some(a_crit) = &attack.crit {
                        if a_crit.applied {
                            continue;
                        }
                    }
                    attack.damage = (attack.damage as f32 * crit.crit_damage_multiplier) as u32;
                    println!("can_crit set true");
                    attack.crit = Some(Crit { applied: false });
                    crit.next_hit_is_critical = false;
                }
                //TODO: come back if something is broken with crit damage
                //move to application of damage?
                //increase crit_counter if the attack hit something new... but should it only
                //increase the hit once per swing?
                // if attack.new_group {
                //     crit.counter += 1;
                // }
            }

            //focus multiplier
            // if let Some(mut focus) = maybe_focus {
            //     if focus.state != FocusState::InActive {
            //         attack.damage *= 2;
            //         focus.state = FocusState::Applied
            //     }
            // }

            //passives
            if let Some(passives) = maybe_passives {
                //backstab
                //check this later on application of damage for each enemy
                if passives.contains(&Passive::Backstab) {
                    attack.can_backstab = true;
                }
            }
        }
    }
}

//Now we only have the attack and target
pub fn apply_attack_damage(
    mut attack_query: Query<(
        &mut Attack,
        &GlobalTransform,
        Option<&Pushback>,
        Option<&Projectile>,
    )>,
    mut target_query: Query<
        (
            Entity,
            &mut Health,
            &GlobalTransform,
            &Facing,
            Has<Defense>,
        ),
        With<Gent>,
    >,
    mut damage_events: EventWriter<DamageInfo>,
    mut commands: Commands,
    time: Res<GameTime>,
) {
    for (mut attack, transform, maybe_pushback, maybe_projectile) in attack_query.iter_mut() {
        let target_set = mem::take(&mut attack.target_set);
        //do i want to do this?
        let damaged_set = mem::take(&mut attack.damaged_set);
        // let target_set = target_set.difference(&damaged_set);
        for target in target_set.difference(&damaged_set) {
            if let Ok((t_entity, mut health, t_transform, t_facing, is_defending)) =
                target_query.get_mut(*target)
            {
                let mut damage = attack.damage;
                //modify damage based on target specific qualities
                //target defenses...
                if attack.can_backstab {
                    let is_backstab = match *t_facing {
                        Facing::Right => t_transform.translation().x > transform.translation().x,
                        Facing::Left => t_transform.translation().x < transform.translation().x,
                    };
                    if is_backstab {
                        damage *= 2;
                    }
                }
                if is_defending {
                    damage /= 4;
                }

                //TODO:
                // Apply Stat Modifier (if exists)
                if let Some(stat_modifier) = &attack.status_mod {
                    commands.entity(t_entity).insert(stat_modifier.clone());
                }

                //             // Stealth Effects
                //             let stealth_lifesteal = if was_critical { 1. } else { 0.2 };
                //             if was_stealthed {
                //                 if let Ok(mut attacker_health) = health_query.get_mut(attack.attacker) {
                //                     attacker_health.current = u32::min(
                //                         attacker_health
                //                             .current
                //                             .saturating_add((damage_dealt as f32 * stealth_lifesteal) as u32),
                //                         config.max_health,
                //                     );

                //             if was_stealthed && was_critical {
                //                 if let Ok((mut maybe_can_dash, mut maybe_whirl_ability, mut maybe_can_stealth)) =
                //                     player_skills.get_single_mut()
                //                 {
                //                     if let Some(ref mut can_dash) = maybe_can_dash {
                //                         can_dash.remaining_cooldown = 0.;
                //                     }
                //                     if let Some(ref mut whirl_ability) = maybe_whirl_ability {
                //                         whirl_ability.energy = config.max_whirl_energy;
                //                     }
                //                     if let Some(ref mut can_stealth) = maybe_can_stealth {
                //                         can_stealth.remaining_cooldown = 0.;
                //                     }
                //                 }
                //             }

                let crit = if let Some(c) = &attack.crit {
                    c.applied
                } else {
                    false
                };
                println!("is a crit? {:?}", crit);

                if attack.stealthed {
                    //do lifesteal

                    if crit {
                        //refresh cooldowns
                        //send event?
                    }
                }

                //apply damage to the targets health
                health.current = health.current.saturating_sub(damage);
                let damage_info = DamageInfo {
                    source: attack.attacker,
                    target: t_entity,
                    tick: time.tick(),
                    amount: damage,
                    crit,
                };
                //send a damage event and add the damage info to the attack
                // attack.damaged.push(damage_info);
                // attack.damaged.insert((t_entity, damage_info));
                attack.damaged_set.insert(t_entity);
                damage_events.send(damage_info);

                //apply Knockback
                if let Some(pushback) = maybe_pushback {
                    commands.entity(t_entity).insert(Knockback::new(
                        pushback.direction,
                        pushback.strength,
                        16,
                    ));
                }
            }
        }
        attack.damaged_set.extend(damaged_set);
    }
}

pub fn despawn_projectile(
    query: Query<(Entity, &Attack), With<Projectile>>,
    mut commands: Commands,
) {
    for (entity, attack) in query.iter() {
        if attack.current_lifetime > 1 && !attack.damaged_set.is_empty() {
            //TODO: ensure projectiles have a max_targets of 1/or however many is correct
            // if attack.current_lifetime > 1 && attack.damaged_set.len() == attack.max_targets as usize {
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

fn on_hit_player_pushback(
    mut commands: Commands,
    mut query: Query<(Entity, &mut Attack), (Changed<Attack>)>,
) {
    for (entity, mut attack) in query.iter_mut() {
        if !attack.pushback_applied && attack.pushback.is_some() {
            if !attack.damaged_set.is_empty() {
                commands
                    .entity(attack.attacker)
                    .insert(attack.pushback.unwrap());
                attack.pushback_applied = true;
            }
        }
    }
}

//maybe should not modify velocity directly but add knockback, but this makes it behave differently
//in states which dont set velocity every frame
fn knockback(
    mut query: Query<(
        Entity,
        &mut Knockback,
        &mut LinearVelocity,
        Has<Defense>,
    )>,
    mut commands: Commands,
) {
    for (entity, mut knockback, mut velocity, is_defending) in query.iter_mut() {
        knockback.ticks += 1;
        if !is_defending {
            velocity.x += knockback.direction * knockback.strength;
        }
        if knockback.ticks > knockback.max_ticks {
            velocity.x = 0.;
            commands.entity(entity).remove::<Knockback>();
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
            //Without<DamageFlash>,
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

fn damage_flash(mut query: Query<(Entity, &mut Sprite, &mut DamageFlash)>, mut commands: Commands) {
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

fn track_crits(
    mut query: Query<&mut Crits>,
    mut attack_query: Query<&mut Attack>,
    mut damage_events: EventReader<DamageInfo>,
) {
    for damage_info in damage_events.read() {
        if let Ok(mut attack) = attack_query.get_mut(damage_info.source) {
            if let Some(crit) = &mut attack.crit {
                if !crit.applied {
                    crit.applied = true;
                    if let Ok(mut crits) = query.get_mut(attack.attacker) {
                        crits.hit_count += 1;
                    }
                }
            }
            // if damage_info.crit {
            //     crits.hit_count += 1;
            // }
        }
    }

    //if this attack hit something for the first time, +1 the crit counter
    //if the attack crits, it should know not to crit again
}

/// Allows the entity to apply critical strikes.
/// Crits in TheSeeker are special, and trigger once
/// every 17th and 19th successful hits
#[derive(Component, Default, Debug)]
pub struct Crits {
    next_hit_is_critical: bool,
    /// Counts how many successful hits since the 19th hit
    hit_count: u32,
    /// Yes
    crit_damage_multiplier: f32,
}

//or just do Some(bool)
pub struct Crit {
    // Possible,
    applied: bool,
    // Applied,
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
