use crate::graphics::quad_mesh::{QuadMesh, add_mesh, publish_mesh};
use crate::graphics::pixel_view::PixelAlignedWorldMesh;
use bevy::mesh::{Mesh, Mesh2d};
use bevy::camera::visibility::{NoFrustumCulling, RenderLayers};
use bevy::sprite_render::{ColorMaterial, MeshMaterial2d};
use rand::Rng;
use theseeker_engine::animation::{AnimationSet, PlaybackState};
use theseeker_engine::gent::Gent;
use theseeker_engine::physics::Collider;
use theseeker_engine::time::GameTickUpdate;

use crate::appstate::AppState;
use crate::game::gentstate::Facing;
use crate::game::player::sensors::{GroundSensor, WallSensor};
use crate::game::player::states::{Jumping, Running};
use crate::game::player::{Player, PlayerGfx, PlayerStateSet};
use crate::graphics::pixel_perfect::{
    pixel_perfect_layers,
};
use crate::prelude::*;

const FOOTSTEP_EVENT: &str = "player_footstep";
const MAX_DUST_PARTICLES: usize = 128;
const MAX_DUST_CELLS_PER_PARTICLE: usize = 3;
const BACKGROUND_DUST_Z: f32 = 14.0 * 0.000_001;
const FOREGROUND_DUST_Z: f32 = 16.0 * 0.000_001;
const DUST_HORIZONTAL_DRAG_PER_TICK: f32 = 0.975;
const DUST_VERTICAL_DRAG_PER_TICK: f32 = 0.985;

pub struct PlayerDustPlugin;

impl Plugin for PlayerDustPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PlayerDustFx>()
            .init_resource::<PlayerDustRenderMeshes>()
            .init_resource::<PlayerDustTriggerState>()
            .add_systems(OnEnter(AppState::InGame), setup_player_dust)
            .add_systems(
                GameTickUpdate,
                (simulate_dust, emit_player_dust)
                    .chain()
                    .after(PlayerStateSet::Behavior)
                    .after(AnimationSet::Signals)
                    .run_if(in_state(AppState::InGame)),
            )
            .add_systems(
                Update,
                sync_player_dust_mesh.run_if(in_state(AppState::InGame)),
            )
;
    }
}

#[derive(Clone, Copy, Debug)]
struct DustParticle {
    position: Vec2,
    velocity: Vec2,
    age_ticks: u16,
    lifetime_ticks: u16,
    opacity: f32,
    depth: DustDepth,
    shape_seed: u8,
    buoyancy: f32,
}

impl DustParticle {
    fn alpha(self) -> f32 {
        let progress = self.age_ticks as f32 / self.lifetime_ticks as f32;
        self.opacity * (1.0 - progress).clamp(0.0, 1.0)
    }
}

#[derive(Resource, Default)]
struct PlayerDustFx {
    particles: Vec<DustParticle>,
    emitters: Vec<DustEmitter>,
    mesh_dirty: bool,
}

impl PlayerDustFx {
    fn clear(&mut self) {
        self.particles.clear();
        self.emitters.clear();
        self.mesh_dirty = true;
    }

    fn push(&mut self, particle: DustParticle) {
        if self.particles.len() == MAX_DUST_PARTICLES {
            self.particles.swap_remove(0);
        }
        self.particles.push(particle);
        self.mesh_dirty = true;
    }

    fn start_emitter(
        &mut self,
        kind: DustEmissionKind,
        horizontal_bias: f32,
    ) {
        if let Some(emitter) = self
            .emitters
            .iter_mut()
            .find(|emitter| emitter.kind == kind)
        {
            *emitter = DustEmitter::new(kind, horizontal_bias);
            return;
        }

        self.emitters.push(DustEmitter::new(kind, horizontal_bias));
    }
}

#[derive(Resource, Default)]
struct PlayerDustRenderMeshes {
    background: Option<Entity>,
    foreground: Option<Entity>,
}

#[derive(Component)]
struct ForegroundPlayerDustMesh;

#[derive(Resource, Default)]
struct PlayerDustTriggerState {
    player: Option<Entity>,
    facing: Facing,
}

impl PlayerDustTriggerState {
    fn observe_facing(
        &mut self,
        player: Entity,
        facing: Facing,
    ) -> Option<Facing> {
        if self.player != Some(player) {
            self.player = Some(player);
            self.facing = facing;
            return None;
        }

        let previous = self.facing;
        self.facing = facing;
        (previous != facing).then_some(previous)
    }

    fn reset(&mut self) {
        *self = Self::default();
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DustDepth {
    Background,
    Foreground,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DustEmissionKind {
    Footstep,
    Turn,
    Jump,
}

#[derive(Clone)]
struct DustEmissionProfile {
    duration_ticks: u16,
    particles_per_tick: f32,
    spawn_spread: f32,
    horizontal_speed: std::ops::Range<f32>,
    vertical_speed: std::ops::Range<f32>,
    buoyancy: std::ops::Range<f32>,
    lifetime_ticks: std::ops::RangeInclusive<u16>,
    opacity: std::ops::Range<f32>,
    foreground_chance: f64,
}

impl DustEmissionKind {
    fn profile(self) -> DustEmissionProfile {
        match self {
            Self::Footstep => DustEmissionProfile {
                duration_ticks: 12,
                particles_per_tick: 3.0,
                spawn_spread: 0.6,
                horizontal_speed: 0.06..0.15,
                vertical_speed: 0.035..0.1,
                buoyancy: 0.000_3..0.001,
                lifetime_ticks: 42..=60,
                opacity: 0.28..0.5,
                foreground_chance: 0.18,
            },
            Self::Turn => DustEmissionProfile {
                duration_ticks: 14,
                particles_per_tick: 4.0,
                spawn_spread: 0.8,
                horizontal_speed: 0.08..0.18,
                vertical_speed: 0.04..0.12,
                buoyancy: 0.000_4..0.001_2,
                lifetime_ticks: 44..=62,
                opacity: 0.3..0.55,
                foreground_chance: 0.22,
            },
            Self::Jump => DustEmissionProfile {
                duration_ticks: 18,
                particles_per_tick: 4.0,
                spawn_spread: 0.55,
                horizontal_speed: 0.09..0.22,
                vertical_speed: 0.015..0.06,
                buoyancy: 0.000_2..0.000_7,
                lifetime_ticks: 46..=66,
                opacity: 0.28..0.52,
                foreground_chance: 0.22,
            },
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct DustEmitter {
    kind: DustEmissionKind,
    horizontal_bias: f32,
    age_ticks: u16,
    emission_credit: f32,
}

impl DustEmitter {
    fn new(kind: DustEmissionKind, horizontal_bias: f32) -> Self {
        Self {
            kind,
            horizontal_bias,
            age_ticks: 0,
            emission_credit: 0.0,
        }
    }
}

fn setup_player_dust(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
    mut fx: ResMut<PlayerDustFx>,
    mut render_meshes: ResMut<PlayerDustRenderMeshes>,
    mut trigger_state: ResMut<PlayerDustTriggerState>,
) {
    fx.clear();
    trigger_state.reset();

    let background_mesh = add_mesh(&mut meshes, dust_mesh(&[], DustDepth::Background));
    let foreground_mesh = add_mesh(&mut meshes, dust_mesh(&[], DustDepth::Foreground));
    let material = materials.add(ColorMaterial::default());
    let background_entity = commands.spawn((
        NoFrustumCulling,
        Mesh2d(background_mesh.clone()),
        MeshMaterial2d(material.clone()),
        Transform::from_translation(Vec3::Z * BACKGROUND_DUST_Z),
        GlobalTransform::default(),
        Visibility::Visible,
        InheritedVisibility::VISIBLE,
        ViewVisibility::default(),
        pixel_perfect_layers(),
        StateDespawnMarker,
        Name::new("BackgroundPlayerDustFx"),
    )).id();
    let foreground_entity = commands.spawn((
        ForegroundPlayerDustMesh,
        PixelAlignedWorldMesh,
        NoFrustumCulling,
        Mesh2d(foreground_mesh.clone()),
        MeshMaterial2d(material),
        Transform::from_translation(Vec3::Z * FOREGROUND_DUST_Z),
        GlobalTransform::default(),
        Visibility::Visible,
        InheritedVisibility::VISIBLE,
        ViewVisibility::default(),
        RenderLayers::layer(2),
        StateDespawnMarker,
        Name::new("ForegroundPlayerDustFx"),
    )).id();
    render_meshes.background = Some(background_entity);
    render_meshes.foreground = Some(foreground_entity);
}

#[expect(
    clippy::type_complexity,
    reason = "The query reads the compact player state needed to classify three dust burst triggers."
)]
fn emit_player_dust(
    player_query: Query<(
        Entity,
        &Transform,
        &Collider,
        &Gent,
        &Facing,
        &GroundSensor,
        &WallSensor,
        Has<Running>,
        Option<Ref<Jumping>>,
    ), With<Player>>,
    gfx_query: Query<&PlaybackState, With<PlayerGfx>>,
    mut fx: ResMut<PlayerDustFx>,
    mut trigger_state: ResMut<PlayerDustTriggerState>,
) {
    let Ok((
        player,
        transform,
        collider,
        gent,
        facing,
        ground_sensor,
        wall_sensor,
        is_running,
        jumping,
    )) = player_query.single()
    else {
        trigger_state.reset();
        fx.emitters.clear();
        return;
    };

    let is_grounded = ground_sensor.is_grounded;
    let feet = player_feet_position(transform, collider);
    let footstep = is_running
        && is_grounded
        && gfx_query
            .get(gent.e_gfx)
            .is_ok_and(|signals| signals.has_event(FOOTSTEP_EVENT));
    let previous_facing = trigger_state.observe_facing(player, *facing);
    let is_touching_wall =
        wall_sensor.left_contact || wall_sensor.right_contact;
    let first_ground_jump = jumping.as_ref().is_some_and(|jumping| {
        should_emit_jump_dust(
            jumping.is_added(),
            jumping.jump_count,
            is_grounded,
            is_touching_wall,
        )
    });

    if footstep {
        fx.start_emitter(
            DustEmissionKind::Footstep,
            -facing.direction(),
        );
    }
    if is_running && is_grounded {
        if let Some(previous_facing) = previous_facing {
            fx.start_emitter(
                DustEmissionKind::Turn,
                previous_facing.direction(),
            );
        }
    }
    if first_ground_jump {
        fx.start_emitter(DustEmissionKind::Jump, 0.0);
    }

    emit_active_dust(&mut fx, feet, &mut rand::rng());
}

fn player_feet_position(transform: &Transform, collider: &Collider) -> Vec2 {
    let half_height = collider
        .cuboid_half_extents()
        .map(|half| half.y)
        .unwrap_or(5.0);
    transform.translation.xy() - Vec2::Y * half_height
}

fn should_emit_jump_dust(
    jumping_added: bool,
    jump_count: u8,
    is_grounded: bool,
    is_touching_wall: bool,
) -> bool {
    jumping_added
        && jump_count == 1
        && (is_grounded || is_touching_wall)
}

fn emit_active_dust(
    fx: &mut PlayerDustFx,
    origin: Vec2,
    rng: &mut impl Rng,
) {
    let mut emitters = std::mem::take(&mut fx.emitters);
    emitters.retain_mut(|emitter| {
        emit_from_dust_emitter(fx, emitter, origin, rng);
        emitter.age_ticks += 1;
        emitter.age_ticks < emitter.kind.profile().duration_ticks
    });
    fx.emitters = emitters;
}

fn emit_from_dust_emitter(
    fx: &mut PlayerDustFx,
    emitter: &mut DustEmitter,
    origin: Vec2,
    rng: &mut impl Rng,
) {
    let profile = emitter.kind.profile();
    emitter.emission_credit += profile.particles_per_tick;
    let particle_count = emitter.emission_credit.floor() as usize;
    emitter.emission_credit -= particle_count as f32;

    for _ in 0..particle_count {
        fx.push(dust_particle_from_emitter(
            emitter,
            &profile,
            origin,
            rng,
        ));
    }
}

fn dust_particle_from_emitter(
    emitter: &DustEmitter,
    profile: &DustEmissionProfile,
    origin: Vec2,
    rng: &mut impl Rng,
) -> DustParticle {
    let position = origin
        + Vec2::new(
            rng.random_range(-profile.spawn_spread..profile.spawn_spread),
            rng.random_range(0.0..0.5),
        );
    let velocity = Vec2::new(
        dust_horizontal_velocity(emitter.horizontal_bias, profile, rng),
        rng.random_range(profile.vertical_speed.clone()),
    );
    let depth = if rng.random_bool(profile.foreground_chance) {
        DustDepth::Foreground
    } else {
        DustDepth::Background
    };

    DustParticle {
        position,
        velocity,
        age_ticks: 0,
        lifetime_ticks: rng.random_range(profile.lifetime_ticks.clone()),
        opacity: rng.random_range(profile.opacity.clone()),
        depth,
        shape_seed: rng.random_range(0..8),
        buoyancy: rng.random_range(profile.buoyancy.clone()),
    }
}

fn dust_horizontal_velocity(
    horizontal_bias: f32,
    profile: &DustEmissionProfile,
    rng: &mut impl Rng,
) -> f32 {
    let speed = rng.random_range(profile.horizontal_speed.clone());
    let factor = if horizontal_bias == 0.0 {
        center_weighted_lateral_factor(rng.random_range(-1.0..1.0))
    } else {
        horizontal_bias.signum()
    };
    factor * speed + rng.random_range(-0.012..0.012)
}

fn center_weighted_lateral_factor(sample: f32) -> f32 {
    sample * sample.abs()
}

fn simulate_dust(mut fx: ResMut<PlayerDustFx>) {
    if fx.particles.is_empty() {
        return;
    }

    for particle in &mut fx.particles {
        particle.position += particle.velocity;
        particle.velocity.x *= DUST_HORIZONTAL_DRAG_PER_TICK;
        particle.velocity.y = particle.velocity.y
            * DUST_VERTICAL_DRAG_PER_TICK
            + particle.buoyancy;
        particle.age_ticks = particle.age_ticks.saturating_add(1);
    }
    fx.particles
        .retain(|particle| particle.age_ticks < particle.lifetime_ticks);
    fx.mesh_dirty = true;
}

fn sync_player_dust_mesh(
    mut fx: ResMut<PlayerDustFx>,
    render_meshes: Res<PlayerDustRenderMeshes>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut mesh_entities: Query<&mut Mesh2d>,
) {
    if !fx.mesh_dirty {
        return;
    }

    if let Some(mut handle) = render_meshes.background.and_then(|entity| mesh_entities.get_mut(entity).ok()) {
        publish_mesh(&mut meshes, &mut handle.0, dust_mesh(&fx.particles, DustDepth::Background));
    }
    if let Some(mut handle) = render_meshes.foreground.and_then(|entity| mesh_entities.get_mut(entity).ok()) {
        publish_mesh(&mut meshes, &mut handle.0, dust_mesh(&fx.particles, DustDepth::Foreground));
    }
    fx.mesh_dirty = false;
}


fn dust_mesh(particles: &[DustParticle], depth: DustDepth) -> Mesh {
    let particle_count = particles.iter().filter(|particle| particle.depth == depth).count();
    let mut mesh = QuadMesh::with_capacity(particle_count * MAX_DUST_CELLS_PER_PARTICLE);
    for particle in particles.iter().filter(|particle| particle.depth == depth) {
        let origin = particle.position.round().as_ivec2();
        let (offsets, cell_count) = dust_cloud_cell_offsets(particle);
        for offset in offsets.into_iter().take(cell_count) {
            mesh.push_pixel(origin + offset, [1.0, 1.0, 1.0, particle.alpha()]);
        }
    }
    mesh.finish_colored()
}

fn dust_cloud_cell_offsets(
    particle: &DustParticle,
) -> ([IVec2; MAX_DUST_CELLS_PER_PARTICLE], usize) {
    let offsets = match particle.shape_seed % 8 {
        0 => [IVec2::ZERO, IVec2::X, IVec2::ONE],
        1 => [IVec2::ZERO, IVec2::ONE, IVec2::Y],
        2 => [IVec2::ZERO, IVec2::Y, IVec2::new(-1, 1)],
        3 => [IVec2::ZERO, IVec2::new(-1, 1), IVec2::NEG_X],
        4 => [IVec2::ZERO, IVec2::NEG_X, IVec2::NEG_ONE],
        5 => [IVec2::ZERO, IVec2::NEG_ONE, IVec2::NEG_Y],
        6 => [IVec2::ZERO, IVec2::NEG_Y, IVec2::new(1, -1)],
        _ => [IVec2::ZERO, IVec2::new(1, -1), IVec2::X],
    };
    let cell_count = if particle.age_ticks < 6 {
        1
    } else if particle.age_ticks < 16 {
        2
    } else {
        3
    };
    (offsets, cell_count)
}


#[cfg(test)]
mod tests {
    use bevy::mesh::VertexAttributeValues;
    use rand::{rngs::StdRng, SeedableRng};

    use super::*;

    fn particle(age_ticks: u16, lifetime_ticks: u16) -> DustParticle {
        DustParticle {
            position: Vec2::new(1.4, -2.6),
            velocity: Vec2::ZERO,
            age_ticks,
            lifetime_ticks,
            opacity: 1.0,
            depth: DustDepth::Background,
            shape_seed: 0,
            buoyancy: 0.0,
        }
    }

    #[test]
    fn jump_dust_requires_a_new_first_jump_from_a_surface() {
        assert!(should_emit_jump_dust(true, 1, true, false));
        assert!(should_emit_jump_dust(true, 1, false, true));
        assert!(!should_emit_jump_dust(true, 2, true, false));
        assert!(!should_emit_jump_dust(true, 1, false, false));
        assert!(!should_emit_jump_dust(false, 1, true, false));
    }

    #[test]
    fn facing_tracker_ignores_initial_observation() {
        let player = Entity::from_raw_u32(7).unwrap();
        let mut state = PlayerDustTriggerState::default();

        assert_eq!(state.observe_facing(player, Facing::Right), None);
        assert_eq!(state.observe_facing(player, Facing::Left), Some(Facing::Right));
        assert_eq!(state.observe_facing(player, Facing::Left), None);
    }

    #[test]
    fn particle_pool_never_exceeds_its_fixed_capacity() {
        let mut fx = PlayerDustFx::default();
        for index in 0..MAX_DUST_PARTICLES + 20 {
            let mut particle = particle(0, 10);
            particle.position.x = index as f32;
            fx.push(particle);
        }

        assert_eq!(fx.particles.len(), MAX_DUST_PARTICLES);
    }

    #[test]
    fn dust_fades_linearly_over_its_lifetime() {
        assert_eq!(particle(0, 40).alpha(), 1.0);
        assert_eq!(particle(20, 40).alpha(), 0.5);
        assert_eq!(particle(40, 40).alpha(), 0.0);
    }

    #[test]
    fn dust_mesh_uses_integer_aligned_single_pixel_quads() {
        let mesh = dust_mesh(&[particle(0, 40)], DustDepth::Background);
        let Some(VertexAttributeValues::Float32x3(positions)) =
            mesh.attribute(Mesh::ATTRIBUTE_POSITION)
        else {
            panic!("dust mesh requires Float32x3 positions");
        };

        assert_eq!(positions[0], [1.0, -3.0, 0.0]);
        assert_eq!(positions[2], [2.0, -2.0, 0.0]);
    }

    #[test]
    fn dust_clouds_expand_in_discrete_pixel_cells_as_they_age() {
        let young = dust_mesh(&[particle(0, 40)], DustDepth::Background);
        let middle = dust_mesh(&[particle(8, 40)], DustDepth::Background);
        let old = dust_mesh(&[particle(20, 40)], DustDepth::Background);

        assert_eq!(mesh_position_count(&young), 4);
        assert_eq!(mesh_position_count(&middle), 8);
        assert_eq!(mesh_position_count(&old), 12);
    }

    #[test]
    fn jump_lateral_distribution_keeps_particles_in_the_center() {
        assert_eq!(center_weighted_lateral_factor(0.0), 0.0);
        assert_eq!(center_weighted_lateral_factor(0.5), 0.25);
        assert_eq!(center_weighted_lateral_factor(-0.5), -0.25);
        assert_eq!(center_weighted_lateral_factor(1.0), 1.0);
        assert_eq!(center_weighted_lateral_factor(-1.0), -1.0);
    }

    #[test]
    fn jump_emitter_releases_over_time_from_the_current_player_position() {
        let mut fx = PlayerDustFx::default();
        let mut rng = StdRng::seed_from_u64(21);
        fx.start_emitter(DustEmissionKind::Jump, 0.0);

        emit_active_dust(&mut fx, Vec2::ZERO, &mut rng);
        let first_tick_count = fx.particles.len();
        emit_active_dust(&mut fx, Vec2::Y * 8.0, &mut rng);

        assert!(first_tick_count > 0);
        assert!(fx.particles.len() > first_tick_count);
        assert!(fx.particles[first_tick_count..]
            .iter()
            .all(|particle| particle.position.y >= 8.0));
        assert!(!fx.emitters.is_empty());
    }

    #[test]
    fn emitter_expires_after_its_profile_duration() {
        let mut fx = PlayerDustFx::default();
        let mut rng = StdRng::seed_from_u64(34);
        let duration = DustEmissionKind::Footstep.profile().duration_ticks;
        fx.start_emitter(DustEmissionKind::Footstep, -1.0);

        for _ in 0..duration {
            emit_active_dust(&mut fx, Vec2::ZERO, &mut rng);
        }

        assert!(fx.emitters.is_empty());
        assert!(fx.particles.len() > 1);
    }

    #[test]
    fn depth_batches_include_only_their_assigned_particles() {
        let background = particle(0, 40);
        let mut foreground = particle(0, 40);
        foreground.depth = DustDepth::Foreground;
        foreground.position = Vec2::new(8.0, 9.0);
        let particles = [background, foreground];

        let background_mesh = dust_mesh(&particles, DustDepth::Background);
        let foreground_mesh = dust_mesh(&particles, DustDepth::Foreground);
        let Some(VertexAttributeValues::Float32x3(background_positions)) =
            background_mesh.attribute(Mesh::ATTRIBUTE_POSITION)
        else {
            panic!("background dust positions");
        };
        let Some(VertexAttributeValues::Float32x3(foreground_positions)) =
            foreground_mesh.attribute(Mesh::ATTRIBUTE_POSITION)
        else {
            panic!("foreground dust positions");
        };

        assert_eq!(background_positions.len(), 4);
        assert_eq!(foreground_positions.len(), 4);
        assert_eq!(foreground_positions[0], [8.0, 9.0, 0.0]);
    }

    fn mesh_position_count(mesh: &Mesh) -> usize {
        let Some(VertexAttributeValues::Float32x3(positions)) =
            mesh.attribute(Mesh::ATTRIBUTE_POSITION)
        else {
            panic!("dust mesh requires Float32x3 positions");
        };
        positions.len()
    }
}
