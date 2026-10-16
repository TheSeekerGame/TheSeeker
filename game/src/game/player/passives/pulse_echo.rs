//! Pulse Echo: melee strikes release short-range echoes that pierce walls.
use bevy::prelude::*;
use theseeker_engine::physics::{
    AnimationCollider, Collider, LinearVelocity, PhysicsSet, StoredColliderFrame,
};
use theseeker_engine::time::{GameTickUpdate, GameTime};

use crate::game::combat::damage_source::determine_damage_targets;
use crate::game::combat::{DamageSource, Stealthed};
use crate::game::physics::projectile::{arc_projectile, WallPiercingProjectile};
use crate::game::player::states::skill::attacking::player_arrow;
use crate::game::player::weapon::WeaponType;
use crate::game::player::{Passive, Passives};
use crate::graphics::pixel_perfect::pixel_perfect_layers;

const SPEED: f32 = 120.0;
const RANGE: f32 = 24.0;
/// Echoes start this far from the owner along their base direction.
const SPAWN_OFFSET: f32 = 6.0;
const DEFAULT_DAMAGE: f32 = 9.0;
/// Prism fans each echo into three, this far apart.
const PRISM_SPREAD_DEGREES: f32 = 20.0;

pub(crate) struct PulseEchoPlugin;

impl Plugin for PulseEchoPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            GameTickUpdate,
            (
                release_swing_echoes
                    .after(PhysicsSet)
                    .before(determine_damage_targets),
                despawn_spent_echoes.after(arc_projectile),
            ),
        );
    }
}

/// The echoes a strike releases, fixed when the strike starts.
#[derive(Debug, Clone)]
pub(crate) struct PulseEcho {
    damage: f32,
    animation: &'static str,
    prism: bool,
    stealthed: bool,
}

impl PulseEcho {
    /// The echo of an owner's strikes, if they have Pulse Echo.
    pub(crate) fn of(passives: &Passives, damage_mult: f32, stealthed: bool) -> Option<Self> {
        if !passives.contains(&Passive::PulseEcho) {
            return None;
        }
        let animation = if passives.contains(&Passive::FireCrystal) {
            "anim.player.MeleeProjectileFire"
        } else if passives.contains(&Passive::IceCrystal) {
            "anim.player.MeleeProjectileIce"
        } else {
            "anim.player.MeleeProjectile"
        };
        let damage = passives
            .passive_effective_value(Passive::PulseEcho)
            .unwrap_or(DEFAULT_DAMAGE);
        Some(Self {
            damage: damage * damage_mult,
            animation,
            prism: passives.contains(&Passive::Prism),
            stealthed,
        })
    }

    /// Defers the release to the first frame in which a melee swing's hitbox has a shape.
    pub(crate) fn on_swing(self, direction: Vec2) -> SwingEcho {
        SwingEcho { echo: self, direction }
    }

    /// Releases echoes from an owner at `origin`: one along each base direction, or three
    /// with Prism. Echoes inherit the owner's horizontal speed along their path, and
    /// range grows with the inherited speed so their lifetime stays the same.
    pub(crate) fn release(
        &self,
        commands: &mut Commands,
        owner: Entity,
        origin: Vec3,
        owner_velocity: Vec2,
        hz: f64,
        directions: &[Vec2],
    ) {
        // The time to cover the base range at 96 Hz, plus slack; the range check ends
        // most echoes first.
        let lifetime = (RANGE / (SPEED / 96.0)).ceil() as u32 + 2;
        let push = WeaponType::Bow.attack().pushback;
        let echoes = directions.iter().filter_map(|base| base.try_normalize()).flat_map(|base| {
            let start = origin.xy() + base * SPAWN_OFFSET;
            self.fan(base)
                .into_iter()
                .filter_map(Vec2::try_normalize)
                .map(move |direction| (start, direction))
        });
        for (start, direction) in echoes {
            // Only horizontal movement is inherited, so jumps don't bend the path.
            let inherited_speed =
                (Vec2::new(owner_velocity.x, 0.0).dot(direction) * hz as f32).max(0.0);
            let velocity = direction * (SPEED + inherited_speed);
            let rotation = Quat::from_rotation_z(direction.y.atan2(direction.x));
            commands
                .spawn((
                    player_arrow(
                        DamageSource::new(lifetime, owner, self.damage),
                        Transform::from_translation(start.extend(origin.z)).with_rotation(rotation),
                        velocity,
                        push.toward(direction),
                        self.animation,
                    ),
                    WallPiercingProjectile,
                    pixel_perfect_layers(),
                    EchoRange {
                        origin: start,
                        max_range: RANGE * (velocity.length() / SPEED),
                    },
                ))
                .insert_if(Stealthed, || self.stealthed);
        }
    }

    fn fan(&self, direction: Vec2) -> Vec<Vec2> {
        if !self.prism {
            return vec![direction];
        }
        let angle = direction.y.atan2(direction.x);
        let spread = PRISM_SPREAD_DEGREES.to_radians();
        vec![
            Vec2::from_angle(angle),
            Vec2::from_angle(angle + spread),
            Vec2::from_angle(angle - spread),
        ]
    }
}

/// An echo waiting on a melee swing's hitbox.
#[derive(Component)]
pub(crate) struct SwingEcho {
    echo: PulseEcho,
    direction: Vec2,
}

#[derive(Component)]
struct EchoRange {
    origin: Vec2,
    max_range: f32,
}

fn release_swing_echoes(
    mut commands: Commands,
    swings: Query<
        (Entity, &DamageSource, &Collider, &SwingEcho),
        (With<AnimationCollider>, With<StoredColliderFrame>),
    >,
    owners: Query<(&Transform, &LinearVelocity)>,
    time: Res<GameTime>,
) {
    for (hitbox, damage, collider, pending) in &swings {
        if collider.is_empty() {
            continue;
        }
        commands.entity(hitbox).try_remove::<SwingEcho>();
        let Ok((transform, velocity)) = owners.get(damage.owner) else {
            continue;
        };
        pending.echo.release(
            &mut commands,
            damage.owner,
            transform.translation,
            velocity.0,
            time.hz,
            &[pending.direction],
        );
    }
}

fn despawn_spent_echoes(
    mut commands: Commands,
    echoes: Query<(Entity, &Transform, &EchoRange)>,
) {
    for (entity, transform, range) in &echoes {
        if transform.translation.xy().distance_squared(range.origin)
            >= range.max_range * range.max_range
        {
            commands.entity(entity).despawn();
        }
    }
}

#[cfg(test)]
mod tests {
    use bevy::ecs::system::RunSystemOnce;

    use super::*;
    use crate::game::physics::projectile::Projectile;

    #[test]
    fn prism_fans_echoes_that_inherit_forward_speed() {
        let passives = Passives {
            equipped: vec![Passive::PulseEcho, Passive::Prism],
            ..Passives::default()
        };
        let echo = PulseEcho::of(&passives, 2.0, true).expect("Pulse Echo is equipped");
        let mut world = World::new();
        let owner = world.spawn_empty().id();
        world
            .run_system_once(move |mut commands: Commands| {
                let origin = Vec3::new(10.0, 0.0, 5.0);
                echo.release(&mut commands, owner, origin, Vec2::new(0.5, 3.0), 96.0, &[Vec2::X]);
            })
            .expect("release runs");

        let mut echoes = world.query::<(&Transform, &Projectile, &EchoRange, &DamageSource, Has<Stealthed>)>();
        assert_eq!(echoes.iter(&world).count(), 3);
        for (transform, projectile, range, damage, stealthed) in echoes.iter(&world) {
            assert_eq!(transform.translation, Vec3::new(16.0, 0.0, 5.0));
            assert_eq!((damage.owner, damage.max_lifetime, stealthed), (owner, 22, true));
            // Lifetime stays fixed: range scales with the launch speed.
            let speed = projectile.vel.0.length();
            assert!(speed > SPEED);
            assert!((range.max_range - RANGE * speed / SPEED).abs() < 1e-4);
        }
        let straight = echoes
            .iter(&world)
            .find(|(_, projectile, ..)| projectile.vel.0.y.abs() < 1e-4)
            .expect("an echo straight ahead");
        // Half a pixel per tick at 96 Hz adds 48 px/s.
        assert!((straight.1.vel.0.x - 168.0).abs() < 1e-3);
    }
}
