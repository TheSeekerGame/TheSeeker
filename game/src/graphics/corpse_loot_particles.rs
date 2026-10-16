use super::particles_util::{CircleParticles, color_gradient};
use bevy_hanabi::{EffectAsset, ParticleEffect, SpawnerSettings};

use bevy::prelude::{
    Entity, InheritedVisibility, Name, Transform, ViewVisibility,
    Visibility,
};

use crate::graphics::pixel_perfect::pixel_perfect_layers;
use crate::prelude::{
    App, Assets, Commands, GlobalTransform, Handle, Plugin, ResMut,
    Resource, Startup, StateDespawnMarker, Vec3,
};

const PARTICLE_LIFETIME_SECONDS: f32 = 0.45;
const PARTICLE_SPAWN_RADIUS: f32 = 5.0;
const PARTICLE_SPEED: f32 = 4.0;
const PARTICLE_RATE: f32 = 7.0;
const PARTICLE_SIZE_WORLD_UNITS: f32 = 1.0;

pub struct CorpseLootParticlesPlugin;

#[derive(Resource)]
pub struct CorpseLootParticleEffectHandle(pub Handle<EffectAsset>);

impl Plugin for CorpseLootParticlesPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, setup_corpse_loot_particles);
    }
}

fn setup_corpse_loot_particles(
    mut commands: Commands,
    mut effects: ResMut<Assets<EffectAsset>>,
) {
    let effect = CircleParticles {
        lifetime: PARTICLE_LIFETIME_SECONDS, radius: PARTICLE_SPAWN_RADIUS,
        speed: PARTICLE_SPEED, size: PARTICLE_SIZE_WORLD_UNITS,
        colors: color_gradient([
            (0.0, Vec3::ONE.extend(0.95)),
            (0.7, Vec3::ONE.extend(0.95)),
            (1.0, Vec3::ONE.extend(0.0)),
        ]),
    }.effect(128, SpawnerSettings::rate(PARTICLE_RATE.into())).with_name("corpse_loot_particles");
    let effect = effects.add(effect);

    commands.insert_resource(CorpseLootParticleEffectHandle(effect));
}

pub(crate) fn spawn_corpse_loot_particle_emitter(
    commands: &mut Commands,
    parent: Entity,
    handle: &Handle<EffectAsset>,
) -> Entity {
    let effect_entity = commands
        .spawn((
            Name::new("CorpseLootParticles"),
            super::particle_presentation::PresentedEffect(ParticleEffect::new(handle.clone())),
            Transform::default(),
            GlobalTransform::default(),
            Visibility::Visible,
            InheritedVisibility::VISIBLE,
            ViewVisibility::default(),
            pixel_perfect_layers(),
            StateDespawnMarker,
        ))
        .id();

    commands.entity(parent).add_child(effect_entity);
    effect_entity
}
