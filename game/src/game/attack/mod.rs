pub mod arc_attack;
pub mod particles;

use theseeker_engine::gent::Gent;
use theseeker_engine::physics::{
    update_sprite_colliders, Collider, LinearVelocity, PhysicsWorld, GROUND,
};

use super::enemy::EnemyGfx;
use super::player::{CanDash, CanStealth, Player, PlayerConfig, WhirlAbility};
use super::player::{PlayerGfx, PlayerPushback, StatusModifier};
use crate::game::enemy::{Defense, Enemy, EnemyStateSet};
use crate::game::player::PlayerStateSet;
use crate::game::{attack::particles::AttackParticlesPlugin, gentstate::Dead};
use crate::prelude::*;
use crate::{
    camera::CameraRig,
    game::attack::arc_attack::{arc_projectile, Projectile},
};

pub struct AttackPlugin;

impl Plugin for AttackPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(AttackParticlesPlugin);
        app.init_resource::<KillCount>();
        app.add_systems(
            GameTickUpdate,
            (
                arc_projectile,
                attack_damage,
                attack_tick,
                attack_cleanup,
                damage_flash,
                knockback,
                on_hit_player_pushback.after(attack_damage),
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
    attack: Attack,
    collider: Collider,
}

#[derive(Component)]
pub struct Attack {
    pub current_lifetime: u32,
    pub max_lifetime: u32,
    pub damage: u32,
    /// Maximum number of targets that can be hit by this attack at once.
    pub max_targets: u32,
    pub attacker: Entity,
    /// Includes every single instance of damage that was applied.
    /// (even against the same enemy)
    pub damaged: Vec<DamageInfo>,
    /// Tracks which entities collided with the attack, and still remain in contact.
    /// Not stored in damage info, because the collided entities might be
    /// different from the entities that damage is applied. (due to max_targets)
    pub collided: HashSet<Entity>,

    /// Unique entities that where in contact with collider and took damage.
    /// and are still in contact with the attack collider.
    pub damaged_set: HashSet<Entity>,
    /// used to track if multiple hits are in the same attack or not
    pub new_group: bool,
    pub stealthed: bool,

    pub pushback: Option<PlayerPushback>,
    pub pushback_applied: bool,
    pub status_mod: Option<StatusModifier>,
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
            damaged: Vec::new(),
            collided: Default::default(),
            damaged_set: Default::default(),
            new_group: false,
            stealthed: false,
            pushback: None,
            pushback_applied: false,
            status_mod: None,
        }
    }

    pub fn set_stat_mod(mut self, modif: StatusModifier) -> Self {
        self.status_mod = Some(modif);
        self
    }
}

pub struct DamageInfo {
    /// Entity that got damaged
    pub entity: Entity,
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

pub fn attack_damage(
    spatial_query: Res<PhysicsWorld>,
    mut query: Query<(
        Entity,
        &GlobalTransform,
        &mut Attack,
        &Collider,
        Option<&Pushback>,
        Option<&Projectile>,
    )>,
    mut health_query: Query<&mut Health>,
    mut damageable_query: Query<(
        Entity,
        &Collider,
        &Gent,
        &GlobalTransform,
        Has<Defense>,
        Has<Enemy>,
    )>,
    mut crits: Query<(&mut Crits), Without<Attack>>,
    mut player_skills: Query<
        (
            Option<&mut CanDash>,
            Option<&mut WhirlAbility>,
            Option<&mut CanStealth>,
        ),
        With<Player>,
    >,
    mut gfx_query: Query<Entity, Or<(With<PlayerGfx>, With<EnemyGfx>)>>,
    mut commands: Commands,
    mut rig: ResMut<CameraRig>,
    time: Res<GameTime>,
    config: Res<PlayerConfig>,
) {
    for (attacker_entity, pos, mut attack, attack_collider, maybe_pushback, maybe_projectile) in
        query.iter_mut()
    {
        let mut newly_collided: HashSet<Entity> = HashSet::default();
        let intersections = spatial_query.intersect(
            pos.translation().xy(),
            attack_collider.0.shape(),
            attack_collider
                .0
                .collision_groups()
                .with_filter(attack_collider.0.collision_groups().filter | GROUND),
            Some(attacker_entity),
        );
        let intersections_empty = intersections.is_empty();
        let mut targets = intersections
            .into_iter()
            // Filters out everything that's not damageable or one of the nearest max_targets entities to attack
            .filter_map(|colliding_entity| {
                if let Ok((_, _, _, dmgbl_pos, _, _)) = damageable_query.get(colliding_entity) {
                    newly_collided.insert(attacker_entity);
                    let dist = dmgbl_pos
                        .translation()
                        .xy()
                        .distance_squared(pos.translation().xy());
                    Some((colliding_entity, dist))
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();
        targets.sort_by(|(_, dist1), (_, dist2)| dist1.total_cmp(dist2));
        let targets_empty = targets.is_empty();
        // Get the closest ones
        let top_n = targets
            .into_iter()
            .take(attack.max_targets as usize)
            .map(|(e, _)| e)
            .collect::<Vec<_>>();

        for entity in top_n.iter() {
            if attack.damaged_set.contains(entity) {
                continue;
            };

            let Ok((damaged_entity, _, gent, _, is_defending, is_enemy)) =
                damageable_query.get_mut(*entity)
            else {
                continue;
            };

            // Apply Stat Modifier (if exists)
            if let Some(stat_modifier) = &attack.status_mod {
                commands
                    .entity(damaged_entity)
                    .insert(stat_modifier.clone());
            }

            attack.damaged_set.insert(damaged_entity);
            let mut damage_dealt = if is_defending {
                attack.damage / 10000
            } else {
                attack.damage
            };

            let mut was_critical = false;
            let was_stealthed = attack.stealthed;
            if let Ok((mut crit)) = crits.get_mut(attack.attacker) {
                if crit.next_hit_is_critical {
                    damage_dealt = (damage_dealt as f32 * crit.crit_damage_multiplier) as u32;
                    was_critical = true;
                    println!("critical damage!")
                }
                if attack.new_group == true {
                    crit.counter += 1;
                    println!("crit_counter: {}", crit.counter);
                }
            }
            // Stealth Effects
            let stealth_lifesteal = if was_critical { 1. } else { 0.2 };
            if was_stealthed {
                if let Ok(mut attacker_health) = health_query.get_mut(attack.attacker) {
                    attacker_health.current = u32::min(
                        attacker_health
                            .current
                            .saturating_add((damage_dealt as f32 * stealth_lifesteal) as u32),
                        config.max_health,
                    );
                }
            }
            if was_stealthed && was_critical {
                if let Ok((mut maybe_can_dash, mut maybe_whirl_ability, mut maybe_can_stealth)) =
                    player_skills.get_single_mut()
                {
                    if let Some(ref mut can_dash) = maybe_can_dash {
                        can_dash.remaining_cooldown = 0.;
                    }
                    if let Some(ref mut whirl_ability) = maybe_whirl_ability {
                        whirl_ability.energy = config.max_whirl_energy;
                    }
                    if let Some(ref mut can_stealth) = maybe_can_stealth {
                        can_stealth.remaining_cooldown = 0.;
                    }
                }
            }
            let mut damaged_health = health_query
                .get_mut(damaged_entity)
                .expect("damaged have health");

            damaged_health.current = damaged_health.current.saturating_sub(damage_dealt);
            attack.damaged.push(DamageInfo {
                entity: damaged_entity,
                tick: time.tick(),
                amount: damage_dealt,
                crit: was_critical,
            });
            if let Ok(anim_entity) = gfx_query.get_mut(gent.e_gfx) {
                //insert a DamageFlash to flash for 1 animation frame/8 ticks
                commands.entity(anim_entity).insert(DamageFlash {
                    current_ticks: 0,
                    max_ticks: 8,
                });
            }
            if damaged_health.current == 0 {
                commands.entity(damaged_entity).insert(Dead::default());
                //apply more screenshake if an enemies health becomes depleted by this attack
                if is_enemy {
                    rig.trauma = 0.4;
                }
            } else if rig.trauma < 0.3 && is_enemy {
                //apply screenshake on damage to enemy
                rig.trauma = 0.3
            }
            if let Some(pushback) = maybe_pushback {
                commands.entity(damaged_entity).insert(Knockback::new(
                    pushback.direction,
                    pushback.strength,
                    16,
                ));
            }

            if attack.new_group == true {
                attack.new_group = false;
            }
        }

        // Removes entities from collided and damaged_set that are not in newly_collided
        let Attack {
            collided,
            damaged_set,
            ..
        } = attack.as_mut();
        for e in collided.difference(&newly_collided) {
            damaged_set.remove(&*e);
        }
        *collided = newly_collided;
        // Handle the edge case where newly collided *and* collided might not have damaged
        // set's contents
        if targets_empty {
            damaged_set.clear();
            if attack.new_group == false {
                attack.new_group = true;

                if let Ok((mut crit)) = crits.get_mut(attack.attacker) {
                    let next_hit_counter = crit.counter + 1;
                    if next_hit_counter % 17 == 0 {
                        crit.next_hit_is_critical = true;
                    } else if next_hit_counter % 19 == 0 {
                        crit.next_hit_is_critical = true;
                    } else {
                        crit.next_hit_is_critical = false;
                    }
                }
            }
        }

        if maybe_projectile.is_some() && !intersections_empty && attack.current_lifetime > 1 {
            // Note: purposefully does not despawn child entities, nor remove the
            // reference, so that child particle systems have the option of lingering
            commands.entity(attacker_entity).despawn();
        }
    }
}

fn on_hit_player_pushback(
    mut commands: Commands,
    mut query: Query<(Entity, &mut Attack), (Changed<Attack>)>,
) {
    for (entity, mut attack) in query.iter_mut() {
        if !attack.pushback_applied && attack.pushback.is_some() {
            if !attack.damaged.is_empty() {
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

/// Allows the entity to apply critical strikes.
/// Crits in TheSeeker are special, and trigger once
/// every 17th and 19th successful hits
#[derive(Component, Default, Debug)]
pub struct Crits {
    next_hit_is_critical: bool,
    /// Counts how many successful hits since the 19th hit
    counter: u32,
    /// Yes
    crit_damage_multiplier: f32,
}

impl Crits {
    pub fn new(multiplier: f32) -> Self {
        Self {
            next_hit_is_critical: false,
            counter: 0,
            crit_damage_multiplier: multiplier,
        }
    }
}
