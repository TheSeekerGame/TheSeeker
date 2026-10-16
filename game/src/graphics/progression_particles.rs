use super::particles_util::{CircleParticles, color_gradient};
use bevy::camera::visibility::RenderLayers;
use bevy_hanabi::{EffectAsset, ParticleEffect, SimulationSpace, SpawnerSettings};

use crate::prelude::*;

const BURST_PARTICLE_COUNT: f32 = 28.0;
const BURST_PARTICLE_LIFETIME: f32 = 0.42;
const BURST_PARTICLE_RADIUS: f32 = 2.5;
const BURST_PARTICLE_SPEED: f32 = 36.0;
const BURST_PARTICLE_SIZE: f32 = 1.35;
const BURST_ENTITY_LIFETIME_TICKS: u32 = 48;

pub struct ProgressionParticlesPlugin;

#[derive(Resource)]
pub struct ProgressionBurstParticleEffectHandle(pub Handle<EffectAsset>);

#[derive(Component, Debug, Clone, Copy)]
struct ProgressionBurstLifetime {
    remaining_ticks: u32,
}

impl Plugin for ProgressionParticlesPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, setup_progression_particles);
        app.add_systems(GameTickUpdate, tick_progression_burst_lifetime);
    }
}

fn setup_progression_particles(
    mut commands: Commands,
    mut effects: ResMut<Assets<EffectAsset>>,
) {
    let effect = CircleParticles {
        lifetime: BURST_PARTICLE_LIFETIME, radius: BURST_PARTICLE_RADIUS,
        speed: BURST_PARTICLE_SPEED, size: BURST_PARTICLE_SIZE,
        colors: color_gradient([
            (0.0, Vec4::new(1.0, 0.98, 0.82, 1.0)),
            (0.45, Vec4::new(0.96, 0.8, 0.36, 0.95)),
            (1.0, Vec4::new(1.0, 0.84, 0.28, 0.0)),
        ]),
    }.effect(256, SpawnerSettings::once(BURST_PARTICLE_COUNT.into())).with_name("progression_burst_particles");
    let effect = effect.with_simulation_space(SimulationSpace::Local);
    let effect = effects.add(effect);

    commands.insert_resource(ProgressionBurstParticleEffectHandle(effect));
}

pub fn spawn_progression_particle_burst(
    commands: &mut Commands,
    parent: Option<Entity>,
    handle: Handle<EffectAsset>,
    transform: Transform,
    render_layers: Option<RenderLayers>,
) -> Entity {
    let mut effect_entity = commands.spawn((
        Name::new("ProgressionBurstParticles"),
        super::particle_presentation::PresentedEffect(ParticleEffect::new(handle)),
        transform,
        GlobalTransform::default(),
        Visibility::Visible,
        InheritedVisibility::VISIBLE,
        ViewVisibility::default(),
        ProgressionBurstLifetime {
            remaining_ticks: BURST_ENTITY_LIFETIME_TICKS,
        },
        StateDespawnMarker,
    ));

    if let Some(render_layers) = render_layers {
        effect_entity.insert(render_layers);
    }

    let entity = effect_entity.id();
    if let Some(parent) = parent {
        commands.entity(parent).add_child(entity);
    }
    entity
}

fn tick_progression_burst_lifetime(
    mut query: Query<(Entity, &mut ProgressionBurstLifetime)>,
    mut commands: Commands,
) {
    for (entity, mut lifetime) in query.iter_mut() {
        lifetime.remaining_ticks = lifetime.remaining_ticks.saturating_sub(1);
        if lifetime.remaining_ticks == 0 {
            commands.entity(entity).despawn();
        }
    }
}
