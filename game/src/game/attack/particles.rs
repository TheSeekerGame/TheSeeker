use bevy::ecs::system::EntityCommands;
use bevy::prelude::{default, BuildChildren, Entity, Name, Query, Transform, Without};
use bevy::time::Time;
use bevy_hanabi::{
    Attribute, ColorOverLifetimeModifier, EffectAsset, EffectProperties,
    ExprWriter, Gradient, ParticleEffect, ParticleEffectBundle,
    SetAttributeModifier, SetPositionCircleModifier, ShapeDimension,
    SizeOverLifetimeModifier, Spawner,
};
use glam::{Vec2, Vec3, Vec4};
use theseeker_engine::prelude::{GameTickUpdate, GameTime};

use crate::game::attack::arc_attack::Projectile;
use crate::prelude::{
    App, Assets, Commands, Component, GlobalTransform, Handle, Parent, Plugin,
    Res, ResMut, Resource, Startup, Update,
};

#[derive(Component)]
pub struct EllipticalMotion {
    width: f32,  // Horizontal semi-major axis
    height: f32, // Vertical semi-minor axis
    speed: f32,  // Angular speed in radians per second
    pub angle: f32, // Current angle in radians
    pub offset: Vec3, // Offset to apply to the position
}

impl EllipticalMotion {
    pub fn new(width: f32, height: f32, speed: f32, offset: Vec3) -> Self {
        Self {
            width,
            height,
            speed,
            angle: 0.0,
            offset,
        }
    }
}

fn elliptical_motion_system(
    time: Res<Time>,
    mut query: Query<(&mut Transform, &mut EllipticalMotion)>,
) {
    for (mut transform, mut motion) in query.iter_mut() {
        // Update the angle based on time and speed
        motion.angle += motion.speed * time.delta_seconds();

        // Calculate new position using elliptical path formula
        let x = motion.width * motion.angle.cos();  // Using width as the semi-major axis
        let y = motion.height * motion.angle.sin(); // Using height as the semi-minor axis

        // Apply the offset to the calculated position
        transform.translation.x = x + motion.offset.x;
        transform.translation.y = y + motion.offset.y;
        transform.translation.z = motion.offset.z; // Optionally offset in the z-axis too
    }
}

pub struct AttackParticlesPlugin;

impl Plugin for AttackParticlesPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, (attack_particles_setup, stun_particles_setup));
        app.add_systems(Update, (track_particles_parent, elliptical_motion_system));
        app.add_systems(GameTickUpdate, despawn_lingering);
        app.add_systems(GameTickUpdate, update_velocity);
    }
}

const MAX_LIFETIME: f32 = 0.2;

#[derive(Resource)]
pub struct ArcParticleEffectHandle(pub Handle<EffectAsset>);

#[derive(Resource)]
pub struct StarParticleEffectHandle(pub Handle<EffectAsset>);

/// Tracks how long the parent has been despawned
#[derive(Component)]
pub struct SystemLifetime(f32);

fn stun_particles_setup(
    mut commands: Commands,
    mut effects: ResMut<Assets<EffectAsset>>,
) {
    // Create a color gradient for the particles
    let mut gradient = Gradient::new();
    let r = 1.0 / 8.0;
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

    let lifetime = writer.lit(1.0).expr();
    let init_lifetime =
        SetAttributeModifier::new(Attribute::LIFETIME, lifetime);

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
  
    let pos = writer.add_property("head_pos", Vec3::ZERO.into());
    let pos = writer.prop(pos);

    let modded_pos = SetAttributeModifier {
        attribute: Attribute::POSITION,
        value: pos.expr(),
    };

    /* 
    let init_vel = SetAttributeModifier {
        attribute: Attribute::VELOCITY,
        value: (writer.prop(dir) * writer.lit(-0.5)
            - particle_pos * writer.lit(5.5))
        .expr(),
    };
    */

    // Create a new effect asset spawning 30 particles per second from a circle
    // and slowly fading from blue-ish to transparent over their lifetime.
    // By default the asset spawns the particles at Z=0.
    let spawner = Spawner::rate(1000.0.into());
    let effect = effects.add(
        EffectAsset::new(vec![3], spawner, writer.finish())
            .with_trails(5000 * 5, 1.0 / 8.0, 1.0, 0)
            .with_name("2d")
            .init(init_pos)
            .init(modded_pos)
            //.init(init_vel)
            .init(init_age)
            .init(init_lifetime)
            .render(SizeOverLifetimeModifier {
                gradient: Gradient::constant(Vec2::splat(1.0)),
                screen_space_size: false,
            })
            .render(ColorOverLifetimeModifier { gradient })
        //.with_alpha_mode(AlphaMode::Mask(ExprWriter::new().lit(0.001).expr())),
    );

    commands.insert_resource(StarParticleEffectHandle(effect.clone()));
}

fn attack_particles_setup(
    mut commands: Commands,
    mut effects: ResMut<Assets<EffectAsset>>,
) {
    // Create a color gradient for the particles
    let mut gradient = Gradient::new();
    let r = 1.0 / 8.0;
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
    let init_lifetime =
        SetAttributeModifier::new(Attribute::LIFETIME, lifetime);

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
        + writer.prop(dir)
            * projection
            * (writer.prop(speed) - writer.lit(1.0));

    let modded_pos = SetAttributeModifier {
        attribute: Attribute::POSITION,
        value: skewed_pos.expr(),
    };

    let init_vel = SetAttributeModifier {
        attribute: Attribute::VELOCITY,
        value: (writer.prop(dir) * writer.lit(-0.5)
            - particle_pos * writer.lit(5.5))
        .expr(),
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
            .render(ColorOverLifetimeModifier { gradient })
        //.with_alpha_mode(AlphaMode::Mask(ExprWriter::new().lit(0.001).expr())),
    );

    commands.insert_resource(ArcParticleEffectHandle(effect.clone()));
}

impl crate::graphics::particles_util::BuildParticles for EntityCommands<'_> {
    /// Attaches a particle bundle as a child entity to the entity being spawned
    /// When the parent entity is despawned, the particle bundles particles will
    /// stop emitting, and linger for [`MAX_LIFETIME`] seconds
    fn with_lingering_particles(
        &mut self,
        handle: Handle<EffectAsset>,
    ) -> &mut Self {
        self.with_children(|builder| {
            builder
                .spawn((
                    ParticleEffectBundle {
                        // Assign the Z layer so it appears in the egui inspector and can be modified at runtime
                        effect: ParticleEffect::new(handle.clone()),
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
    for (entity, mut lifetime) in &mut query {
        lifetime.0 -= 1.0 / time.hz as f32;
        if lifetime.0 < 0.0 {
            commands.entity(entity).despawn();
        }
    }
}
