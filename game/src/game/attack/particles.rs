use crate::game::attack::arc_attack::Projectile;
use crate::prelude::{
    App, Assets, ChildBuilder, ColorMaterial, Commands, Component, GlobalTransform, Handle, Mesh,
    Parent, Plugin, PushChildren, Rectangle, Res, ResMut, Resource, Startup, Update,
};
use bevy::ecs::system::EntityCommands;
use bevy::prelude::{default, BuildChildren, Color, Entity, Name, Query, Transform, Without};
use bevy::sprite::MaterialMesh2dBundle;
use bevy::utils::smallvec::SmallVec;
use bevy_hanabi::{
    AccelModifier, Attribute, ColorOverLifetimeModifier, EffectAsset, EffectProperties, ExprWriter,
    Gradient, Module, ParticleEffect, ParticleEffectBundle, SetAttributeModifier,
    SetPositionCircleModifier, SetPositionSphereModifier, SetVelocityCircleModifier,
    SetVelocitySphereModifier, ShapeDimension, SizeOverLifetimeModifier, Spawner, Value,
};
use glam::{Vec2, Vec2Swizzles, Vec3, Vec4};
use theseeker_engine::physics::LinearVelocity;
use theseeker_engine::prelude::{GameTickUpdate, GameTime};

pub struct AttackParticlesPlugin;

impl Plugin for AttackParticlesPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, attack_particles_setup);
        app.add_systems(Update, track_particles_parent);
        app.add_systems(GameTickUpdate, despawn_lingering);
        app.add_systems(GameTickUpdate, update_velocity);
    }
}

const MAX_LIFETIME: f32 = 0.2;

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
    let r = (1.0 / 8.0);
    // set w to 5 on the first one for fun
    gradient.add_key(
        r * 0.0,
        Vec4::new(0.643, 0.753, 0.773, 1.0),
    );
    gradient.add_key(
        r * 3.0,
        Vec4::new(0.643, 0.753, 0.773, 1.),
    );
    gradient.add_key(
        r * 3.0,
        Vec4::new(0.761, 0.827, 0.851, 1.),
    );
    gradient.add_key(
        r * 4.0,
        Vec4::new(0.761, 0.827, 0.851, 1.),
    );
    gradient.add_key(
        r * 4.0,
        Vec4::new(0.925, 0.965, 0.98, 1.0),
    );
    gradient.add_key(
        r * 6.0,
        Vec4::new(0.925, 0.965, 0.98, 1.0),
    );
    gradient.add_key(
        r * 6.0,
        Vec4::new(0.761, 0.827, 0.851, 1.),
    );
    gradient.add_key(
        r * 7.0,
        Vec4::new(0.761, 0.827, 0.851, 1.),
    );
    gradient.add_key(
        r * 8.0,
        Vec4::new(0.761, 0.827, 0.851, 0.),
    );

    let writer = ExprWriter::new();
    let age = writer.lit(0.).expr();
    let init_age = SetAttributeModifier::new(Attribute::AGE, age);

    let lifetime = writer.lit(MAX_LIFETIME).expr();
    let init_lifetime = SetAttributeModifier::new(Attribute::LIFETIME, lifetime);

    let emit_loc = writer.add_property("emit_loc", Vec3::splat(0.0).into());
    let dir = writer.add_property("dir", Vec3::splat(0.0).into());
    let speed = writer.add_property("speed", 1.0.into());

    let init_pos = SetPositionCircleModifier {
        center: writer.prop(emit_loc).expr(),
        axis: writer.lit(Vec3::Z).expr(),
        radius: writer.lit(2.5).expr(),
        dimension: ShapeDimension::Volume,
    };

    // makes the particles spawn aligned with the direction the projectile is moving
    let particle_pos = writer.attr(Attribute::POSITION) - writer.prop(emit_loc);
    let projection = particle_pos.clone().dot(writer.prop(dir));
    let skewed_pos = particle_pos.clone()
        + writer.prop(dir) * projection * (writer.prop(speed) - writer.lit(1.0));

    let modded_pos = SetAttributeModifier {
        attribute: Attribute::POSITION,
        value: skewed_pos.expr(),
    };

    let init_vel = SetAttributeModifier {
        attribute: Attribute::VELOCITY,
        value: (writer.prop(dir) * writer.lit(-0.5) - particle_pos * writer.lit(5.5)).expr(),
    };

    // Create a new effect asset spawning 30 particles per second from a circle
    // and slowly fading from blue-ish to transparent over their lifetime.
    // By default the asset spawns the particles at Z=0.
    let spawner = Spawner::rate(1000.0.into());
    let effect = effects.add(
        EffectAsset::new(vec![4096], spawner, writer.finish())
            .with_name("2d")
            .init(init_pos)
            .init(modded_pos)
            .init(init_vel)
            .init(init_age)
            .init(init_lifetime)
            .render(SizeOverLifetimeModifier {
                gradient: Gradient::constant(Vec2::splat(1.0)),
                screen_space_size: false,
            })
            .render(ColorOverLifetimeModifier { gradient }),
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
                ))
                .insert(Name::new("projectile_particles"));
        })
    }
}

fn update_velocity(
    q_parent: Query<&Projectile>,
    mut query: Query<(&Parent, &mut EffectProperties)>,
) {
    for (parent, mut effect) in &mut query {
        if let Ok(vel) = q_parent.get(**parent) {
            // sets emission location far far away so emission appears to have stopped
            effect.set(
                "dir",
                vel.vel.0.normalize_or_zero().extend(0.0).into(),
            );
            effect.set(
                "speed",
                (vel.vel.0.length() * 0.005 + 1.0).into(),
            );
        }
    }
}

fn track_particles_parent(
    q_parent: Query<&GlobalTransform>,
    mut query: Query<(Entity, &Parent, &mut EffectProperties)>,
    mut commands: Commands,
) {
    for (entity, parent, mut effect) in &mut query {
        if q_parent.get(**parent).is_err() {
            commands.entity(entity).remove_parent();
            // sets emission location far far away so emission appears to have stopped
            effect.set(
                "emit_loc",
                Vec3::new(1000000.0, 0.0, 0.0).into(),
            );
        }
    }
}

fn despawn_lingering(
    time: Res<GameTime>,
    mut query: Query<(Entity, &mut SystemLifetime), Without<Parent>>,
    mut commands: Commands,
) {
    for ((entity, mut lifetime)) in &mut query {
        lifetime.0 -= 1.0 / time.hz as f32;
        if lifetime.0 < 0.0 {
            commands.entity(entity).despawn();
        }
    }
}
