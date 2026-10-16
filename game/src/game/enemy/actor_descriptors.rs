pub(super) use crate::game::actors::projectile::ProjectileCache;
use bevy::ecs::hierarchy::ChildOf;
use bevy::ecs::system::Command;
use theseeker_engine::assets::attack::{
    AttackColliderSource, AttackDescriptor,
};
use theseeker_engine::assets::projectile::{
    ProjectileDescriptor,
};
use theseeker_engine::physics::{AnimationCollider, Collider, LinearVelocity, TileCollisionWorld};

use super::{
    projectiles::{
        can_spawn_sprite_pop_projectile, spawn_enemy_sprite_pop_projectile,
        EnemySpriteProjectileSpawn,
    },
};
use crate::game::combat::{DamageSource, DespawnOnOwnerInvalid};
use crate::game::gentstate::Facing;
use crate::prelude::*;

pub(super) struct EnemyAttackSpawn<'a> {
    pub(super) owner: Entity,
    pub(super) gfx: Entity,
    pub(super) descriptor: &'a AttackDescriptor,
    pub(super) damage: f32,
}

pub(super) struct AttachedEnemyDamageSourceSpawn {
    pub(super) owner: Entity,
    pub(super) damage: f32,
    pub(super) lifetime_ticks: u32,
    pub(super) collider: Collider,
    pub(super) offset: Vec3,
}

struct AttachChildIfAlive {
    child: Entity,
    parent: Entity,
}

impl Command for AttachChildIfAlive {
    type Out = ();
    fn apply(self, world: &mut World) {
        if world.get_entity(self.parent).is_err() {
            despawn_entity_if_present(world, self.child);
            return;
        }

        attach_child_to_parent(world, self.child, self.parent);
    }
}

fn despawn_entity_if_present(world: &mut World, entity: Entity) {
    let Ok(child) = world.get_entity_mut(entity) else {
        return;
    };
    child.despawn();
}

fn attach_child_to_parent(world: &mut World, child: Entity, parent: Entity) {
    let Ok(mut child) = world.get_entity_mut(child) else {
        return;
    };
    child.insert(ChildOf(parent));
}

pub(super) fn attach_child_if_alive(
    commands: &mut Commands,
    child: Entity,
    parent: Entity,
) {
    commands.queue(AttachChildIfAlive { child, parent });
}

pub(super) fn spawn_attached_enemy_damage_source(
    commands: &mut Commands,
    spec: AttachedEnemyDamageSourceSpawn,
) -> Entity {
    let entity = commands
        .spawn((
            spec.collider,
            theseeker_engine::physics::groups::enemy_attack(),
            Transform::from_translation(spec.offset),
            GlobalTransform::default(),
            DespawnOnOwnerInvalid,
            DamageSource::new(
                spec.lifetime_ticks,
                spec.owner,
                spec.damage,
            ),
        ))
        .id();
    attach_child_if_alive(commands, entity, spec.owner);
    entity
}

pub(super) struct FlyingCollision {
    pub(super) normal: Vec2,
}

pub(super) fn move_flying_actor(
    collision_world: Option<&TileCollisionWorld>,
    transform: &mut Transform,
    collider: &Collider,
    velocity: &mut LinearVelocity,
) -> Option<FlyingCollision> {
    if velocity.0.length_squared() <= f32::EPSILON {
        return None;
    }

    let z = transform.translation.z;
    let Some(collision_world) = collision_world else {
        transform.translation =
            (transform.translation.xy() + velocity.0).extend(z);
        return None;
    };

    let half_extents = collider.half_extents();
    let half_extents = Vec2::new(half_extents.x, half_extents.y);
    if half_extents.x <= 0.0 || half_extents.y <= 0.0 {
        transform.translation =
            (transform.translation.xy() + velocity.0).extend(z);
        return None;
    }

    let attempted = velocity.0;
    let result = collision_world.resolve_movement(
        transform.translation.xy(),
        attempted,
        half_extents,
    );

    transform.translation = result.position.extend(z);
    velocity.0 = result.velocity;

    let collided = result.hit_wall || result.grounded || result.hit_ceiling;
    if !collided {
        return None;
    }

    velocity.0 = Vec2::ZERO;
    let normal = if result.hit_wall {
        if attempted.x > 0.0 {
            Vec2::new(-1.0, 0.0)
        } else if attempted.x < 0.0 {
            Vec2::new(1.0, 0.0)
        } else {
            Vec2::ZERO
        }
    } else if result.grounded {
        Vec2::new(0.0, 1.0)
    } else if result.hit_ceiling {
        Vec2::new(0.0, -1.0)
    } else {
        Vec2::ZERO
    };

    Some(FlyingCollision { normal })
}

pub(super) fn move_flying_actor_with_wall_nudge(
    collision_world: Option<&TileCollisionWorld>,
    transform: &mut Transform,
    collider: &Collider,
    velocity: &mut LinearVelocity,
) -> Option<FlyingCollision> {
    let collision =
        move_flying_actor(collision_world, transform, collider, velocity)?;
    if collision.normal != Vec2::ZERO {
        transform.translation += collision.normal.extend(0.0) * 0.5;
    }
    Some(collision)
}

pub(super) fn spawn_enemy_attack_from_descriptor(
    commands: &mut Commands,
    spec: EnemyAttackSpawn<'_>,
) -> Entity {
    let collider = spec.descriptor.collider;
    let attack_entity = spawn_attached_enemy_damage_source(
        commands,
        AttachedEnemyDamageSourceSpawn {
            owner: spec.owner,
            damage: spec.damage,
            lifetime_ticks: spec.descriptor.lifetime_ticks,
            collider: Collider::cuboid(
                spec.descriptor.collider.half_width,
                spec.descriptor.collider.half_height,
            ),
            offset: Vec3::new(
                collider.offset_x,
                collider.offset_y,
                0.0,
            ),
        },
    );

    if collider.source == AttackColliderSource::AnimationPixels {
        commands
            .entity(attack_entity)
            .insert(AnimationCollider(spec.gfx));
    }
    attack_entity
}

pub(super) struct EnemyProjectileSpawn<'a> {
    pub(super) clips: Option<&'a crate::game::actors::action::ProjectileClips>,
    pub(super) owner: Entity,
    pub(super) start: Vec2,
    pub(super) target: Option<Vec2>,
    pub(super) facing: Facing,
    pub(super) descriptor: &'a ProjectileDescriptor,
    pub(super) damage: f32,
    pub(super) particle_effect: Handle<bevy_hanabi::EffectAsset>,
    pub(super) cache: Option<&'a mut ProjectileCache>,
}

pub(super) fn spawn_enemy_projectile_from_descriptor(
    commands: &mut Commands,
    mut spec: EnemyProjectileSpawn<'_>,
) {
    if can_spawn_sprite_pop_projectile(spec.descriptor) {
        spawn_enemy_sprite_pop_projectile(
            commands,
            EnemySpriteProjectileSpawn {
                owner: spec.owner,
                start: spec.start,
                target: spec.target,
                facing: spec.facing,
                descriptor: spec.descriptor,
                clips: spec.clips.expect("resolved sprite projectile clips"),
                damage: spec.damage,
            },
        );
        return;
    }

    crate::game::actors::projectile::emit_projectile_shot(commands,
        crate::game::actors::projectile::ProjectileShot {
            owner: spec.owner, start: spec.start, target: spec.target,
            forward: spec.facing.direction(), descriptor: spec.descriptor, damage: spec.damage,
            particle_effect: Some(spec.particle_effect), split_angle: None,
            policy: crate::game::actors::projectile::ProjectilePolicy {
                groups: theseeker_engine::physics::groups::enemy_attack(), skill: None,
                max_targets: None, descriptor_hit_effects: true, z: 1.0, layers: None, state_lifetime: false,
            },
        }, spec.cache.take());
}
