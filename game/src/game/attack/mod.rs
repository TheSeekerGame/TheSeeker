pub mod arc_attack;
mod particles;

use theseeker_engine::assets::animation::SpriteAnimation;
use theseeker_engine::gent::Gent;
use theseeker_engine::physics::{
    update_sprite_colliders, Collider, LinearVelocity, PhysicsSet, PhysicsWorld, GROUND,
};
use theseeker_engine::script::ScriptPlayer;

use super::enemy::EnemyGfx;
use super::player::PlayerGfx;
use crate::game::attack::arc_attack::{arc_projectile, Projectile};
use crate::game::attack::particles::attack_particles_setup;
use crate::game::enemy::{Defense, EnemyStateSet};
use crate::game::player::PlayerStateSet;
use crate::prelude::*;

pub struct AttackPlugin;

impl Plugin for AttackPlugin {
    fn build(&self, app: &mut App) {
        //app.add_systems(Startup, attack_particles_setup);
        app.add_systems(
            OnEnter(AppState::InGame),
            attack_particles_setup,
        );
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
                // TODO: unify statesets?
                .after(PlayerStateSet::Transition)
                .after(EnemyStateSet::Transition)
                // has to be before physics set or colliders sometimes linger
                .before(PhysicsSet),
        );
    }
}
#[derive(Component)]
pub struct Health {
    pub current: u32,
    pub max: u32,
}

#[derive(Component)]
pub struct DamageFlash {
    pub current_ticks: u32,
    pub max_ticks: u32,
}

// TODO: change to a gentstate once we have death animations
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
    pub attacker: Entity,
    /// (entity that got damaged, tick it was damaged, damage actually applied)
    pub damaged: Vec<(Entity, u64, u32)>,
}
impl Attack {
    /// Lifetime is in game ticks
    pub fn new(lifetime: u32, attacker: Entity) -> Self {
        Attack {
            current_lifetime: 0,
            max_lifetime: lifetime,
            damage: 20,
            attacker,
            damaged: Vec::new(),
        }
    }
}

// Component added to attack entity to indicate it causes knockback
#[derive(Component, Default)]
pub struct Pushback {
    pub direction: f32,
    pub strength: f32,
}

// Component added to an entity damaged by a pushback attack
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
        Has<Defense>,
    )>,
    mut gfx_query: Query<
        (
            Entity,
            &mut ScriptPlayer<SpriteAnimation>,
        ),
        Or<(With<PlayerGfx>, With<EnemyGfx>)>,
    >,
    mut commands: Commands,
    time: Res<GameTime>, // animation query to flash red?
) {
    for (entity, pos, mut attack, attack_collider, maybe_pushback, maybe_projectile) in
        query.iter_mut()
    {
        let colliding_entities = spatial_query.intersect(
            pos.translation().xy(),
            attack_collider.0.shape(),
            attack_collider
                .0
                .collision_groups()
                .with_filter(attack_collider.0.collision_groups().filter | GROUND),
            Some(entity),
        );
        for (entity, mut health, collider, gent, is_defending) in damageable_query.iter_mut() {
            if colliding_entities.contains(&entity)
                && attack.damaged.iter().find(|x| x.0 == entity).is_none()
            {
                let damage_dealt = if is_defending {
                    attack.damage / 4
                } else {
                    attack.damage
                };
                health.current = health.current.saturating_sub(damage_dealt);
                attack.damaged.push((entity, time.tick(), damage_dealt));
                if let Ok((anim_entity, mut anim_player)) = gfx_query.get_mut(gent.e_gfx) {
                    // is there any way to check if a slot is set?
                    anim_player.set_slot("Damage", true);
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
        }
        if maybe_projectile.is_some()
            && !colliding_entities.is_empty()
            && attack.current_lifetime > 1
        {
            commands.entity(entity).despawn();
        }
    }
}

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
            velocity.x = knockback.direction * knockback.strength;
        }
        if knockback.ticks > knockback.max_ticks {
            velocity.x = 0.;
            commands.entity(entity).remove::<Knockback>();
        }
    }
}

fn damage_flash(
    mut query: Query<(
        Entity,
        &mut DamageFlash,
        &mut ScriptPlayer<SpriteAnimation>,
    )>,
    mut commands: Commands,
) {
    for (entity, mut damage_flash, mut anim_player) in query.iter_mut() {
        if damage_flash.current_ticks == damage_flash.max_ticks {
            commands.entity(entity).remove::<DamageFlash>();
            anim_player.set_slot("Damage", false);
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

// TODO: change to a gentstate Dying once we have death animations
fn despawn_dead(query: Query<(Entity, &Gent), With<Dead>>, mut commands: Commands) {
    for (entity, gent) in query.iter() {
        commands.entity(gent.e_gfx).despawn_recursive();
        commands.entity(entity).despawn_recursive();
    }
}
