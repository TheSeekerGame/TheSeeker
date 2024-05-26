use crate::prelude::{
    App, Assets, ChildBuilder, ColorMaterial, Commands, Component, GlobalTransform, Handle, Mesh,
    Parent, Plugin, PushChildren, Rectangle, Res, ResMut, Resource, Startup, Update,
};
use bevy::ecs::system::EntityCommands;
use bevy::prelude::{default, BuildChildren, Color, Entity, Name, Query, Transform};
use bevy::sprite::MaterialMesh2dBundle;
use bevy::utils::smallvec::SmallVec;
use bevy_hanabi::{
    AccelModifier, Attribute, ColorOverLifetimeModifier, EffectAsset, EffectProperties, ExprWriter,
    Gradient, Module, ParticleEffect, ParticleEffectBundle, SetAttributeModifier,
    SetPositionCircleModifier, SetPositionSphereModifier, SetVelocityCircleModifier,
    SetVelocitySphereModifier, ShapeDimension, SizeOverLifetimeModifier, Spawner,
};
use glam::{Vec2, Vec3, Vec4};
use theseeker_engine::prelude::GameTime;

pub struct AttackParticlesPlugin;

impl Plugin for AttackParticlesPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, attack_particles_setup);
        app.add_systems(Update, track_particles_parent);
    }
}

const MAX_LIFETIME: f32 = 5.0;

#[derive(Resource)]
pub struct ArcParticleEffectHandle(pub Handle<EffectAsset>);

/// Tracks how long the parent has been despawned
#[derive(Component)]
pub struct SystemLifetime(f32);

fn attack_particles_setup(
    mut commands: Commands,
    mut effects: ResMut<Assets<EffectAsset>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
) {
    // Spawn a reference white square in the center of the screen at Z=0
    commands
        .spawn(MaterialMesh2dBundle {
            mesh: meshes
                .add(Rectangle {
                    half_size: Vec2::splat(100.0),
                })
                .into(),
            material: materials.add(ColorMaterial {
                color: Color::RED,
                ..Default::default()
            }),
            ..Default::default()
        })
        .insert(Name::new("square"));

    // Create a color gradient for the particles
    let mut gradient = Gradient::new();
    gradient.add_key(0.0, Vec4::new(0.0, 0.0, 1.0, 1.0));
    gradient.add_key(1.0, Vec4::new(1.0, 0.0, 0.0, 0.0));

    let writer = ExprWriter::new();
    let age = writer.lit(0.).expr();
    let init_age = SetAttributeModifier::new(Attribute::AGE, age);

    let lifetime = writer.lit(MAX_LIFETIME).expr();
    let init_lifetime = SetAttributeModifier::new(Attribute::LIFETIME, lifetime);

    let init_pos = SetPositionCircleModifier {
        center: writer.prop("emission_location").expr(),
        axis: writer.lit(Vec3::Z).expr(),
        radius: writer.lit(0.05).expr(),
        dimension: ShapeDimension::Volume,
    };

    let init_vel = SetVelocityCircleModifier {
        center: writer.lit(Vec3::ZERO).expr(),
        axis: writer.lit(Vec3::Z).expr(),
        speed: writer.lit(10.0).expr(),
    };

    // Create a new effect asset spawning 30 particles per second from a circle
    // and slowly fading from blue-ish to transparent over their lifetime.
    // By default the asset spawns the particles at Z=0.
    let spawner = Spawner::rate(300.0.into());
    let effect = effects.add(
        EffectAsset::new(4096, spawner, writer.finish())
            .with_name("2d")
            .init(init_pos)
            .init(init_vel)
            .init(init_age)
            .init(init_lifetime)
            .render(SizeOverLifetimeModifier {
                gradient: Gradient::constant(Vec2::splat(1.0)),
                screen_space_size: false,
            })
            .render(ColorOverLifetimeModifier { gradient })
            .with_property(
                "emission_location",
                Vec3::splat(0.0).into(),
            ),
    );

    commands.insert_resource(ArcParticleEffectHandle(effect.clone()));
    // Spawn an instance of the particle effect, and override its Z layer to
    // be above the reference white square previously spawned.
    commands
        .spawn((ParticleEffectBundle {
            // Assign the Z layer so it appears in the egui inspector and can be modified at runtime
            effect: ParticleEffect::new(effect).with_z_layer_2d(Some(4.5)),
            transform: Transform::from_translation(Vec3::new(100.0, 50.0, 0.0)),
            ..default()
        },))
        .insert(Name::new("effect:2d"));

    println!("spawned particle system");
}

impl crate::graphics::particles_util::BuildParticles for EntityCommands<'_> {
    /// Attaches a particle bundle as a child entity to the entity being spawned
    /// When the parent entity is despawned, the particle bundles particles will
    /// stop emitting, and linger for [`MAX_LIFETIME`] seconds
    fn with_lingering_particles(&mut self, handle: Handle<EffectAsset>) -> &mut Self {
        self.with_children(|builder| {
            builder
                .spawn((
                    ParticleEffectBundle {
                        // Assign the Z layer so it appears in the egui inspector and can be modified at runtime
                        effect: ParticleEffect::new(handle.clone()).with_z_layer_2d(Some(100.0)),
                        ..default()
                    },
                    SystemLifetime(MAX_LIFETIME),
                    EffectProperties::default(),
                ))
                .insert(Name::new("projectile_particles"));
        })
    }
}

fn track_particles_parent(
    time: Res<GameTime>,
    q_parent: Query<&GlobalTransform>,
    mut query: Query<(
        Entity,
        &Parent,
        &mut EffectProperties,
        //&mut SystemLifetime,
    )>,
    mut commands: Commands,
) {
    for (entity, parent, mut effect) in &mut query {
        if q_parent.get(**parent).is_err() {
            commands.entity(entity).remove_parent();
            // sets emission location far far away so emission appears to have stopped
            effect.set(
                "emission_location",
                Vec3::new(1000000.0, 0.0, 0.0).into(),
            );
        }
    }
}
