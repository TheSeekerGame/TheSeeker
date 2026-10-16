use bevy::shader::ShaderRef;
use std::collections::{HashMap, HashSet};
use std::f32::consts::TAU;

use bevy::asset::RenderAssetUsages;
use bevy::ecs::system::SystemParam;
use bevy::image::{ImageSampler, TextureAtlasBuilder, TextureAtlasSources};
use bevy::mesh::{
    Indices, Mesh, Mesh2d, MeshVertexAttribute, MeshVertexBufferLayoutRef,
};
use bevy::camera::primitives::Aabb;
use bevy::render::render_resource::{
    AsBindGroup, Extent3d, PrimitiveTopology, RenderPipelineDescriptor,
    ShaderType, SpecializedMeshPipelineError, TextureDimension,
    TextureFormat, TextureUsages, VertexFormat,
};
use bevy::camera::visibility::RenderLayers;
use bevy::sprite_render::{
    AlphaMode2d, Material2d, Material2dKey, Material2dPlugin,
    MeshMaterial2d,
};
use theseeker_engine::physics::{Collider, LinearVelocity};
use theseeker_engine::time::{
    GameTickPost, GameTickSet, GameTime,
};

use crate::camera::MainCamera;
use crate::graphics::pixel_view::PixelView;
#[cfg(test)]
use crate::graphics::pixel_view::PixelViewMapping;
use crate::game::combat::{effects::DamageFlash, DamageSource};
use crate::game::physics::projectile::Projectile;
#[cfg(test)]
use crate::graphics::pixel_view::snap_world_position_to_physical_pixels;
use crate::prelude::*;

use super::{
    harvestable_materials, CompiledLevel,
    EntityPlacement, ENTITY_Z,
};

const FOLIAGE_SHADER_PATH: &str = "shaders/foliage.wgsl";
const MAX_IMPULSE_INFLUENCES: usize = 8;
const FOLIAGE_CULL_BEND_MARGIN: f32 = 6.0;
const PROJECTILE_INFLUENCE_RADIUS: f32 = 8.0;
const FOLIAGE_FIELD_SIZE: UVec2 = UVec2::new(320, 192);
const FOLIAGE_FIELD_CELL_SIZE: f32 = 2.0;
const FOLIAGE_SPRING_STIFFNESS: f32 = 24.0;
const FOLIAGE_SPRING_DAMPING: f32 = 1.4;

const ATTRIBUTE_FOLIAGE_DATA: MeshVertexAttribute = MeshVertexAttribute::new(
    "FoliageData",
    1_731_449_921,
    VertexFormat::Float32x4,
);
const ATTRIBUTE_FOLIAGE_BEND: MeshVertexAttribute = MeshVertexAttribute::new(
    "FoliageBend",
    1_731_449_922,
    VertexFormat::Float32,
);
const ATTRIBUTE_FOLIAGE_FLASH: MeshVertexAttribute = MeshVertexAttribute::new(
    "FoliageFlash",
    1_731_449_923,
    VertexFormat::Float32,
);

pub(super) struct FoliagePlugin;

impl Plugin for FoliagePlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(Material2dPlugin::<FoliageMaterial>::default());
        app.init_resource::<FoliageSettings>();
        app.init_resource::<FoliageRuntime>();
        app.add_message::<FoliageImpulseEvent>();
        app.add_systems(
            GameTickPost,
            (
                attach_moving_damage_source_interactors,
                attach_player_attack_interactors,
                track_foliage_interactors,
                update_foliage_interaction_field,
                animate_foliage_damage_flash,
            )
                .chain()
                .run_if(in_state(AppState::InGame))
                .run_if(in_state(GameState::Playing)),
        );
        app.add_systems(
            Update,
            upload_foliage_render_state
                .after(crate::graphics::pixel_view::update_pixel_view)
                .in_set(GameTickSet::Post)
                .after(crate::camera::update_camera)
                .run_if(in_state(AppState::InGame)),
        );
        app.add_systems(OnExit(AppState::InGame), clear_foliage_runtime);
    }
}

#[derive(Resource, Clone, Copy, Debug)]
pub(super) struct FoliageSettings {
    grass_wind_amplitude: f32,
    plant_wind_amplitude: f32,
    primary_wind_angular_speed: f32,
    secondary_wind_angular_speed: f32,
    max_interaction_bend: f32,
    full_speed_interaction_bend: f32,
    full_bend_speed: f32,
    attack_excitation_speed: f32,
    interaction_min_speed: f32,
    teleport_distance_per_tick: f32,
}

impl Default for FoliageSettings {
    fn default() -> Self {
        Self {
            grass_wind_amplitude: 0.75,
            plant_wind_amplitude: 1.0,
            primary_wind_angular_speed: TAU * 0.22,
            secondary_wind_angular_speed: TAU * 0.47,
            max_interaction_bend: 6.0,
            full_speed_interaction_bend: 4.0,
            full_bend_speed: 400.0,
            attack_excitation_speed: 440.0,
            interaction_min_speed: 24.0,
            teleport_distance_per_tick: 48.0,
        }
    }
}

#[derive(Clone, Copy, ShaderType)]
struct FoliageMaterialParams {
    time_seconds: f32,
    max_interaction_bend: f32,
    primary_wind_angular_speed: f32,
    secondary_wind_angular_speed: f32,
    interaction_field_params: Vec4,
    camera_alignment_offset: Vec2,
    _padding: Vec2,
}

#[derive(Asset, TypePath, AsBindGroup, Clone)]
pub(super) struct FoliageMaterial {
    #[texture(0)]
    #[sampler(1)]
    atlas: Handle<Image>,
    #[texture(2)]
    #[sampler(3)]
    interaction_field: Handle<Image>,
    #[uniform(4)]
    params: FoliageMaterialParams,
}

impl Material2d for FoliageMaterial {
    fn vertex_shader() -> ShaderRef {
        FOLIAGE_SHADER_PATH.into()
    }

    fn fragment_shader() -> ShaderRef {
        FOLIAGE_SHADER_PATH.into()
    }

    fn alpha_mode(&self) -> AlphaMode2d {
        AlphaMode2d::Mask(0.5)
    }

    fn specialize(
        descriptor: &mut RenderPipelineDescriptor,
        layout: &MeshVertexBufferLayoutRef,
        _key: Material2dKey<Self>,
    ) -> Result<(), SpecializedMeshPipelineError> {
        let vertex_layout = layout.0.get_layout(&[
            Mesh::ATTRIBUTE_POSITION.at_shader_location(0),
            Mesh::ATTRIBUTE_UV_0.at_shader_location(1),
            ATTRIBUTE_FOLIAGE_DATA.at_shader_location(2),
            ATTRIBUTE_FOLIAGE_BEND.at_shader_location(3),
            ATTRIBUTE_FOLIAGE_FLASH.at_shader_location(4),
        ])?;
        descriptor.vertex.buffers = vec![vertex_layout];
        Ok(())
    }
}

#[derive(Component, Debug, Clone, Copy)]
pub(crate) struct FoliageInteractor {
    mode: FoliageInteractorMode,
    radius: f32,
    strength: f32,
    previous_position: Option<Vec2>,
    influence_position: Vec2,
    velocity_per_second: Vec2,
}

#[derive(Debug, Clone, Copy)]
enum FoliageInteractorMode {
    Movement,
    PlayerAttack { owner: Entity },
}

impl FoliageInteractor {
    pub(crate) fn body(collider: &Collider) -> Self {
        Self::body_with_strength(collider, 1.0)
    }

    pub(crate) fn player_body(collider: &Collider) -> Self {
        Self::body_with_strength(collider, 1.5)
    }

    fn body_with_strength(collider: &Collider, strength: f32) -> Self {
        let half_width = collider.half_extents().x;
        Self::new(
            FoliageInteractorMode::Movement,
            (half_width + 6.0).clamp(8.0, 32.0),
            strength,
        )
    }

    pub(crate) const fn projectile() -> Self {
        Self::new(
            FoliageInteractorMode::Movement,
            PROJECTILE_INFLUENCE_RADIUS,
            0.7,
        )
    }

    const fn player_attack(owner: Entity, radius: f32) -> Self {
        Self::new(
            FoliageInteractorMode::PlayerAttack { owner },
            radius,
            1.0,
        )
    }

    const fn new(
        mode: FoliageInteractorMode,
        radius: f32,
        strength: f32,
    ) -> Self {
        Self {
            mode,
            radius,
            strength,
            previous_position: None,
            influence_position: Vec2::ZERO,
            velocity_per_second: Vec2::ZERO,
        }
    }
}

#[derive(Message, Clone, Copy, Debug)]
pub(crate) struct FoliageImpulseEvent {
    pub origin: Vec2,
    pub propagation: FoliageImpulsePropagation,
    pub strength: f32,
}

impl FoliageImpulseEvent {
    pub(crate) const fn instant(
        origin: Vec2,
        radius: f32,
        strength: f32,
    ) -> Self {
        Self {
            origin,
            propagation: FoliageImpulsePropagation::Instant {
                radius,
                duration_seconds: 0.35,
            },
            strength,
        }
    }

    pub(crate) const fn ring(
        origin: Vec2,
        start_radius: f32,
        max_radius: f32,
        speed_per_tick: f32,
        width: f32,
        strength: f32,
        fade_power: f32,
    ) -> Self {
        Self {
            origin,
            propagation: FoliageImpulsePropagation::Ring {
                start_radius,
                max_radius,
                speed_per_tick,
                width,
                fade_power,
            },
            strength,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum FoliageImpulsePropagation {
    Instant {
        radius: f32,
        duration_seconds: f32,
    },
    Ring {
        start_radius: f32,
        max_radius: f32,
        speed_per_tick: f32,
        width: f32,
        fade_power: f32,
    },
}

#[derive(Clone, Copy, Debug)]
struct ActiveFoliageImpulse {
    origin: Vec2,
    propagation: FoliageImpulsePropagation,
    strength: f32,
    age_seconds: f32,
    ring_radius: f32,
}

impl From<FoliageImpulseEvent> for ActiveFoliageImpulse {
    fn from(event: FoliageImpulseEvent) -> Self {
        let ring_radius = match event.propagation {
            FoliageImpulsePropagation::Instant { .. } => 0.0,
            FoliageImpulsePropagation::Ring { start_radius, .. } => {
                start_radius
            },
        };
        Self {
            origin: event.origin,
            propagation: event.propagation,
            strength: event.strength,
            age_seconds: 0.0,
            ring_radius,
        }
    }
}

impl ActiveFoliageImpulse {
    fn advance(&mut self, seconds_per_tick: f32) -> bool {
        self.age_seconds += seconds_per_tick;
        match self.propagation {
            FoliageImpulsePropagation::Instant {
                duration_seconds, ..
            } => self.age_seconds < duration_seconds,
            FoliageImpulsePropagation::Ring {
                max_radius,
                speed_per_tick,
                width,
                ..
            } => {
                self.ring_radius += speed_per_tick;
                self.ring_radius <= max_radius + width.max(8.0)
            },
        }
    }

    fn current_strength(self) -> f32 {
        match self.propagation {
            FoliageImpulsePropagation::Instant {
                duration_seconds,
                ..
            } => {
                let progress =
                    (self.age_seconds / duration_seconds).clamp(0.0, 1.0);
                self.strength * (1.0 - progress).powi(2)
            },
            FoliageImpulsePropagation::Ring {
                start_radius,
                max_radius,
                fade_power,
                ..
            } => {
                let distance = (max_radius - start_radius).max(f32::EPSILON);
                let progress =
                    ((self.ring_radius - start_radius) / distance)
                        .clamp(0.0, 1.0);
                self.strength
                    * (1.0 - progress).powf(fade_power.max(0.0))
            },
        }
    }
}

#[derive(Resource, Default)]
struct FoliageRuntime {
    time_seconds: f32,
    impulses: Vec<ActiveFoliageImpulse>,
}

#[derive(Clone, Copy, Debug, Default)]
struct FoliageFieldCell {
    previous_displacement: f32,
    displacement: f32,
    velocity: f32,
}

#[derive(Resource)]
struct FoliageInteractionField {
    image: Handle<Image>,
    origin_cell: IVec2,
    initialized: bool,
    cells: Vec<FoliageFieldCell>,
    scratch: Vec<FoliageFieldCell>,
    revision: u64,
    uploaded_revision: u64,
}

impl FoliageInteractionField {
    fn new(image: Handle<Image>) -> Self {
        let cell_count = (FOLIAGE_FIELD_SIZE.x * FOLIAGE_FIELD_SIZE.y) as usize;
        Self {
            image,
            origin_cell: IVec2::ZERO,
            initialized: false,
            cells: vec![FoliageFieldCell::default(); cell_count],
            scratch: vec![FoliageFieldCell::default(); cell_count],
            revision: 1,
            uploaded_revision: 0,
        }
    }

    #[expect(
        clippy::excessive_nesting,
        reason = "The bounded two-dimensional field remap mirrors grid coordinates directly."
    )]
    fn recenter(&mut self, camera_center: Vec2) {
        let center_cell = (camera_center / FOLIAGE_FIELD_CELL_SIZE)
            .floor()
            .as_ivec2();
        let desired_origin = center_cell
            - IVec2::new(
                FOLIAGE_FIELD_SIZE.x as i32 / 2,
                FOLIAGE_FIELD_SIZE.y as i32 / 2,
            );
        if !self.initialized {
            self.origin_cell = desired_origin;
            self.initialized = true;
            return;
        }
        if desired_origin == self.origin_cell {
            return;
        }

        self.scratch.fill(FoliageFieldCell::default());
        for new_y in 0..FOLIAGE_FIELD_SIZE.y as i32 {
            for new_x in 0..FOLIAGE_FIELD_SIZE.x as i32 {
                let world_cell = desired_origin + IVec2::new(new_x, new_y);
                let old = world_cell - self.origin_cell;
                if old.x < 0
                    || old.y < 0
                    || old.x >= FOLIAGE_FIELD_SIZE.x as i32
                    || old.y >= FOLIAGE_FIELD_SIZE.y as i32
                {
                    continue;
                }
                let new_index = field_index(new_x as u32, new_y as u32);
                let old_index = field_index(old.x as u32, old.y as u32);
                self.scratch[new_index] = self.cells[old_index];
            }
        }
        std::mem::swap(&mut self.cells, &mut self.scratch);
        self.origin_cell = desired_origin;
    }

    #[expect(
        clippy::excessive_nesting,
        reason = "Per-cell spring settling is intentionally local to the bounded update loop."
    )]
    fn advance_spring(&mut self, delta_seconds: f32, max_displacement: f32) {
        for cell in &mut self.cells {
            cell.previous_displacement = cell.displacement;
            let acceleration = -cell.displacement * FOLIAGE_SPRING_STIFFNESS
                - cell.velocity * FOLIAGE_SPRING_DAMPING;
            cell.velocity += acceleration * delta_seconds;
            cell.displacement = (cell.displacement
                + cell.velocity * delta_seconds)
                .clamp(-max_displacement, max_displacement);
            if cell.displacement.abs() < 0.000_1
                && cell.velocity.abs() < 0.000_1
            {
                *cell = FoliageFieldCell::default();
            }
        }
        self.revision = self.revision.wrapping_add(1);
    }

    fn inject_mover(
        &mut self,
        center: Vec2,
        radius: f32,
        direction: Vec2,
        excitation_velocity: f32,
    ) {
        self.inject_region(center, radius, |delta, falloff, cell| {
            let vertical_push = delta.x / (delta.x.abs() + 2.0)
                * direction.y.abs()
                * 0.35;
            let target_velocity = (direction.x + vertical_push)
                * excitation_velocity;
            cell.velocity = cell.velocity.lerp(target_velocity, falloff);
        });
    }

    fn inject_radial(
        &mut self,
        center: Vec2,
        radius: f32,
        excitation_velocity: f32,
    ) {
        self.inject_region(center, radius, |delta, falloff, cell| {
            let outward = delta.x / (delta.x.abs() + 2.0);
            cell.velocity = cell
                .velocity
                .lerp(outward * excitation_velocity, falloff);
        });
    }

    fn inject_ring(
        &mut self,
        center: Vec2,
        radius: f32,
        width: f32,
        excitation_velocity: f32,
    ) {
        let extent = radius + width;
        self.inject_region(center, extent, |delta, _falloff, cell| {
            let ring_distance = (delta.length() - radius).abs();
            let ring_falloff = 1.0
                - smoothstep(width * 0.35, width, ring_distance);
            let outward = delta.x / (delta.x.abs() + 2.0);
            cell.velocity = cell
                .velocity
                .lerp(outward * excitation_velocity, ring_falloff);
        });
    }

    #[expect(
        clippy::excessive_nesting,
        reason = "Emitter injection iterates only the clipped rectangular subset of the field."
    )]
    fn inject_region(
        &mut self,
        center: Vec2,
        radius: f32,
        mut inject: impl FnMut(Vec2, f32, &mut FoliageFieldCell),
    ) {
        let field_min = self.origin_cell.as_vec2() * FOLIAGE_FIELD_CELL_SIZE;
        let min = (((center - Vec2::splat(radius)) - field_min)
            / FOLIAGE_FIELD_CELL_SIZE)
            .floor()
            .as_ivec2()
            .max(IVec2::ZERO);
        let max = (((center + Vec2::splat(radius)) - field_min)
            / FOLIAGE_FIELD_CELL_SIZE)
            .ceil()
            .as_ivec2()
            .min(FOLIAGE_FIELD_SIZE.as_ivec2() - IVec2::ONE);
        if min.x > max.x || min.y > max.y {
            return;
        }

        for y in min.y..=max.y {
            for x in min.x..=max.x {
                let cell_center = field_min
                    + (Vec2::new(x as f32, y as f32) + Vec2::splat(0.5))
                        * FOLIAGE_FIELD_CELL_SIZE;
                let delta = cell_center - center;
                let distance = delta.length();
                let falloff = 1.0
                    - smoothstep(radius * 0.2, radius, distance);
                if falloff <= 0.0 {
                    continue;
                }
                let index = field_index(x as u32, y as u32);
                inject(delta, falloff, &mut self.cells[index]);
            }
        }
    }

    fn origin_world(&self) -> Vec2 {
        self.origin_cell.as_vec2() * FOLIAGE_FIELD_CELL_SIZE
    }

    fn write_image(&mut self, image: &mut Image, max_displacement: f32) {
        let Some(data) = image.data.as_mut() else { return };
        for (bytes, cell) in data.as_chunks_mut::<4>().0.iter_mut().zip(&self.cells) {
            let previous = encode_field_displacement(
                cell.previous_displacement,
                max_displacement,
            )
            .to_ne_bytes();
            let current = encode_field_displacement(
                cell.displacement,
                max_displacement,
            )
            .to_ne_bytes();
            bytes[..2].copy_from_slice(&previous);
            bytes[2..].copy_from_slice(&current);
        }
        self.uploaded_revision = self.revision;
    }
}

#[derive(Resource)]
struct FoliageRenderAssets {
    material: Handle<FoliageMaterial>,
}

fn field_index(x: u32, y: u32) -> usize {
    (y * FOLIAGE_FIELD_SIZE.x + x) as usize
}

fn smoothstep(edge_0: f32, edge_1: f32, value: f32) -> f32 {
    let progress = ((value - edge_0) / (edge_1 - edge_0).max(f32::EPSILON))
        .clamp(0.0, 1.0);
    progress * progress * (3.0 - 2.0 * progress)
}

fn encode_field_displacement(displacement: f32, max_displacement: f32) -> u16 {
    let normalized = (displacement / max_displacement * 0.5 + 0.5)
        .clamp(0.0, 1.0);
    (normalized * u16::MAX as f32).round() as u16
}

#[derive(Component)]
struct HarvestableFoliageVisual;

#[derive(Default)]
struct FoliageMeshData {
    positions: Vec<[f32; 3]>,
    uvs: Vec<[f32; 2]>,
    foliage_data: Vec<[f32; 4]>,
    bend_weights: Vec<f32>,
    flash: Vec<f32>,
    indices: Vec<u32>,
    min: Vec2,
    max: Vec2,
    has_bounds: bool,
}

#[derive(Clone, Copy)]
struct FoliageSpriteSpec {
    min: Vec2,
    max: Vec2,
    deformation_root: Vec2,
    uv_rect: Rect,
    flip_x: bool,
    flip_y: bool,
    phase: f32,
    wind_amplitude: f32,
}

#[derive(Clone, Copy)]
struct FoliageSourceGeometry {
    image_size: UVec2,
    opaque_rect: URect,
    opaque_uv_rect: Rect,
}

impl FoliageMeshData {
    fn append(&mut self, spec: FoliageSpriteSpec) {
        let FoliageSpriteSpec {
            min,
            max,
            deformation_root,
            uv_rect,
            flip_x,
            flip_y,
            phase,
            wind_amplitude,
        } = spec;
        let left = min.x;
        let right = max.x;
        let row_count = (max.y - min.y).round().max(1.0) as usize;

        for row in 0..row_count {
            let base_index = self.positions.len() as u32;
            let lower_fraction = row as f32 / row_count as f32;
            let upper_fraction = (row + 1) as f32 / row_count as f32;
            let center_fraction = (lower_fraction + upper_fraction) * 0.5;
            let lower_y = min.y + (max.y - min.y) * lower_fraction;
            let upper_y = min.y + (max.y - min.y) * upper_fraction;
            let bend = foliage_bend_weight(center_fraction);
            let lower_v_fraction =
                foliage_v_fraction(lower_fraction, flip_y);
            let upper_v_fraction =
                foliage_v_fraction(upper_fraction, flip_y);
            let lower_v =
                uv_rect.min.y + uv_rect.height() * lower_v_fraction;
            let upper_v =
                uv_rect.min.y + uv_rect.height() * upper_v_fraction;
            let (left_u, right_u) = foliage_horizontal_uvs(uv_rect, flip_x);

            self.push_vertex(
                Vec3::new(left, lower_y, 0.0),
                Vec2::new(left_u, lower_v),
                deformation_root,
                phase,
                wind_amplitude,
                bend,
            );
            self.push_vertex(
                Vec3::new(right, lower_y, 0.0),
                Vec2::new(right_u, lower_v),
                deformation_root,
                phase,
                wind_amplitude,
                bend,
            );
            self.push_vertex(
                Vec3::new(right, upper_y, 0.0),
                Vec2::new(right_u, upper_v),
                deformation_root,
                phase,
                wind_amplitude,
                bend,
            );
            self.push_vertex(
                Vec3::new(left, upper_y, 0.0),
                Vec2::new(left_u, upper_v),
                deformation_root,
                phase,
                wind_amplitude,
                bend,
            );
            self.indices.extend_from_slice(&[
                base_index,
                base_index + 1,
                base_index + 2,
                base_index,
                base_index + 2,
                base_index + 3,
            ]);
        }
    }

    fn push_vertex(
        &mut self,
        position: Vec3,
        uv: Vec2,
        root: Vec2,
        phase: f32,
        wind_amplitude: f32,
        bend: f32,
    ) {
        self.positions.push(position.to_array());
        self.uvs.push(uv.to_array());
        self.foliage_data
            .push([root.x, root.y, phase, wind_amplitude]);
        self.bend_weights.push(bend);
        self.flash.push(1.0);
        if !self.has_bounds {
            self.min = position.xy();
            self.max = position.xy();
            self.has_bounds = true;
            return;
        }
        self.min = self.min.min(position.xy());
        self.max = self.max.max(position.xy());
    }

    fn into_mesh_and_bounds(self) -> (Mesh, Aabb) {
        let min = Vec3::new(
            self.min.x - FOLIAGE_CULL_BEND_MARGIN,
            self.min.y,
            -0.001,
        );
        let max = Vec3::new(
            self.max.x + FOLIAGE_CULL_BEND_MARGIN,
            self.max.y,
            0.001,
        );
        let mut mesh = Mesh::new(
            PrimitiveTopology::TriangleList,
            RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
        );
        mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, self.positions);
        mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, self.uvs);
        mesh.insert_attribute(ATTRIBUTE_FOLIAGE_DATA, self.foliage_data);
        mesh.insert_attribute(ATTRIBUTE_FOLIAGE_BEND, self.bend_weights);
        mesh.insert_attribute(ATTRIBUTE_FOLIAGE_FLASH, self.flash);
        mesh.insert_indices(Indices::U32(self.indices));
        (mesh, Aabb::from_min_max(min, max))
    }
}

fn foliage_bend_weight(height_fraction: f32) -> f32 {
    height_fraction.clamp(0.0, 1.0).powf(1.35)
}

fn foliage_v_fraction(height_fraction: f32, flip_y: bool) -> f32 {
    if flip_y {
        height_fraction
    } else {
        1.0 - height_fraction
    }
}

fn foliage_horizontal_uvs(uv_rect: Rect, flip_x: bool) -> (f32, f32) {
    if flip_x {
        (uv_rect.max.x, uv_rect.min.x)
    } else {
        (uv_rect.min.x, uv_rect.max.x)
    }
}

pub(super) fn is_foliage_kind(kind: &str) -> bool {
    matches!(kind, "plants" | "grass")
}

#[expect(
    clippy::too_many_arguments,
    reason = "Foliage assembly consumes the level placement and render asset stores in one load-time operation."
)]
pub(super) fn spawn_foliage(
    commands: &mut Commands,
    parent: Entity,
    compiled: &CompiledLevel,
    placements: &[&EntityPlacement],
    images: &mut Assets<Image>,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<FoliageMaterial>,
    settings: FoliageSettings,
) -> Result<usize, String> {
    if placements.is_empty() {
        return Ok(0);
    }

    let (layout, sources, mut atlas_image) =
        build_foliage_atlas(placements, images)?;
    let source_geometries = build_foliage_source_geometries(
        placements,
        images,
        &layout,
        &sources,
    )?;
    atlas_image.sampler = ImageSampler::nearest();
    let atlas = images.add(atlas_image);
    let interaction_field = images.add(create_foliage_field_image());
    let material = materials.add(FoliageMaterial {
        atlas,
        interaction_field: interaction_field.clone(),
        params: FoliageMaterialParams {
            time_seconds: 0.0,
            max_interaction_bend: settings.max_interaction_bend,
            primary_wind_angular_speed: settings.primary_wind_angular_speed,
            secondary_wind_angular_speed: settings.secondary_wind_angular_speed,
            interaction_field_params: Vec4::ZERO,
            camera_alignment_offset: Vec2::ZERO,
            _padding: Vec2::ZERO,
        },
    });
    commands.insert_resource(FoliageRenderAssets {
        material: material.clone(),
    });
    commands.insert_resource(FoliageInteractionField::new(interaction_field));
    commands.insert_resource(FoliageRuntime::default());

    let tile_size = compiled.tile_size();
    let mut chunk_placements: HashMap<IVec2, Vec<&EntityPlacement>> =
        HashMap::default();
    let mut spawned = 0usize;

    for &placement in placements {
        if placement.harvest.is_some() {
            spawn_harvestable_foliage(
                commands,
                parent,
                compiled,
                tile_size,
                placement,
                &source_geometries,
                meshes,
                &material,
                settings,
            )?;
            spawned += 1;
        } else {
            chunk_placements
                .entry(placement.chunk)
                .or_default()
                .push(placement);
        }
    }

    let mut chunks = chunk_placements.into_iter().collect::<Vec<_>>();
    chunks.sort_unstable_by_key(|(chunk, _)| (chunk.y, chunk.x));
    for (chunk, chunk_foliage) in chunks {
        spawn_foliage_chunk(
            commands,
            parent,
            compiled,
            tile_size,
            chunk,
            &chunk_foliage,
            &source_geometries,
            meshes,
            &material,
            settings,
        )?;
        spawned += chunk_foliage.len();
    }

    Ok(spawned)
}

fn create_foliage_field_image() -> Image {
    let mut image = Image::new_fill(
        Extent3d {
            width: FOLIAGE_FIELD_SIZE.x,
            height: FOLIAGE_FIELD_SIZE.y,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        &[0, 128, 0, 128],
        TextureFormat::Rg16Unorm,
        RenderAssetUsages::default(),
    );
    image.texture_descriptor.usage =
        TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST;
    image.sampler = ImageSampler::linear();
    image
}

fn build_foliage_atlas(
    placements: &[&EntityPlacement],
    images: &Assets<Image>,
) -> Result<(TextureAtlasLayout, TextureAtlasSources, Image), String> {
    let mut builder = TextureAtlasBuilder::default();
    builder.padding(UVec2::splat(2));
    let mut included: HashSet<bevy::asset::AssetId<Image>> = HashSet::new();

    for placement in placements {
        let image_id = placement.handle.id();
        if !included.insert(image_id) {
            continue;
        }
        let image = images.get(&placement.handle).ok_or_else(|| {
            format!("foliage image {:?} is not loaded", placement.handle.path())
        })?;
        builder.add_texture(Some(image_id), image);
    }

    builder
        .build()
        .map_err(|error| format!("failed to build foliage atlas: {error}"))
}

fn build_foliage_source_geometries(
    placements: &[&EntityPlacement],
    images: &Assets<Image>,
    layout: &TextureAtlasLayout,
    sources: &TextureAtlasSources,
) -> Result<HashMap<bevy::asset::AssetId<Image>, FoliageSourceGeometry>, String>
{
    let mut geometries = HashMap::default();
    for placement in placements {
        let image_id = placement.handle.id();
        if geometries.contains_key(&image_id) {
            continue;
        }
        let image = images.get(&placement.handle).ok_or_else(|| {
            format!("foliage image {:?} is not loaded", placement.handle.path())
        })?;
        let full_uv_rect = sources
            .uv_rect(layout, image_id)
            .ok_or_else(|| "foliage atlas is missing a source image".to_string())?;
        let image_size = image.size();
        let opaque_rect = opaque_alpha_rect(image)
            .unwrap_or(URect::from_corners(UVec2::ZERO, image_size));
        let uv_scale = full_uv_rect.size() / image_size.as_vec2();
        let opaque_uv_rect = Rect::from_corners(
            full_uv_rect.min + opaque_rect.min.as_vec2() * uv_scale,
            full_uv_rect.min + opaque_rect.max.as_vec2() * uv_scale,
        );
        geometries.insert(
            image_id,
            FoliageSourceGeometry {
                image_size,
                opaque_rect,
                opaque_uv_rect,
            },
        );
    }
    Ok(geometries)
}

fn opaque_alpha_rect(image: &Image) -> Option<URect> {
    if !matches!(
        image.texture_descriptor.format,
        TextureFormat::Rgba8Unorm
            | TextureFormat::Rgba8UnormSrgb
            | TextureFormat::Bgra8Unorm
            | TextureFormat::Bgra8UnormSrgb
    ) {
        return None;
    }
    let size = image.size();
    let data = image.data.as_deref()?;
    if data.len() < (size.x * size.y * 4) as usize {
        return None;
    }

    let mut min = size;
    let mut max = UVec2::ZERO;
    let mut found = false;
    let pixel_count = (size.x * size.y) as usize;
    for (index, _) in data
        .as_chunks::<4>().0.iter()
        .take(pixel_count)
        .enumerate()
        .filter(|(_, pixel)| pixel[3] != 0)
    {
        let index = index as u32;
        let pixel = UVec2::new(index % size.x, index / size.x);
        found = true;
        min = min.min(pixel);
        max = max.max(pixel + UVec2::ONE);
    }
    found.then_some(URect::from_corners(min, max))
}

#[expect(
    clippy::too_many_arguments,
    reason = "Chunk mesh assembly keeps placement, atlas, and render assets explicit."
)]
fn spawn_foliage_chunk(
    commands: &mut Commands,
    parent: Entity,
    compiled: &CompiledLevel,
    tile_size: f32,
    chunk: IVec2,
    placements: &[&EntityPlacement],
    source_geometries:
        &HashMap<bevy::asset::AssetId<Image>, FoliageSourceGeometry>,
    meshes: &mut Assets<Mesh>,
    material: &Handle<FoliageMaterial>,
    settings: FoliageSettings,
) -> Result<(), String> {
    let mut data = FoliageMeshData::default();
    for placement in placements {
        append_placement_mesh(
            &mut data,
            compiled,
            tile_size,
            placement,
            source_geometries,
            settings,
            false,
        )?;
    }
    let (mesh, aabb) = data.into_mesh_and_bounds();
    commands.entity(parent).with_children(|children| {
        children.spawn((
            Mesh2d(meshes.add(mesh)),
            MeshMaterial2d(material.clone()),
            Transform::from_xyz(0.0, 0.0, ENTITY_Z),
            aabb,
            RenderLayers::layer(0),
            StateDespawnMarker,
            Name::new(format!("FoliageChunk{}x{}", chunk.x, chunk.y)),
        ));
    });
    Ok(())
}

#[expect(
    clippy::too_many_arguments,
    reason = "Harvestable foliage preserves its authored placement and gameplay components."
)]
fn spawn_harvestable_foliage(
    commands: &mut Commands,
    parent: Entity,
    compiled: &CompiledLevel,
    tile_size: f32,
    placement: &EntityPlacement,
    source_geometries:
        &HashMap<bevy::asset::AssetId<Image>, FoliageSourceGeometry>,
    meshes: &mut Assets<Mesh>,
    material: &Handle<FoliageMaterial>,
    settings: FoliageSettings,
) -> Result<(), String> {
    let root = placement_world_root(compiled, tile_size, placement);
    let mut data = FoliageMeshData::default();
    append_placement_mesh(
        &mut data,
        compiled,
        tile_size,
        placement,
        source_geometries,
        settings,
        true,
    )?;
    let (mesh, aabb) = data.into_mesh_and_bounds();
    commands.entity(parent).with_children(|children| {
        let mut entity = children.spawn((
            Mesh2d(meshes.add(mesh)),
            MeshMaterial2d(material.clone()),
            Transform::from_xyz(root.x, root.y, ENTITY_Z + placement.z_offset),
            aabb,
            HarvestableFoliageVisual,
            RenderLayers::layer(0),
            StateDespawnMarker,
            Name::new(format!(
                "Entity:{}@{},{}",
                placement.kind, placement.tile_x, placement.tile_y
            )),
        ));
        if let Some(definition) = placement.harvest {
            entity.insert(
                harvestable_materials::HarvestablePlant::from_definition(
                    definition,
                ),
            );
        }
    });
    Ok(())
}

fn append_placement_mesh(
    data: &mut FoliageMeshData,
    compiled: &CompiledLevel,
    tile_size: f32,
    placement: &EntityPlacement,
    source_geometries:
        &HashMap<bevy::asset::AssetId<Image>, FoliageSourceGeometry>,
    settings: FoliageSettings,
    local_to_root: bool,
) -> Result<(), String> {
    let source = source_geometries
        .get(&placement.handle.id())
        .ok_or_else(|| "foliage source geometry is missing".to_string())?;
    let world_root = placement_world_root(compiled, tile_size, placement);
    let mesh_root = if local_to_root { Vec2::ZERO } else { world_root };
    let phase = foliage_phase(placement.tile_x, placement.tile_y);
    let wind_amplitude = if placement.kind == "grass" {
        settings.grass_wind_amplitude
    } else {
        settings.plant_wind_amplitude
    };
    data.append(foliage_sprite_spec(
        mesh_root,
        *source,
        placement.flip_x,
        placement.flip_y,
        phase,
        wind_amplitude,
    ));
    Ok(())
}

fn foliage_sprite_spec(
    sprite_root: Vec2,
    source: FoliageSourceGeometry,
    flip_x: bool,
    flip_y: bool,
    phase: f32,
    wind_amplitude: f32,
) -> FoliageSpriteSpec {
    let (min_x, max_x) = flipped_pixel_range(
        source.opaque_rect.min.x,
        source.opaque_rect.max.x,
        source.image_size.x,
        flip_x,
    );
    let (min_y, max_y) = flipped_pixel_range(
        source.opaque_rect.min.y,
        source.opaque_rect.max.y,
        source.image_size.y,
        flip_y,
    );
    let sprite_left = sprite_root.x - source.image_size.x as f32 * 0.5;
    let visible_min = Vec2::new(
        sprite_left + min_x as f32,
        sprite_root.y + (source.image_size.y - max_y) as f32,
    );
    let visible_max = Vec2::new(
        sprite_left + max_x as f32,
        sprite_root.y + (source.image_size.y - min_y) as f32,
    );
    FoliageSpriteSpec {
        min: visible_min,
        max: visible_max,
        deformation_root: Vec2::new(sprite_root.x, visible_min.y),
        uv_rect: source.opaque_uv_rect,
        flip_x,
        flip_y,
        phase,
        wind_amplitude,
    }
}

fn flipped_pixel_range(
    min: u32,
    max: u32,
    extent: u32,
    flip: bool,
) -> (u32, u32) {
    if flip {
        (extent - max, extent - min)
    } else {
        (min, max)
    }
}

fn placement_world_root(
    compiled: &CompiledLevel,
    tile_size: f32,
    placement: &EntityPlacement,
) -> Vec2 {
    Vec2::new(
        (placement.tile_x as f32 + 0.5) * tile_size
            + placement.offset_px.x as f32,
        (compiled.height as f32 - placement.tile_y as f32 - 1.0) * tile_size
            + placement.offset_px.y as f32,
    )
}

fn foliage_phase(tile_x: u32, tile_y: u32) -> f32 {
    let mixed = tile_x
        .wrapping_mul(0x9E37_79B9)
        .rotate_left(13)
        ^ tile_y.wrapping_mul(0x85EB_CA6B);
    mixed as f32 / u32::MAX as f32 * TAU
}

fn attach_moving_damage_source_interactors(
    mut commands: Commands,
    query: Query<
        (Entity, Option<&Collider>),
        (
            Added<DamageSource>,
            Or<(With<Projectile>, With<LinearVelocity>)>,
            Without<FoliageInteractor>,
        ),
    >,
) {
    for (entity, collider) in &query {
        let mut interactor = FoliageInteractor::projectile();
        if let Some(collider) = collider {
            let half_width = collider.half_extents().x;
            interactor.radius = (half_width + 3.0)
                .clamp(PROJECTILE_INFLUENCE_RADIUS, 16.0);
        }
        commands.entity(entity).try_insert(interactor);
    }
}

fn attach_player_attack_interactors(
    mut commands: Commands,
    query: Query<
        (Entity, &DamageSource, &Collider, &ChildOf),
        (
            Added<DamageSource>,
            Without<Projectile>,
            Without<LinearVelocity>,
            Without<FoliageInteractor>,
        ),
    >,
    players: Query<(), With<crate::game::player::Player>>,
) {
    for (entity, damage_source, collider, child_of) in &query {
        if child_of.parent() != damage_source.owner
            || players.get(damage_source.owner).is_err()
        {
            continue;
        }
        let half_extents = collider.half_extents();
        let radius = (half_extents.max_element() + 3.0).clamp(8.0, 32.0);
        commands
            .entity(entity)
            .try_insert(FoliageInteractor::player_attack(
                damage_source.owner,
                radius,
            ));
    }
}

fn track_foliage_interactors(
    time: Res<GameTime>,
    settings: Res<FoliageSettings>,
    mut query: Query<(&Transform, Option<&Collider>, &mut FoliageInteractor)>,
    players: Query<
        (&Transform, &crate::game::gentstate::Facing),
        With<crate::game::player::Player>,
    >,
) {
    for (transform, collider, mut interactor) in &mut query {
        update_interactor_motion(
            transform,
            collider,
            &mut interactor,
            &players,
            &settings,
            time.hz as f32,
        );
    }
}

fn update_interactor_motion(
    transform: &Transform,
    collider: Option<&Collider>,
    interactor: &mut FoliageInteractor,
    players: &Query<
        (&Transform, &crate::game::gentstate::Facing),
        With<crate::game::player::Player>,
    >,
    settings: &FoliageSettings,
    tick_rate: f32,
) {
    match interactor.mode {
        FoliageInteractorMode::Movement => {
            let position = transform.translation.xy();
            interactor.velocity_per_second = interactor
                .previous_position
                .map(|previous| {
                    movement_velocity(
                        previous,
                        position,
                        tick_rate,
                        settings.teleport_distance_per_tick,
                    )
                })
                .unwrap_or(Vec2::ZERO);
            interactor.influence_position = position;
            interactor.previous_position = Some(position);
        },
        FoliageInteractorMode::PlayerAttack { owner } => {
            let Ok((owner_transform, facing)) = players.get(owner) else {
                interactor.velocity_per_second = Vec2::ZERO;
                return;
            };
            let owner_position = owner_transform.translation.xy();
            let collider_center = collider
                .map(|collider| collider.local_aabb().unwrap_or_default().center())
                .unwrap_or(Vec2::ZERO);
            let (position, velocity) = player_attack_motion(
                owner_position,
                transform.translation.xy(),
                collider_center,
                facing.direction(),
                settings.attack_excitation_speed,
            );
            interactor.influence_position = position;
            interactor.velocity_per_second = velocity;
            interactor.previous_position = Some(position);
        },
    }
}

fn player_attack_motion(
    owner_position: Vec2,
    attack_local_position: Vec2,
    collider_center: Vec2,
    facing_direction: f32,
    excitation_speed: f32,
) -> (Vec2, Vec2) {
    let position = owner_position + attack_local_position + collider_center;
    let direction = (position - owner_position)
        .normalize_or(Vec2::X * facing_direction);
    (position, direction * excitation_speed)
}

fn movement_velocity(
    previous: Vec2,
    current: Vec2,
    tick_rate: f32,
    teleport_distance: f32,
) -> Vec2 {
    let delta = current - previous;
    if delta.length_squared() > teleport_distance * teleport_distance {
        return Vec2::ZERO;
    }
    delta * tick_rate
}

fn update_foliage_interaction_field(
    time: Res<GameTime>,
    settings: Res<FoliageSettings>,
    mut events: MessageReader<FoliageImpulseEvent>,
    mut runtime: ResMut<FoliageRuntime>,
    field: Option<ResMut<FoliageInteractionField>>,
    camera: Query<&Transform, With<MainCamera>>,
    interactors: Query<&FoliageInteractor>,
) {
    let seconds_per_tick = time.seconds_per_tick() as f32;
    runtime.time_seconds += seconds_per_tick;
    for event in events.read().copied() {
        if runtime.impulses.len() == MAX_IMPULSE_INFLUENCES {
            runtime.impulses.remove(0);
        }
        runtime.impulses.push(event.into());
    }
    runtime
        .impulses
        .retain_mut(|impulse| impulse.advance(seconds_per_tick));

    let Some(mut field) = field else { return };
    if let Ok(camera_transform) = camera.single() {
        field.recenter(camera_transform.translation.xy());
    }
    field.advance_spring(seconds_per_tick, settings.max_interaction_bend);

    let spring_speed = FOLIAGE_SPRING_STIFFNESS.sqrt();
    for interactor in &interactors {
        let speed = interactor.velocity_per_second.length();
        if speed < settings.interaction_min_speed {
            continue;
        }
        let direction = interactor.velocity_per_second / speed;
        let displacement =
            foliage_interaction_strength(speed, interactor, &settings);
        field.inject_mover(
            interactor.influence_position,
            interactor.radius,
            direction,
            displacement * spring_speed,
        );
    }

    for impulse in runtime.impulses.iter().copied() {
        let excitation_velocity = impulse.current_strength() * spring_speed;
        match impulse.propagation {
            FoliageImpulsePropagation::Instant { radius, .. } => {
                field.inject_radial(
                    impulse.origin,
                    radius,
                    excitation_velocity,
                );
            },
            FoliageImpulsePropagation::Ring { width, .. } => {
                field.inject_ring(
                    impulse.origin,
                    impulse.ring_radius,
                    width.max(8.0),
                    excitation_velocity,
                );
            },
        }
    }
}

fn animate_foliage_damage_flash(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut query: Query<
        (Entity, &Mesh2d, &mut DamageFlash),
        With<HarvestableFoliageVisual>,
    >,
) {
    for (entity, mesh_handle, mut flash) in &mut query {
        let finished = flash.current_ticks == flash.max_ticks;
        let brightness = if finished { 1.0 } else { 50.0 };
        if let Some(mut mesh) = meshes.get_mut(&mesh_handle.0) {
            let vertex_count = mesh.count_vertices();
            mesh.insert_attribute(
                ATTRIBUTE_FOLIAGE_FLASH,
                vec![brightness; vertex_count],
            );
        }
        if finished {
            commands.entity(entity).remove::<DamageFlash>();
        } else {
            flash.current_ticks += 1;
        }
    }
}

#[derive(SystemParam)]
struct FoliageRenderAssetParams<'w> {
    materials: ResMut<'w, Assets<FoliageMaterial>>,
    images: ResMut<'w, Assets<Image>>,
}

#[derive(SystemParam)]
struct FoliageRenderTimeParams<'w> {
    interpolation: Res<'w, RenderInterpolation>,
    presentation: Res<'w, theseeker_engine::time::PresentationTime>,
    pixel_view: Res<'w, PixelView>,
}

fn upload_foliage_render_state(
    render_assets: Option<Res<FoliageRenderAssets>>,
    settings: Res<FoliageSettings>,
    timing: FoliageRenderTimeParams,
    field: Option<ResMut<FoliageInteractionField>>,
    mut render_resources: FoliageRenderAssetParams,
) {
    let Some(render_assets) = render_assets else { return };
    let Some(mut field) = field else { return };
    let Some(view) = timing.pixel_view.0 else { return };
    if field.revision != field.uploaded_revision {
        let image_handle = field.image.clone();
        if let Some(mut image) = render_resources.images.get_mut(&image_handle) {
            field.write_image(&mut image, settings.max_interaction_bend);
        }
    }
    if let Some(mut material) = render_resources
        .materials
        .get_mut(&render_assets.material)
    {
        material.params.time_seconds = timing.presentation.elapsed_secs();
        let origin = field.origin_world();
        material.params.interaction_field_params = Vec4::new(
            origin.x,
            origin.y,
            timing.interpolation.alpha(),
            FOLIAGE_FIELD_CELL_SIZE,
        );
        material.params.camera_alignment_offset = view.mesh_offset();
    }
}


fn foliage_interaction_strength(
    speed: f32,
    interactor: &FoliageInteractor,
    settings: &FoliageSettings,
) -> f32 {
    let speed_factor =
        (speed / settings.full_bend_speed).clamp(0.0, 1.0).sqrt();
    settings.full_speed_interaction_bend
        * interactor.strength
        * speed_factor
}

fn clear_foliage_runtime(
    mut commands: Commands,
    mut runtime: ResMut<FoliageRuntime>,
) {
    *runtime = FoliageRuntime::default();
    commands.remove_resource::<FoliageRenderAssets>();
    commands.remove_resource::<FoliageInteractionField>();
}

/// Texture dependencies required before publishing a generated world.
pub(super) fn publication_images(world: &World) -> Vec<Handle<Image>> {
    world.resource::<Assets<FoliageMaterial>>().iter().flat_map(|(_, material)| {
        [material.atlas.clone(), material.interaction_field.clone()]
    }).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_pixel_rows_use_rigid_axis_aligned_quads() {
        let mut data = FoliageMeshData::default();
        data.append(FoliageSpriteSpec {
            min: Vec2::new(2.0, 20.0),
            max: Vec2::new(18.0, 24.0),
            deformation_root: Vec2::new(10.0, 20.0),
            uv_rect: Rect::from_corners(Vec2::ZERO, Vec2::ONE),
            flip_x: false,
            flip_y: false,
            phase: 0.5,
            wind_amplitude: 1.0,
        });
        assert_eq!(data.positions.len(), 16);
        assert_eq!(data.indices.len(), 24);
        for (row, vertices) in data.positions.as_chunks::<4>().0.iter().enumerate() {
            let lower_y = 20.0 + row as f32;
            let upper_y = lower_y + 1.0;
            assert_eq!(vertices[0], [2.0, lower_y, 0.0]);
            assert_eq!(vertices[1], [18.0, lower_y, 0.0]);
            assert_eq!(vertices[2], [18.0, upper_y, 0.0]);
            assert_eq!(vertices[3], [2.0, upper_y, 0.0]);
        }
        for weights in data.bend_weights.as_chunks::<4>().0.iter() {
            assert!(weights.iter().all(|weight| *weight == weights[0]));
        }
        assert!(data.bend_weights[0] < data.bend_weights[4]);
        assert!(data.bend_weights[4] < data.bend_weights[8]);
        assert!(data.bend_weights[8] < data.bend_weights[12]);
    }

    #[test]
    fn flipped_sprite_reverses_horizontal_uvs() {
        let mut normal = FoliageMeshData::default();
        let mut flipped = FoliageMeshData::default();
        let uv = Rect::from_corners(
            Vec2::new(0.25, 0.25),
            Vec2::new(0.75, 0.75),
        );
        normal.append(FoliageSpriteSpec {
            min: Vec2::ZERO,
            max: Vec2::splat(8.0),
            deformation_root: Vec2::ZERO,
            uv_rect: uv,
            flip_x: false,
            flip_y: false,
            phase: 0.0,
            wind_amplitude: 1.0,
        });
        flipped.append(FoliageSpriteSpec {
            min: Vec2::ZERO,
            max: Vec2::splat(8.0),
            deformation_root: Vec2::ZERO,
            uv_rect: uv,
            flip_x: true,
            flip_y: false,
            phase: 0.0,
            wind_amplitude: 1.0,
        });
        assert_eq!(normal.uvs[0][0], flipped.uvs[1][0]);
        assert_eq!(normal.uvs[1][0], flipped.uvs[0][0]);
    }

    #[test]
    fn transparent_padding_is_excluded_without_moving_visible_pixels() {
        let source = FoliageSourceGeometry {
            image_size: UVec2::new(16, 32),
            opaque_rect: URect::from_corners(
                UVec2::new(2, 20),
                UVec2::new(14, 25),
            ),
            opaque_uv_rect: Rect::from_corners(
                Vec2::new(0.1, 0.2),
                Vec2::new(0.3, 0.4),
            ),
        };
        let spec = foliage_sprite_spec(
            Vec2::new(100.0, -10.0),
            source,
            false,
            false,
            0.0,
            0.75,
        );
        assert_eq!(spec.min, Vec2::new(94.0, -3.0));
        assert_eq!(spec.max, Vec2::new(106.0, 2.0));
        assert_eq!(spec.deformation_root, Vec2::new(100.0, -3.0));
    }

    #[test]
    fn alpha_bounds_find_the_exact_visible_pixel_rectangle() {
        let size = UVec2::new(6, 5);
        let mut pixels = vec![0u8; (size.x * size.y * 4) as usize];
        for (x, y) in [(1, 2), (4, 2), (2, 3)] {
            let alpha_index = ((y * size.x + x) * 4 + 3) as usize;
            pixels[alpha_index] = 255;
        }
        let image = Image::new(
            bevy::render::render_resource::Extent3d {
                width: size.x,
                height: size.y,
                depth_or_array_layers: 1,
            },
            bevy::render::render_resource::TextureDimension::D2,
            pixels,
            TextureFormat::Rgba8UnormSrgb,
            RenderAssetUsages::MAIN_WORLD,
        );
        assert_eq!(
            opaque_alpha_rect(&image),
            Some(URect::from_corners(
                UVec2::new(1, 2),
                UVec2::new(5, 4),
            ))
        );
    }

    #[test]
    fn movement_rejects_teleports() {
        assert_eq!(
            movement_velocity(Vec2::ZERO, Vec2::new(49.0, 0.0), 96.0, 48.0),
            Vec2::ZERO
        );
        assert_eq!(
            movement_velocity(Vec2::ZERO, Vec2::new(0.5, 0.0), 96.0, 48.0),
            Vec2::new(48.0, 0.0)
        );
    }

    #[test]
    fn dash_speed_has_more_bend_than_running_speed() {
        let settings = FoliageSettings::default();
        let collider = Collider::cuboid(4.0, 8.0);
        let regular_interactor = FoliageInteractor::body(&collider);
        let player_interactor = FoliageInteractor::player_body(&collider);
        let running =
            foliage_interaction_strength(100.0, &player_interactor, &settings);
        let dashing = foliage_interaction_strength(
            settings.full_bend_speed,
            &player_interactor,
            &settings,
        );
        let regular_running = foliage_interaction_strength(
            100.0,
            &regular_interactor,
            &settings,
        );

        assert!(running > regular_running);
        assert!(dashing > running);
        assert_eq!(dashing, settings.max_interaction_bend);
    }

    #[test]
    fn player_attack_moves_away_from_player() {
        let (position, velocity) = player_attack_motion(
            Vec2::new(10.0, 20.0),
            Vec2::new(3.0, 1.0),
            Vec2::new(2.0, -1.0),
            -1.0,
            400.0,
        );
        assert_eq!(position, Vec2::new(15.0, 20.0));
        assert_eq!(velocity, Vec2::new(400.0, 0.0));

        let (_, fallback_velocity) = player_attack_motion(
            Vec2::ZERO,
            Vec2::ZERO,
            Vec2::ZERO,
            -1.0,
            400.0,
        );
        assert_eq!(fallback_velocity, Vec2::new(-400.0, 0.0));
    }

    #[test]
    fn field_injection_changes_velocity_without_jumping_displacement() {
        let mut field =
            FoliageInteractionField::new(Handle::<Image>::default());
        field.recenter(Vec2::ZERO);
        field.inject_mover(Vec2::ZERO, 12.0, Vec2::X, 20.0);

        assert!(field.cells.iter().any(|cell| cell.velocity > 10.0));
        assert!(field
            .cells
            .iter()
            .all(|cell| cell.displacement == 0.0));
    }

    #[test]
    fn spring_field_overshoots_and_preserves_each_previous_state() {
        let mut field =
            FoliageInteractionField::new(Handle::<Image>::default());
        let index = field_index(100, 80);
        field.cells[index].velocity = 20.0;
        let mut saw_positive = false;
        let mut saw_negative_after_positive = false;
        let mut prior = 0.0;

        for _ in 0..240 {
            field.advance_spring(1.0 / 96.0, 6.0);
            let cell = field.cells[index];
            assert!((cell.previous_displacement - prior).abs() < 0.000_001);
            saw_positive |= cell.displacement > 0.1;
            saw_negative_after_positive |=
                saw_positive && cell.displacement < -0.1;
            prior = cell.displacement;
        }

        assert!(saw_positive);
        assert!(saw_negative_after_positive);
        assert!(field.cells[index].displacement.abs() > 0.01);
    }

    #[test]
    fn recentering_preserves_cells_at_the_same_world_location() {
        let mut field =
            FoliageInteractionField::new(Handle::<Image>::default());
        field.recenter(Vec2::ZERO);
        let old_index = field_index(160, 96);
        field.cells[old_index].displacement = 3.0;

        field.recenter(Vec2::new(FOLIAGE_FIELD_CELL_SIZE, 0.0));

        let shifted_index = field_index(159, 96);
        assert_eq!(field.cells[shifted_index].displacement, 3.0);
        assert_eq!(field.cells[old_index].displacement, 0.0);
    }

    #[test]
    fn field_encoding_maps_signed_displacement_to_unorm() {
        assert_eq!(encode_field_displacement(-6.0, 6.0), 0);
        assert_eq!(encode_field_displacement(6.0, 6.0), u16::MAX);
        assert_eq!(encode_field_displacement(0.0, 6.0), 32_768);
    }

    #[test]
    fn camera_alignment_matches_the_pixel_perfect_composite() {
        let camera_center = Vec2::new(10.18, -3.18);
        let divisor = 6;
        let offset =
            PixelViewMapping::new(camera_center.extend(0.0), divisor, UVec2::ZERO).mesh_offset();
        let effective_center = camera_center - offset;
        let composite_center =
            snap_world_position_to_physical_pixels(camera_center, divisor);

        assert!((effective_center - composite_center).length() < 0.000_001);
        assert!(offset.abs().cmple(Vec2::splat(0.5 / divisor as f32)).all());
        assert_eq!(
            PixelViewMapping::new(composite_center.extend(0.0), divisor, UVec2::ZERO).mesh_offset(),
            Vec2::ZERO
        );
    }

    #[test]
    fn body_radius_uses_collider_width_and_clamps() {
        assert_eq!(FoliageInteractor::body(&Collider::cuboid(2.0, 5.0)).radius, 8.0);
        assert_eq!(FoliageInteractor::body(&Collider::cuboid(40.0, 5.0)).radius, 32.0);
        assert_eq!(
            FoliageInteractor::player_body(&Collider::cuboid(2.0, 5.0)).radius,
            8.0
        );
    }

    #[test]
    fn instant_impulse_decays_to_zero() {
        let mut impulse = ActiveFoliageImpulse::from(
            FoliageImpulseEvent::instant(Vec2::ZERO, 60.0, 4.0),
        );
        let initial = impulse.current_strength();
        impulse.age_seconds = 0.35;
        let finished = impulse.current_strength();
        assert_eq!(initial, 4.0);
        assert_eq!(finished, 0.0);
    }

    #[test]
    fn foliage_phase_is_deterministic_and_position_dependent() {
        assert_eq!(foliage_phase(3, 7), foliage_phase(3, 7));
        assert_ne!(foliage_phase(3, 7), foliage_phase(4, 7));
    }
}
