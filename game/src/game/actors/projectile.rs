//! Projectile emission shared by action owners; policy carries combat attribution.

use bevy::prelude::*;
use bevy::camera::visibility::RenderLayers;
use theseeker_engine::assets::projectile::{ProjectileDescriptor, ProjectileHitEffect};
use theseeker_engine::physics::{Collider, CollisionGroups, LinearVelocity};
use crate::game::combat::damage_source::{DamageSource, DamageSourceSkill};
use crate::game::effects::chilled::ChilledEffect;
use crate::game::physics::projectile::{GroundDespawnProjectile, Projectile};
use crate::game::player::skills::types::SkillId;
use crate::graphics::projectile_particles::projectile_particles;
use crate::prelude::StateDespawnMarker;

#[derive(Component, Default)]
pub(crate) struct ProjectileCache {
    velocity: Option<LinearVelocity>,
    cached_start: Vec2,
    cached_player_pos: Vec2,
    cached_gravity: f32,
    position_tolerance: f32,
}

impl ProjectileCache {
    pub(crate) fn new() -> Self {
        Self {
            velocity: None,
            cached_start: Vec2::ZERO,
            cached_player_pos: Vec2::ZERO,
            cached_gravity: 0.0,
            position_tolerance: 6.0,
        }
    }

    fn is_valid(
        &self,
        current_start: Vec2,
        current_player_pos: Vec2,
        current_gravity: f32,
    ) -> bool {
        self.velocity.is_some()
            && self.cached_start.distance(current_start) < self.position_tolerance
            && self.cached_player_pos.distance(current_player_pos)
                < self.position_tolerance
            && (self.cached_gravity - current_gravity).abs() < 0.1
    }

}

pub struct ProjectilePolicy {
    pub groups: CollisionGroups,
    pub skill: Option<SkillId>,
    pub max_targets: Option<u32>,
    pub descriptor_hit_effects: bool,
    pub z: f32,
    pub layers: Option<RenderLayers>,
    pub state_lifetime: bool,
}

pub struct ProjectileShot<'a> {
    pub owner: Entity,
    pub start: Vec2,
    pub target: Option<Vec2>,
    pub forward: f32,
    pub descriptor: &'a ProjectileDescriptor,
    pub damage: f32,
    pub particle_effect: Option<Handle<bevy_hanabi::EffectAsset>>,
    pub policy: ProjectilePolicy,
    pub split_angle: Option<f32>,
}

pub(crate) fn emit_projectile_shot(commands: &mut Commands, shot: ProjectileShot<'_>, cache: Option<&mut ProjectileCache>) {
    let velocity = resolve_shot_velocity(&shot, cache);
    spawn_projectile(commands, &shot, velocity);
    if let Some(angle) = shot.split_angle {
        spawn_projectile(commands, &shot, LinearVelocity(Vec2::from_angle(angle).rotate(velocity.0)));
        spawn_projectile(commands, &shot, LinearVelocity(Vec2::from_angle(-angle).rotate(velocity.0)));
    }
}

fn resolve_shot_velocity(shot: &ProjectileShot<'_>, cache: Option<&mut ProjectileCache>) -> LinearVelocity {
    let gravity = shot.descriptor.movement.gravity;
    let target = shot.target.unwrap_or(shot.start);
    let cached = cache.as_ref().filter(|cache| cache.is_valid(shot.start, target, gravity))
        .and_then(|cache| cache.velocity);
    if let Some(velocity) = cached.filter(|_| shot.target.is_some()) {
        return velocity;
    }
    let velocity = shot.target.map(|target| solve_projectile_velocity(shot.descriptor, shot.start, target))
        .unwrap_or_else(|| fallback_projectile_velocity(shot.descriptor, shot.forward));
    store_projectile_cache(cache, velocity, shot.start, target, gravity);
    velocity
}

/// A ballistic arc to `target`, raising the launch speed until one exists.
fn solve_projectile_velocity(descriptor: &ProjectileDescriptor, start: Vec2, target: Vec2) -> LinearVelocity {
    let movement = descriptor.movement;
    let mut launch_speed = movement.initial_speed;
    for _ in 0..movement.speed_attempts {
        if let Some(projectile) = Projectile::with_vel(target, start, launch_speed, movement.gravity) {
            return projectile.vel;
        }
        launch_speed *= movement.speed_multiplier;
    }
    fallback_projectile_velocity(descriptor, (target - start).x.signum())
}

fn fallback_projectile_velocity(descriptor: &ProjectileDescriptor, direction_x: f32) -> LinearVelocity {
    let movement = descriptor.movement;
    LinearVelocity(Vec2::new(movement.fallback_x_speed * direction_x.signum(), movement.fallback_y_speed))
}

fn shot_damage_source(shot: &ProjectileShot<'_>) -> DamageSource {
    let mut damage = DamageSource::new(shot.descriptor.lifetime_ticks, shot.owner, shot.damage);
    if let Some(max_targets) = shot.policy.max_targets {
        damage = damage.with_max_targets(max_targets);
    }
    if !shot.policy.descriptor_hit_effects {
        return damage;
    }
    for effect in &shot.descriptor.hit_effects {
        let ProjectileHitEffect::Chilled { slow_multiplier, duration_ticks } = *effect;
        damage = damage.with_chilled_effect(ChilledEffect::new(duration_ticks, slow_multiplier));
    }
    damage
}

fn spawn_projectile(commands: &mut Commands, shot: &ProjectileShot<'_>, velocity: LinearVelocity) {
    let mut entity = commands.spawn((
        shot_damage_source(shot), Projectile { vel: velocity },
        Collider::cuboid(shot.descriptor.collider.half_width, shot.descriptor.collider.half_height),
        shot.policy.groups, Transform::from_translation(shot.start.extend(shot.policy.z)),
        GlobalTransform::default(), Visibility::Visible, InheritedVisibility::VISIBLE, ViewVisibility::default(),
    ));
    if shot.descriptor.ground_despawn { entity.insert(GroundDespawnProjectile); }
    if let Some(skill) = shot.policy.skill { entity.insert(DamageSourceSkill(skill)); }
    if let Some(layers) = &shot.policy.layers { entity.insert(layers.clone()); }
    if shot.policy.state_lifetime { entity.insert(StateDespawnMarker); }
    if let Some(effect) = &shot.particle_effect { entity.with_child(projectile_particles(effect.clone())); }
}

fn store_projectile_cache(
    cache: Option<&mut ProjectileCache>,
    velocity: LinearVelocity,
    start: Vec2,
    target: Vec2,
    gravity: f32,
) {
    let Some(cache) = cache else {
        return;
    };

    cache.velocity = Some(velocity);
    cache.cached_start = start;
    cache.cached_player_pos = target;
    cache.cached_gravity = gravity;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fallback_velocity_uses_descriptor_values() {
        let descriptor = ProjectileDescriptor::standard_ball_defaults();
        let velocity = fallback_projectile_velocity(&descriptor, -1.0);
        assert_eq!(velocity.0, Vec2::new(-134.0, 151.0));
    }

    #[test]
    fn ballistic_velocity_falls_back_when_no_arc_solution_exists() {
        let mut descriptor = ProjectileDescriptor::standard_ball_defaults();
        descriptor.movement.speed_attempts = 1;
        descriptor.movement.initial_speed = 1.0;

        let velocity = solve_projectile_velocity(&descriptor, Vec2::ZERO, Vec2::new(100.0, 0.0));
        assert_eq!(velocity.0, Vec2::new(134.0, 151.0));
    }

    #[test]
    fn projectile_cache_is_tied_to_start_and_target() {
        let mut cache = ProjectileCache::new();
        cache.velocity = Some(LinearVelocity(Vec2::new(1.0, 2.0)));
        cache.cached_start = Vec2::new(10.0, 0.0);
        cache.cached_player_pos = Vec2::new(20.0, 0.0);
        cache.cached_gravity = 432.0;

        assert!(cache.is_valid(
            Vec2::new(12.0, 0.0),
            Vec2::new(23.0, 0.0),
            432.05
        ));
        assert!(!cache.is_valid(
            Vec2::new(17.0, 0.0),
            Vec2::new(20.0, 0.0),
            432.0
        ));
        assert!(!cache.is_valid(
            Vec2::new(10.0, 0.0),
            Vec2::new(27.0, 0.0),
            432.0
        ));
    }
}
