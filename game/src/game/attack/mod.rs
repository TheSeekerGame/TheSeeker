pub mod arc_attack;
pub mod particles;

use theseeker_engine::assets::animation::SpriteAnimation;
use theseeker_engine::gent::Gent;
use theseeker_engine::physics::{
    update_sprite_colliders, Collider, LinearVelocity, PhysicsSet, PhysicsWorld, GROUND,
};
use theseeker_engine::script::ScriptPlayer;

use super::enemy::EnemyGfx;
use super::player::PlayerGfx;
use crate::game::attack::arc_attack::{arc_projectile, Projectile};
use crate::game::attack::particles::AttackParticlesPlugin;
use crate::game::enemy::{Defense, Enemy, EnemyStateSet};
use crate::game::player::PlayerStateSet;
use crate::prelude::*;

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
            )
                .chain()
                .after(update_sprite_colliders)
                .after(PlayerStateSet::Behavior)
                .after(EnemyStateSet::Behavior)
                .before(PlayerStateSet::Collisions)
                .before(EnemyStateSet::Collisions),
        );
        app.add_systems(
            GameTickUpdate,
            despawn_dead
                //TODO: unify statesets?
                .after(PlayerStateSet::Transition)
                .after(EnemyStateSet::Transition)
                //has to be before physics set or colliders sometimes linger
                .before(PhysicsSet),
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

//TODO: change to a gentstate once we have death animations
///Component applied to an entity when its health was depleted
#[derive(Component)]
pub struct Dead;

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
        }
    }
}

pub struct DamageInfo {
    /// Entity that got damaged
    pub entity: Entity,
    /// The tick it was damaged
    pub tick: u64,
    /// Amount of damage that was actually applied
    pub amount: u32,
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
    mut damageable_query: Query<(
        Entity,
        &mut Health,
        &Collider,
        &Gent,
        &GlobalTransform,
        Has<Defense>,
    )>,
    mut gfx_query: Query<Entity, Or<(With<PlayerGfx>, With<EnemyGfx>)>>,
    mut commands: Commands,
    time: Res<GameTime>, //animation query to flash red?
) {
    for (entity, pos, mut attack, attack_collider, maybe_pushback, maybe_projectile) in
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
            Some(entity),
        );
        let intersections_empty = intersections.is_empty();
        let mut targets = intersections
            .into_iter()
            // Filters out everything that's not damageable or one of the nearest max_targets entities to attack
            .filter_map(|colliding_entity| {
                if let Ok((_, _, _, _, dmgbl_pos, _)) = damageable_query.get(colliding_entity) {
                    newly_collided.insert(entity);
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

            let Ok((entity, mut health, collider, gent, dmgbl_trnsfrm, is_defending)) =
                damageable_query.get_mut(*entity)
            else {
                continue;
            };

            attack.damaged_set.insert(entity);

            let damage_dealt = if is_defending {
                attack.damage / 4
            } else {
                attack.damage
            };
            health.current = health.current.saturating_sub(damage_dealt);
            attack.damaged.push(DamageInfo {
                entity,
                tick: time.tick(),
                amount: damage_dealt,
            });
            if let Ok(anim_entity) = gfx_query.get_mut(gent.e_gfx) {
                //insert a DamageFlash to flash for 1 animation frame/8 ticks
                commands.entity(anim_entity).insert(DamageFlash {
                    current_ticks: 0,
                    max_ticks: 8,
                });
            }
            if health.current == 0 {
                commands.entity(entity).insert(Dead);
            }
            if let Some(pushback) = maybe_pushback {
                commands.entity(entity).insert(Knockback::new(
                    pushback.direction,
                    pushback.strength,
                    16,
                ));
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
            damaged_set.clear()
        }

        if maybe_projectile.is_some() && !intersections_empty && attack.current_lifetime > 1 {
            // Note: purposefully does not despawn child entities, nor remove the
            // reference, so that child particle systems have the option of lingering
            commands.entity(entity).despawn();
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

//TODO: change to a gentstate Dying once we have death animations
fn despawn_dead(
    query: Query<(Entity, &Gent, Has<Enemy>), With<Dead>>,
    mut commands: Commands,
    mut kill_count: ResMut<KillCount>,
) {
    for (entity, gent, is_enemy) in query.iter() {
        commands.entity(gent.e_gfx).despawn_recursive();
        commands.entity(entity).despawn_recursive();
        if is_enemy {
            **kill_count += 1;
            println!("{:?}", kill_count);
        }
    }
}
