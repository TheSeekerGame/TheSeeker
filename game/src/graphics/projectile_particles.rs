//! The particle trail of projectiles, stretched along their flight.
use bevy_hanabi::{
    Attribute, ColorOverLifetimeModifier, EffectAsset, EffectProperties, ExprWriter, Gradient, ParticleEffect,
    SetAttributeModifier, SetPositionCircleModifier, ShapeDimension, SizeOverLifetimeModifier, SpawnerSettings,
};

use crate::game::physics::projectile::Projectile;
use crate::graphics::particle_presentation::PresentedEffect;
use crate::graphics::pixel_perfect::pixel_perfect_layers;
use crate::prelude::*;

pub struct AttackParticlesPlugin;

impl Plugin for AttackParticlesPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, attack_particles_setup).add_systems(GameTickUpdate, update_velocity);
    }
}

const MAX_LIFETIME: f32 = 0.2;

#[derive(Resource)]
pub struct ArcParticleEffectHandle(pub Handle<EffectAsset>);

/// A projectile's particle emitter, spawned as the projectile's child on the pixel-perfect
/// layer. Despawning the projectile removes the emitter; its render proxy survives until the
/// presentation cursor reaches that death tick, without a further particle tail.
pub(crate) fn projectile_particles(effect: Handle<EffectAsset>) -> impl Bundle {
    (
        PresentedEffect(ParticleEffect::new(effect)),
        EffectProperties::default(),
        Transform::default(),
        Visibility::Visible,
        pixel_perfect_layers(),
        Name::new("projectile_particles"),
    )
}

fn attack_particles_setup(mut commands: Commands, mut effects: ResMut<Assets<EffectAsset>>) {
    let r = 1.0 / 8.0;
    let gradient = super::particles_util::color_gradient([
        (r * 0.0, Vec4::new(0.643, 0.753, 0.773, 1.0)),
        (r * 3.0, Vec4::new(0.643, 0.753, 0.773, 1.)),
        (r * 3.0, Vec4::new(0.761, 0.827, 0.851, 1.)),
        (r * 4.0, Vec4::new(0.761, 0.827, 0.851, 1.)),
        (r * 4.0, Vec4::new(0.925, 0.965, 0.98, 1.0)),
        (r * 6.0, Vec4::new(0.925, 0.965, 0.98, 1.0)),
        (r * 6.0, Vec4::new(0.761, 0.827, 0.851, 1.)),
        (r * 7.0, Vec4::new(0.761, 0.827, 0.851, 1.)),
        (r * 8.0, Vec4::new(0.761, 0.827, 0.851, 0.)),
    ]);

    let writer = ExprWriter::new();
    let init_age = SetAttributeModifier::new(Attribute::AGE, writer.lit(0.).expr());
    let init_lifetime = SetAttributeModifier::new(Attribute::LIFETIME, writer.lit(MAX_LIFETIME).expr());
    // The projectile's flight direction, and a stretch that grows with its speed.
    let dir = writer.add_property("dir", Vec3::ZERO.into());
    let speed = writer.add_property("speed", 1.0.into());

    let init_pos = SetPositionCircleModifier {
        center: writer.lit(Vec3::ZERO).expr(),
        axis: writer.lit(Vec3::Z).expr(),
        radius: writer.lit(2.5).expr(),
        dimension: ShapeDimension::Volume,
    };
    // Stretches the spawn disc along the flight direction.
    let position = writer.attr(Attribute::POSITION);
    let along = position.clone().dot(writer.prop(dir));
    let stretched = position.clone() + writer.prop(dir) * along * (writer.prop(speed) - writer.lit(1.0));
    let stretch = SetAttributeModifier { attribute: Attribute::POSITION, value: stretched.expr() };
    // Particles trail back from the flight and draw in toward its line.
    let init_vel = SetAttributeModifier {
        attribute: Attribute::VELOCITY,
        value: (writer.prop(dir) * writer.lit(-0.5) - position * writer.lit(5.5)).expr(),
    };

    let effect = EffectAsset::new(4096, SpawnerSettings::rate(1000.0.into()), writer.finish())
        .with_name("2d")
        .init(init_pos)
        .init(stretch)
        .init(init_vel)
        .init(init_age)
        .init(init_lifetime)
        .render(SizeOverLifetimeModifier { gradient: Gradient::constant(Vec3::ONE), screen_space_size: false })
        .render(ColorOverLifetimeModifier::new(gradient));
    commands.insert_resource(ArcParticleEffectHandle(effects.add(effect)));
}

/// Points each projectile's trail along its flight, stretched with its speed.
fn update_velocity(projectiles: Query<&Projectile>, mut emitters: Query<(&ChildOf, &mut EffectProperties)>) {
    for (child_of, mut properties) in &mut emitters {
        if let Ok(projectile) = projectiles.get(child_of.parent()) {
            let velocity = projectile.vel.0;
            properties.set("dir", velocity.normalize_or_zero().extend(0.0).into());
            properties.set("speed", (velocity.length() * 0.005 + 1.0).into());
        }
    }
}
