use bevy::prelude::*;
use crate::game::combat::DamageSource;
use crate::game::physics::projectile::Projectile;
use crate::graphics::projectile_particles::ArcParticleEffectHandle;
use crate::game::combat::Health;
use crate::graphics::particles_util::BuildParticles;
use theseeker_engine::physics::{Collider, groups, LinearVelocity};

/// Spawn a basic arcing projectile attack for an enemy.
/// Uses a ballistic helper (`Projectile::with_vel`) with fixed gravity (px/s²).
pub fn spawn_enemy_attack(
    commands: &mut Commands,
    enemy_entity: Entity,
    enemy_pos: Vec2,
    target_pos: Vec2,
    damage: f32,
    particles: Option<&ArcParticleEffectHandle>,
) {
    let projectile = Projectile::with_vel(
        target_pos,
        enemy_pos,
        300.0,
        432.0, // gravity in px/s²
    );
    
    if let Some(projectile) = projectile {
        let mut e_commands = commands.spawn((
            DamageSource::new(192, enemy_entity, damage),
            projectile,
            Collider::circle(2.0),
            groups::enemy_attack(),
            Transform::from_translation(enemy_pos.extend(0.0)),
            GlobalTransform::default(),
        ));
        
        if let Some(particles) = particles {
            e_commands.with_lingering_particles(particles.0.clone());
        }
    }
}