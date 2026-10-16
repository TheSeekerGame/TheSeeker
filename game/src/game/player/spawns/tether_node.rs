use bevy::shader::ShaderRef;
use crate::graphics::quad_mesh::{QuadMesh, add_mesh, publish_mesh};
use bevy::image::{ImageLoaderSettings, ImageSampler};
use bevy::mesh::{Mesh, Mesh2d, MeshVertexBufferLayoutRef};
use bevy::render::render_resource::{
    AsBindGroup, CompareFunction, RenderPipelineDescriptor, SpecializedMeshPipelineError,
};
use bevy::render::storage::ShaderBuffer;
use bevy::camera::visibility::{NoFrustumCulling, RenderLayers};
use bevy::sprite_render::{
    AlphaMode2d, ColorMaterial, Material2d, Material2dKey,
    Material2dPlugin, MeshMaterial2d,
};
use std::collections::HashMap;
use theseeker_engine::animation::SpriteAnimationBundle;
use theseeker_engine::time::{GameTickUpdate, GameTime};

use crate::game::combat::Health;
use crate::game::gentstate::Dead;
use crate::game::player::{Alignment, CraftMaterial, Player, PlayerStateSet};
use crate::game::world_objects::Capsule;
use crate::graphics::pixel_perfect::pixel_perfect_layers;
use crate::levelgen::harvestable_materials::{
    HarvestableResource, HARVEST_HITS_REQUIRED,
};
use crate::prelude::*;

pub(crate) const TETHER_RANGE: f32 = 192.0;
pub(crate) const TETHER_LINK_RANGE: f32 = 384.0;
pub(crate) const TETHER_HALF_EXTENTS: Vec2 = Vec2::new(10.0, 24.0);
pub(crate) const TETHER_VISUAL_Z: f32 = -0.25;
const TETHER_LINK_Z: f32 = 0.1;
const TETHER_RANGE_ART_Z: f32 = 8.0 * 0.000_001;
const TETHER_LINK_WIDTH: f32 = 1.25;
const TETHER_PRIMARY_PULSE_WIDTH: f32 = 2.75;
const TETHER_PRIMARY_PULSE_LENGTH: f32 = 18.0;
const TETHER_SECONDARY_PULSE_WIDTH: f32 = 2.25;
const TETHER_SECONDARY_PULSE_LENGTH: f32 = 12.0;
const TETHER_PULSE_SPEED: f32 = 288.0;
const TETHER_PULSE_ANIMATION_STEPS: u16 = 120;
const TETHER_OUTLINE_EPSILON: f32 = 0.001;
const TETHER_OUTLINE_RADIUS_QUANTUM: f32 = 0.5;
const TETHER_OUTLINE_JAGGEDNESS: f32 = 3.25;
const TETHER_OUTLINE_DEFORMATION: f32 = 1.3;
const TETHER_LINK_COLOR: Color = Color::BLACK;
const TETHER_RANGE_ART_PATH: &str = "fx/tether_range_art.png";
const TETHER_RANGE_ART_QUAD_RADIUS: f32 = TETHER_RANGE * 1.25;
const TETHER_RANGE_ART_SOURCE_RADIUS: f32 = 0.445;
const TETHER_RANGE_OUTLINE_ALPHA: f32 = 0.5;
const TETHER_RANGE_TENDRIL_ALPHA: f32 = 0.22;
const TETHER_RANGE_CLIP_INSET: f32 = 3.0;
const TETHER_HEARTBEAT_PERIOD_SECONDS: f32 = 1.25;
const TETHER_HEARTBEAT_PRIMARY_PHASE: f32 = 0.12;
const TETHER_HEARTBEAT_SECONDARY_PHASE: f32 = 0.31;
const TETHER_INFLUENCE_EXPAND_TICKS: u32 = 32;
const CAPSULE_INFLUENCE_GROW_TICKS: u32 = 72;

#[derive(Component, Debug, Default, Clone, Copy)]
pub struct TetherNode {
    influence_ticks: u32,
}

/// A capsule's visual influence spreading outward from it, as when it has just landed.
/// Gameplay influence is unaffected.
#[derive(Component, Debug, Default, Clone, Copy)]
pub(crate) struct CapsuleInfluenceGrowth {
    ticks: u32,
}

impl CapsuleInfluenceGrowth {
    fn radius(self) -> f32 {
        let progress = (self.ticks as f32 / CAPSULE_INFLUENCE_GROW_TICKS as f32).clamp(0.0, 1.0);
        TETHER_RANGE * (1.0 - (1.0 - progress).powi(3))
    }
}

impl TetherNode {
    fn influence_radius(self) -> f32 {
        TETHER_RANGE
            * (self.influence_ticks as f32
                / TETHER_INFLUENCE_EXPAND_TICKS as f32)
                .clamp(0.0, 1.0)
    }

    pub(crate) const fn is_network_active(self) -> bool {
        self.influence_ticks > 0
    }
}

#[derive(Component, Default)]
pub(crate) struct TetherRangeArtwork;

#[derive(Asset, TypePath, AsBindGroup, Clone)]
struct TetherRangeArtworkMaterial {
    #[texture(0)]
    #[sampler(1)]
    artwork: Handle<Image>,
    #[storage(2, read_only)]
    circles: Handle<ShaderBuffer>,
    #[uniform(3)]
    settings: Vec4,
}

impl TetherRangeArtworkMaterial {
    fn new(
        artwork: Handle<Image>,
        circles: Handle<ShaderBuffer>,
    ) -> Self {
        Self {
            artwork,
            circles,
            settings: Vec4::new(
                TETHER_RANGE_ART_SOURCE_RADIUS,
                TETHER_RANGE_TENDRIL_ALPHA,
                TETHER_RANGE_OUTLINE_ALPHA,
                TETHER_RANGE_CLIP_INSET,
            ),
        }
    }
}

impl Material2d for TetherRangeArtworkMaterial {
    fn fragment_shader() -> ShaderRef {
        "shaders/tether_range_artwork.wgsl".into()
    }

    fn alpha_mode(&self) -> AlphaMode2d {
        AlphaMode2d::Blend
    }

    fn specialize(
        descriptor: &mut RenderPipelineDescriptor,
        _layout: &MeshVertexBufferLayoutRef,
        _key: Material2dKey<Self>,
    ) -> Result<(), SpecializedMeshPipelineError> {
        let depth_stencil = descriptor
            .depth_stencil
            .as_mut()
            .expect("2D material pipeline has a depth attachment");
        depth_stencil.depth_write_enabled = Some(true);
        depth_stencil.depth_compare = Some(CompareFunction::Greater);
        Ok(())
    }
}

fn tether_link_material() -> ColorMaterial {
    ColorMaterial {
        color: TETHER_LINK_COLOR,
        // The ground uses the transparent 2D phase, so the connector must share
        // that phase for their z values to determine draw order.
        alpha_mode: AlphaMode2d::Blend,
        ..default()
    }
}

#[derive(Component, Default)]
struct TetherNetworkVeins;

#[derive(Clone, Copy, PartialEq)]
struct TetherConnector {
    entity: Option<Entity>,
    position: Vec2,
}

#[derive(Clone, Copy, PartialEq)]
struct InfluenceCircle {
    center: Vec2,
    radius: f32,
}

#[derive(Clone, Copy, PartialEq)]
struct NetworkInfluenceCircle {
    circle: InfluenceCircle,
    root_distance: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct VisualInfluenceCircle {
    pub(crate) center: Vec2,
    pub(crate) radius: f32,
    pub(crate) deformation: f32,
}

impl VisualInfluenceCircle {
    pub(crate) fn outline_seed(self) -> f32 {
        self.center.x * 0.017 + self.center.y * 0.023
    }

    pub(crate) fn outline_jaggedness(self) -> f32 {
        let expansion_scale = (self.radius / TETHER_RANGE).clamp(0.0, 1.0);
        TETHER_OUTLINE_JAGGEDNESS * expansion_scale
    }

    pub(crate) fn maximum_outline_radius(self) -> f32 {
        self.radius + self.outline_jaggedness() + self.deformation.abs()
    }
}

#[derive(Resource, Default, Debug, Clone, PartialEq)]
pub(crate) struct TetherInfluenceVisuals {
    pub(crate) circles: Vec<VisualInfluenceCircle>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct PowerLinkSegment {
    start: Vec2,
    end: Vec2,
    root_distance: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct PowerPulseSection {
    segment_index: usize,
    start_distance: f32,
    end_distance: f32,
    width: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct VeinSpan {
    start_distance: f32,
    end_distance: f32,
    width: f32,
}

#[derive(Default)]
struct TetherNetworkCache {
    connectors: Vec<TetherConnector>,
    power_segments: Vec<PowerLinkSegment>,
    tether_root_distances: HashMap<Entity, f32>,
}

impl TetherNetworkCache {
    fn update(&mut self, connectors: Vec<TetherConnector>) -> bool {
        if self.connectors == connectors {
            return false;
        }

        let network = build_network(&connectors);
        self.power_segments = network.segments;
        self.tether_root_distances = network.tether_distances;
        self.connectors = connectors;
        true
    }
}

pub struct TetherNodePlugin;

impl Plugin for TetherNodePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<TetherInfluenceVisuals>()
            .add_plugins(Material2dPlugin::<TetherRangeArtworkMaterial>::default())
            .add_systems(OnEnter(AppState::InGame), reset_tether_influence_visuals)
            .add_systems(
                GameTickUpdate,
                (sync_tether_network_visuals, tick_player_alignment)
                    .chain()
                    .in_set(PlayerStateSet::Behavior)
                    .run_if(in_state(AppState::InGame)),
            );
    }
}

fn reset_tether_influence_visuals(mut visuals: ResMut<TetherInfluenceVisuals>) {
    visuals.circles.clear();
}

pub(crate) fn spawn_tether_node(
    commands: &mut Commands,
    center: Vec2,
) -> Entity {
    commands
        .spawn((
            TetherNode::default(),
            HarvestableResource {
                material: CraftMaterial::Tether,
                drop_count: 1,
                hits_remaining: HARVEST_HITS_REQUIRED,
            },
            SpriteAnimationBundle::new_play_key("anim.objects.Tether"),
            Sprite {
                texture_atlas: Some(TextureAtlas::default()),
                ..default()
            },
            Transform::from_translation(center.extend(TETHER_VISUAL_Z)),
            GlobalTransform::default(),
            Visibility::Visible,
            InheritedVisibility::VISIBLE,
            ViewVisibility::default(),
            pixel_perfect_layers(),
            StateDespawnMarker,
            Name::new("TetherNode"),
        ))
        .id()
}

fn tick_player_alignment(
    mut commands: Commands,
    game_time: Res<GameTime>,
    mut players: Query<
        (Entity, &Transform, &mut Alignment, &mut Health),
        (With<Player>, Without<Dead>),
    >,
    tethers: Query<(&Transform, &TetherNode)>,
    capsules: Query<&Transform, With<Capsule>>,
) {
    let influences = collect_influence_circles(&tethers, &capsules);

    for (entity, transform, mut alignment, mut health) in players.iter_mut() {
        let player_pos = transform.translation.truncate();
        let connected = influences.iter().any(|circle| {
            player_pos.distance_squared(circle.center)
                <= circle.radius * circle.radius
        });
        let damage = alignment.tick(connected, game_time.seconds_per_tick());
        if damage == 0 {
            continue;
        }

        health.current = health.current.saturating_sub(damage);
        if health.current == 0 {
            commands.entity(entity).insert(Dead::default());
        }
    }
}

#[expect(clippy::too_many_arguments, reason = "Bevy system dependencies are expressed as explicit system parameters.")]
fn sync_tether_network_visuals(
    mut commands: Commands,
    game_time: Res<GameTime>,
    asset_server: Res<AssetServer>,
    mut tethers: Query<(Entity, &Transform, &mut TetherNode)>,
    capsule_positions: Query<&Transform, With<Capsule>>,
    mut capsules: Query<(Entity, &Transform, Option<&mut CapsuleInfluenceGrowth>), With<Capsule>>,
    mut range_artwork: Query<
        (
            Entity,
            &mut Mesh2d,
            &MeshMaterial2d<TetherRangeArtworkMaterial>,
        ),
        (With<TetherRangeArtwork>, Without<TetherNetworkVeins>),
    >,
    mut vein_entities: Query<(Entity, &mut Mesh2d), (With<TetherNetworkVeins>, Without<TetherRangeArtwork>)>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
    mut artwork_materials: ResMut<Assets<TetherRangeArtworkMaterial>>,
    mut artwork_buffers: ResMut<Assets<ShaderBuffer>>,
    mut published_influences: ResMut<TetherInfluenceVisuals>,
    mut network_cache: Local<TetherNetworkCache>,
    mut previous_pulse_frame: Local<Option<u16>>,
    mut previous_visual_influences: Local<Vec<VisualInfluenceCircle>>,
) {
    let connectors = collect_network_connectors(&mut tethers, &capsule_positions);
    let topology_changed = network_cache.update(connectors);

    let influences = update_tether_influence_circles(
        &mut commands,
        &mut tethers,
        &mut capsules,
        &network_cache.tether_root_distances,
    );

    let power_segments = &network_cache.power_segments;
    let elapsed_seconds = game_time.time_in_seconds() as f32;
    let pulse_frame = tether_pulse_animation_frame(elapsed_seconds);
    let heartbeat_phase =
        pulse_frame as f32 / TETHER_PULSE_ANIMATION_STEPS as f32;

    if power_segments.is_empty() {
        for (entity, _) in vein_entities.iter_mut() {
            commands.entity(entity).despawn();
        }
        *previous_pulse_frame = None;
    } else {
        let topology_unchanged = !topology_changed;
        sync_single_mesh_entity::<TetherNetworkVeins, ColorMaterial, _>(
            &mut commands,
            &mut vein_entities,
            &mut meshes,
            &mut materials,
            topology_unchanged && *previous_pulse_frame == Some(pulse_frame),
            || tether_vein_mesh(power_segments, pulse_frame),
            "TetherNetworkVeins",
            TETHER_LINK_Z,
            tether_link_material(),
            pixel_perfect_layers(),
        );
        *previous_pulse_frame = Some(pulse_frame);
    }

    if influences.is_empty() {
        for (entity, _, _) in range_artwork.iter_mut() {
            commands.entity(entity).despawn();
        }
        if !published_influences.circles.is_empty() {
            published_influences.circles.clear();
        }
        previous_visual_influences.clear();
        return;
    }

    let visual_influences = influences
        .iter()
        .map(|circle| visual_influence_circle(*circle, heartbeat_phase))
        .collect::<Vec<_>>();
    let fog_influences = influences
        .iter()
        .map(|circle| fog_influence_circle(*circle))
        .collect::<Vec<_>>();

    sync_tether_range_artwork(
        &mut commands,
        &asset_server,
        &mut range_artwork,
        &mut meshes,
        &mut artwork_materials,
        &mut artwork_buffers,
        &visual_influences,
        &previous_visual_influences,
    );
    *previous_visual_influences = visual_influences;
    if published_influences.circles != fog_influences {
        published_influences.circles = fog_influences;
    }
}

fn update_tether_influence_circles(
    commands: &mut Commands,
    tethers: &mut Query<
        (Entity, &Transform, &mut TetherNode),
        impl bevy::ecs::query::QueryFilter,
    >,
    capsules: &mut Query<
        (Entity, &Transform, Option<&mut CapsuleInfluenceGrowth>),
        impl bevy::ecs::query::QueryFilter,
    >,
    tether_root_distances: &HashMap<Entity, f32>,
) -> Vec<NetworkInfluenceCircle> {
    let mut influences = capsules
        .iter_mut()
        .map(|(entity, transform, growth)| NetworkInfluenceCircle {
            circle: InfluenceCircle {
                center: transform.translation.truncate(),
                radius: grow_capsule_influence(commands, entity, growth),
            },
            root_distance: 0.0,
        })
        .collect::<Vec<_>>();
    for (entity, transform, mut tether) in tethers.iter_mut() {
        if tether_root_distances.contains_key(&entity) {
            tether.influence_ticks = (tether.influence_ticks + 1)
                .min(TETHER_INFLUENCE_EXPAND_TICKS);
        } else {
            tether.influence_ticks = 0;
        }
        let radius = tether.influence_radius();
        if radius <= TETHER_OUTLINE_EPSILON {
            continue;
        }
        let Some(root_distance) = tether_root_distances.get(&entity).copied()
        else {
            continue;
        };
        influences.push(NetworkInfluenceCircle {
            circle: InfluenceCircle {
                center: transform.translation.truncate(),
                radius,
            },
            root_distance,
        });
    }
    influences.sort_by(|a, b| {
        a.circle
            .center
            .x
            .total_cmp(&b.circle.center.x)
            .then_with(|| {
                a.circle.center.y.total_cmp(&b.circle.center.y)
            })
            .then_with(|| a.circle.radius.total_cmp(&b.circle.radius))
    });
    influences
}


fn collect_network_connectors(
    tethers: &mut Query<
        (Entity, &Transform, &mut TetherNode),
        impl bevy::ecs::query::QueryFilter,
    >,
    capsules: &Query<
        &Transform,
        impl bevy::ecs::query::QueryFilter,
    >,
) -> Vec<TetherConnector> {
    let mut connectors = tethers
        .iter_mut()
        .map(|(entity, transform, _)| TetherConnector {
            entity: Some(entity),
            position: transform.translation.truncate(),
        })
        .collect::<Vec<_>>();
    connectors.extend(capsules.iter().map(|transform| TetherConnector {
        entity: None,
        position: transform.translation.truncate(),
    }));
    connectors.sort_by(|a, b| {
        a.position
            .x
            .total_cmp(&b.position.x)
            .then_with(|| a.position.y.total_cmp(&b.position.y))
    });
    connectors
}

struct NetworkBuild {
    links: Vec<(usize, usize)>,
    root_distances: Vec<Option<f32>>,
    segments: Vec<PowerLinkSegment>,
    tether_distances: HashMap<Entity, f32>,
}

fn build_network(connectors: &[TetherConnector]) -> NetworkBuild {
    let mut connected = connectors
        .iter()
        .map(|connector| connector.entity.is_none())
        .collect::<Vec<_>>();
    let mut nearest_parent = vec![None; connectors.len()];
    let mut nearest_distance_squared =
        vec![f32::INFINITY; connectors.len()];

    for root in 0..connectors.len() {
        if connected[root] {
            update_nearest_network_parents(
                connectors,
                &connected,
                root,
                &mut nearest_parent,
                &mut nearest_distance_squared,
            );
        }
    }

    let mut network = NetworkBuild {
        links: Vec::new(),
        root_distances: connectors.iter().map(|connector| connector.entity.is_none().then_some(0.0)).collect(),
        segments: Vec::new(),
        tether_distances: HashMap::new(),
    };
    loop {
        let next = (0..connectors.len())
            .filter(|index| {
                !connected[*index] && nearest_parent[*index].is_some()
            })
            .min_by(|a, b| {
                nearest_distance_squared[*a]
                    .total_cmp(&nearest_distance_squared[*b])
                    .then_with(|| a.cmp(b))
            });
        let Some(next) = next else { break; };
        let parent = nearest_parent[next].expect("reachable connector has parent");
        network.connect(connectors, parent, next);
        connected[next] = true;
        update_nearest_network_parents(
            connectors,
            &connected,
            next,
            &mut nearest_parent,
            &mut nearest_distance_squared,
        );
    }

    network
}

impl NetworkBuild {
    fn connect(&mut self, connectors: &[TetherConnector], parent: usize, child: usize) {
        let root_distance = self.root_distances[parent].expect("network parent is connected to a capsule");
        let start = connectors[parent].position;
        let end = connectors[child].position;
        let child_distance = root_distance + start.distance(end);
        self.links.push((parent, child));
        self.root_distances[child] = Some(child_distance);
        self.segments.push(PowerLinkSegment { start, end, root_distance });
        if let Some(entity) = connectors[child].entity {
            self.tether_distances.insert(entity, child_distance);
        }
    }
}

fn update_nearest_network_parents(
    connectors: &[TetherConnector],
    connected: &[bool],
    source: usize,
    nearest_parent: &mut [Option<usize>],
    nearest_distance_squared: &mut [f32],
) {
    for target in 0..connectors.len() {
        if connected[target] {
            continue;
        }
        let distance_squared = connectors[source]
            .position
            .distance_squared(connectors[target].position);
        if distance_squared > TETHER_LINK_RANGE * TETHER_LINK_RANGE {
            continue;
        }
        let current_parent = nearest_parent[target];
        let is_better = distance_squared < nearest_distance_squared[target]
            || (distance_squared == nearest_distance_squared[target]
                && current_parent.is_none_or(|parent| source < parent));
        if is_better {
            nearest_parent[target] = Some(source);
            nearest_distance_squared[target] = distance_squared;
        }
    }
}





pub(crate) fn tether_connector_in_range(
    position: Vec2,
    mut connector_positions: impl Iterator<Item = Vec2>,
) -> bool {
    connector_positions.any(|connector| {
        position.distance_squared(connector)
            <= TETHER_LINK_RANGE * TETHER_LINK_RANGE
    })
}

fn collect_influence_circles(
    tethers: &Query<
        (&Transform, &TetherNode),
        impl bevy::ecs::query::QueryFilter,
    >,
    capsules: &Query<
        &Transform,
        impl bevy::ecs::query::QueryFilter,
    >,
) -> Vec<InfluenceCircle> {
    let mut circles = capsules
        .iter()
        .map(|transform| InfluenceCircle {
            center: transform.translation.truncate(),
            radius: TETHER_RANGE,
        })
        .collect::<Vec<_>>();
    circles.extend(tethers.iter().filter_map(|(transform, tether)| {
        let radius = tether.influence_radius();
        (radius > TETHER_OUTLINE_EPSILON).then_some(InfluenceCircle {
            center: transform.translation.truncate(),
            radius,
        })
    }));
    sort_influence_circles(&mut circles);
    circles
}

fn sort_influence_circles(circles: &mut [InfluenceCircle]) {
    circles.sort_by(|a, b| {
        a.center
            .x
            .total_cmp(&b.center.x)
            .then_with(|| a.center.y.total_cmp(&b.center.y))
            .then_with(|| a.radius.total_cmp(&b.radius))
    });
}

#[expect(
    clippy::too_many_arguments,
    reason = "Mesh sync keeps command access, entity query, mesh and material assets, geometry, render state, and naming explicit."
)]
fn sync_single_mesh_entity<
    T: Component + Default,
    M: Material2d,
    F: FnOnce() -> Mesh,
>(
    commands: &mut Commands,
    entities: &mut Query<
        (Entity, &mut Mesh2d),
        impl bevy::ecs::query::QueryFilter,
    >,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<M>,
    unchanged: bool,
    build_mesh: F,
    name: &'static str,
    z: f32,
    material: M,
    render_layers: RenderLayers,
) {
    let mut existing = entities.iter_mut();
    let active = existing.next();
    for (entity, _) in existing {
        commands.entity(entity).despawn();
    }
    if active.is_some() && unchanged {
        return;
    }

    let mesh = build_mesh();

    if let Some((_, mut mesh_handle)) = active {
        publish_mesh(meshes, &mut mesh_handle.0, mesh);
    } else {
        let material = materials.add(material);
        commands.spawn((
            T::default(),
            NoFrustumCulling,
            Mesh2d(add_mesh(meshes, mesh)),
            MeshMaterial2d(material),
            Transform::from_translation(Vec3::Z * z),
            GlobalTransform::default(),
            Visibility::Visible,
            InheritedVisibility::VISIBLE,
            ViewVisibility::default(),
            render_layers,
            StateDespawnMarker,
            Name::new(name),
        ));
    }
}

#[expect(
    clippy::too_many_arguments,
    reason = "Artwork sync owns the entity, mesh, material, texture, and GPU circle data."
)]
fn sync_tether_range_artwork(
    commands: &mut Commands,
    asset_server: &AssetServer,
    entities: &mut Query<
        (
            Entity,
            &mut Mesh2d,
            &MeshMaterial2d<TetherRangeArtworkMaterial>,
        ),
        impl bevy::ecs::query::QueryFilter,
    >,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<TetherRangeArtworkMaterial>,
    buffers: &mut Assets<ShaderBuffer>,
    circles: &[VisualInfluenceCircle],
    previous_circles: &[VisualInfluenceCircle],
) {
    let mut existing = entities.iter_mut();
    let active = existing.next();
    for (entity, _, _) in existing {
        commands.entity(entity).despawn();
    }

    let topology_unchanged = circles.len() == previous_circles.len()
        && circles
            .iter()
            .zip(previous_circles)
            .all(|(circle, previous)| circle.center == previous.center);
    let circle_data_unchanged = circles == previous_circles;
    let circle_data = tether_range_artwork_buffer_data(circles);

    if let Some((_, mut mesh_handle, material_handle)) = active {
        if !topology_unchanged {
            publish_mesh(meshes, &mut mesh_handle.0, tether_range_artwork_mesh(circles));
        }
        if !circle_data_unchanged {
            update_tether_range_artwork_buffer(
                material_handle,
                materials,
                buffers,
                &circle_data,
            );
        }
        return;
    }

    let artwork = asset_server.load_builder().with_settings(
        |settings: &mut ImageLoaderSettings| {
            settings.sampler = ImageSampler::nearest();
        },
    ).load(TETHER_RANGE_ART_PATH);
    let circle_buffer = buffers.add(tether_range_artwork_buffer(&circle_data));
    let material = materials.add(TetherRangeArtworkMaterial::new(
        artwork,
        circle_buffer,
    ));
    commands.spawn((
        TetherRangeArtwork,
        NoFrustumCulling,
        Mesh2d(add_mesh(meshes, tether_range_artwork_mesh(circles))),
        MeshMaterial2d(material),
        Transform::from_translation(Vec3::Z * TETHER_RANGE_ART_Z),
        GlobalTransform::default(),
        Visibility::Visible,
        InheritedVisibility::VISIBLE,
        ViewVisibility::default(),
        pixel_perfect_layers(),
        StateDespawnMarker,
        Name::new("TetherRangeArtwork"),
    ));
}

fn update_tether_range_artwork_buffer(
    material_handle: &MeshMaterial2d<TetherRangeArtworkMaterial>,
    materials: &mut Assets<TetherRangeArtworkMaterial>,
    buffers: &mut Assets<ShaderBuffer>,
    circle_data: &[[f32; 4]],
) {
    let Some(mut material) = materials.get_mut(&material_handle.0) else {
        return;
    };
    let buffer_handle = material.circles.clone();
    let Some(mut buffer) = buffers.get_mut(&buffer_handle) else {
        return;
    };
    if buffer.buffer_description.size == std::mem::size_of_val(circle_data) as u64 {
        buffer.set_data(circle_data);
        return;
    }
    drop(buffer);
    // A new handle makes material preparation wait for the resized allocation;
    // rebinding an existing handle in the same frame could retain its old GPU buffer.
    material.circles = buffers.add(tether_range_artwork_buffer(circle_data));
}

fn tether_range_artwork_buffer(circle_data: &[[f32; 4]]) -> ShaderBuffer {
    let mut buffer = ShaderBuffer::from(circle_data);
    buffer.buffer_description.size = std::mem::size_of_val(circle_data) as u64;
    buffer
}

fn tether_range_artwork_buffer_data(
    circles: &[VisualInfluenceCircle],
) -> Vec<[f32; 4]> {
    circles
        .iter()
        .map(|circle| {
            [
                circle.center.x,
                circle.center.y,
                circle.radius,
                circle.deformation,
            ]
        })
        .collect()
}

fn tether_range_artwork_mesh(circles: &[VisualInfluenceCircle]) -> Mesh {
    let mut mesh = QuadMesh::with_capacity(circles.len());
    for (index, circle) in circles.iter().enumerate() {
        let radius = Vec2::splat(TETHER_RANGE_ART_QUAD_RADIUS);
        mesh.rect(circle.center - radius, circle.center + radius, [index as f32, 0.0, 0.0, 0.0]);
    }
    mesh.finish_colored()
}

fn tether_vein_mesh(segments: &[PowerLinkSegment], animation_frame: u16) -> Mesh {
    let mut mesh = QuadMesh::default();
    let heartbeat_phase = animation_frame as f32 / TETHER_PULSE_ANIMATION_STEPS as f32;
    let pulse_sections = power_pulse_sections(segments, heartbeat_phase);
    for (segment_index, segment) in segments.iter().enumerate() {
        append_vein_segment(segment_index, *segment, &pulse_sections, &mut mesh);
    }
    mesh.finish()
}

fn append_vein_segment(
    segment_index: usize,
    segment: PowerLinkSegment,
    pulse_sections: &[PowerPulseSection],
    mesh: &mut QuadMesh,
) {
    let delta = segment.end - segment.start;
    let segment_length = delta.length();
    if segment_length <= TETHER_OUTLINE_EPSILON {
        return;
    }

    let direction = delta / segment_length;
    for span in vein_spans(segment_index, segment_length, pulse_sections) {
        mesh.line(
            segment.start + direction * span.start_distance,
            segment.start + direction * span.end_distance,
            span.width,
        );
    }
}

fn vein_spans(
    segment_index: usize,
    segment_length: f32,
    pulse_sections: &[PowerPulseSection],
) -> Vec<VeinSpan> {
    let segment_pulses = pulse_sections
        .iter()
        .filter(|section| section.segment_index == segment_index)
        .collect::<Vec<_>>();
    let mut boundaries = Vec::with_capacity(segment_pulses.len() * 2 + 2);
    boundaries.extend([0.0, segment_length]);
    for section in &segment_pulses {
        boundaries.extend([section.start_distance, section.end_distance]);
    }
    boundaries.sort_by(f32::total_cmp);
    boundaries.dedup_by(|a, b| (*a - *b).abs() <= TETHER_OUTLINE_EPSILON);

    boundaries
        .windows(2)
        .map(|pair| {
            let start_distance = pair[0];
            let end_distance = pair[1];
            let midpoint = (start_distance + end_distance) * 0.5;
            let width = segment_pulses
                .iter()
                .filter(|section| {
                    midpoint >= section.start_distance
                        && midpoint <= section.end_distance
                })
                .map(|section| section.width)
                .fold(TETHER_LINK_WIDTH, f32::max);
            VeinSpan {
                start_distance,
                end_distance,
                width,
            }
        })
        .collect()
}

fn tether_pulse_animation_frame(elapsed_seconds: f32) -> u16 {
    let heartbeat_phase = tether_heartbeat_phase(elapsed_seconds);
    (heartbeat_phase * TETHER_PULSE_ANIMATION_STEPS as f32).floor() as u16
}

fn power_pulse_sections(
    segments: &[PowerLinkSegment],
    heartbeat_phase: f32,
) -> Vec<PowerPulseSection> {
    let wavelength =
        TETHER_PULSE_SPEED * TETHER_HEARTBEAT_PERIOD_SECONDS;
    let mut sections = Vec::with_capacity(segments.len() * 2);
    append_power_wave_sections(
        &mut sections,
        segments,
        (heartbeat_phase - TETHER_HEARTBEAT_PRIMARY_PHASE)
            .rem_euclid(1.0)
            * wavelength,
        TETHER_PRIMARY_PULSE_LENGTH,
        TETHER_PRIMARY_PULSE_WIDTH,
        wavelength,
    );
    append_power_wave_sections(
        &mut sections,
        segments,
        (heartbeat_phase - TETHER_HEARTBEAT_SECONDARY_PHASE)
            .rem_euclid(1.0)
            * wavelength,
        TETHER_SECONDARY_PULSE_LENGTH,
        TETHER_SECONDARY_PULSE_WIDTH,
        wavelength,
    );
    sections
}

fn append_power_wave_sections(
    sections: &mut Vec<PowerPulseSection>,
    segments: &[PowerLinkSegment],
    wave_distance: f32,
    pulse_length: f32,
    pulse_width: f32,
    wavelength: f32,
) {
    let half_length = pulse_length * 0.5;
    for (segment_index, segment) in segments.iter().enumerate() {
        let delta = segment.end - segment.start;
        let segment_length = delta.length();
        if segment_length <= TETHER_OUTLINE_EPSILON {
            continue;
        }

        let global_start = segment.root_distance;
        let global_end = global_start + segment_length;
        let first_wave_index = ((global_start - half_length - wave_distance)
            / wavelength)
            .ceil()
            .max(0.0) as u32;
        let mut center =
            wave_distance + first_wave_index as f32 * wavelength;
        while center - half_length < global_end {
            sections.extend(power_wave_overlap_section(
                segment_index,
                segment.root_distance,
                segment_length,
                center,
                half_length,
                pulse_width,
            ));
            center += wavelength;
        }
    }
}

fn power_wave_overlap_section(
    segment_index: usize,
    root_distance: f32,
    segment_length: f32,
    center: f32,
    half_length: f32,
    pulse_width: f32,
) -> Option<PowerPulseSection> {
    let global_start = root_distance;
    let global_end = global_start + segment_length;
    let overlap_start = (center - half_length).max(global_start);
    let overlap_end = (center + half_length).min(global_end);
    (overlap_end > overlap_start + TETHER_OUTLINE_EPSILON).then_some({
        PowerPulseSection {
            segment_index,
            start_distance: overlap_start - global_start,
            end_distance: overlap_end - global_start,
            width: pulse_width,
        }
    })
}

fn tether_heartbeat_displacement(phase: f32) -> f32 {
    let primary = heartbeat_bump(
        phase,
        TETHER_HEARTBEAT_PRIMARY_PHASE,
        0.08,
    );
    let secondary = heartbeat_bump(
        phase,
        TETHER_HEARTBEAT_SECONDARY_PHASE,
        0.10,
    );
    primary * 0.014 + secondary * 0.007
}

fn grow_capsule_influence(
    commands: &mut Commands,
    capsule: Entity,
    growth: Option<Mut<CapsuleInfluenceGrowth>>,
) -> f32 {
    let Some(mut growth) = growth else {
        return TETHER_RANGE;
    };
    growth.ticks += 1;
    if growth.ticks >= CAPSULE_INFLUENCE_GROW_TICKS {
        commands.entity(capsule).remove::<CapsuleInfluenceGrowth>();
    }
    growth.radius()
}

fn visual_influence_circle(
    influence: NetworkInfluenceCircle,
    heartbeat_phase: f32,
) -> VisualInfluenceCircle {
    let wavelength =
        TETHER_PULSE_SPEED * TETHER_HEARTBEAT_PERIOD_SECONDS;
    let local_phase = (heartbeat_phase
        - influence.root_distance / wavelength)
        .rem_euclid(1.0);
    let displacement = tether_heartbeat_displacement(local_phase);
    let radius = influence.circle.radius;
    VisualInfluenceCircle {
        center: influence.circle.center,
        radius: quantize_tether_outline_radius(radius * (1.0 + displacement)),
        deformation: quantize_tether_outline_radius(
            radius * displacement * TETHER_OUTLINE_DEFORMATION,
        ),
    }
}

fn fog_influence_circle(
    influence: NetworkInfluenceCircle,
) -> VisualInfluenceCircle {
    VisualInfluenceCircle {
        center: influence.circle.center,
        radius: quantize_tether_outline_radius(influence.circle.radius),
        deformation: 0.0,
    }
}

fn tether_heartbeat_phase(elapsed_seconds: f32) -> f32 {
    (elapsed_seconds / TETHER_HEARTBEAT_PERIOD_SECONDS).rem_euclid(1.0)
}

fn quantize_tether_outline_radius(radius: f32) -> f32 {
    (radius / TETHER_OUTLINE_RADIUS_QUANTUM).round()
        * TETHER_OUTLINE_RADIUS_QUANTUM
}

fn heartbeat_bump(phase: f32, center: f32, half_width: f32) -> f32 {
    let wrapped = (phase - center + 0.5).rem_euclid(1.0) - 0.5;
    let strength = (1.0 - wrapped.abs() / half_width).clamp(0.0, 1.0);
    strength * strength * (3.0 - 2.0 * strength)
}


#[cfg(test)]
fn tether_range_artwork_deformed_radius(
    circle: VisualInfluenceCircle,
    angle: f32,
) -> f32 {
    let seed = circle.outline_seed();
    let deformation = (angle * 2.0 + seed * 1.13).sin() * 0.62
        + (angle * 3.0 - seed * 0.73).sin() * 0.38;
    (circle.radius + circle.deformation * deformation)
        .max(TETHER_OUTLINE_EPSILON)
}

#[cfg(test)]
fn tether_range_artwork_circle_hides_point(
    current_index: usize,
    current: VisualInfluenceCircle,
    other_index: usize,
    other: VisualInfluenceCircle,
    world_position: Vec2,
    clip_inset: f32,
) -> bool {
    let center_distance = current.center.distance(other.center);
    if center_distance <= TETHER_OUTLINE_EPSILON {
        return other.radius > current.radius
            || (other.radius == current.radius
                && other_index < current_index);
    }

    let offset = world_position - other.center;
    let angle = offset.y.atan2(offset.x);
    let clip_radius =
        (tether_range_artwork_deformed_radius(other, angle) - clip_inset)
            .max(0.0);
    offset.length() < clip_radius
}

#[cfg(test)]
fn tether_range_artwork_fragment_hidden(
    current_index: usize,
    world_position: Vec2,
    is_tendril: bool,
    circles: &[VisualInfluenceCircle],
) -> bool {
    let current = circles[current_index];
    let offset = world_position - current.center;
    let angle = offset.y.atan2(offset.x);
    let root_position = current.center
        + Vec2::new(angle.cos(), angle.sin())
            * tether_range_artwork_deformed_radius(current, angle);

    circles.iter().enumerate().any(|(other_index, other)| {
        other_index != current_index
            && (tether_range_artwork_circle_hides_point(
                current_index,
                current,
                other_index,
                *other,
                world_position,
                TETHER_RANGE_CLIP_INSET,
            ) || (is_tendril
                && tether_range_artwork_circle_hides_point(
                    current_index,
                    current,
                    other_index,
                    *other,
                    root_position,
                    -TETHER_RANGE_CLIP_INSET,
                )))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const _: () = {
        assert!(TETHER_VISUAL_Z < 0.0);
        assert!(TETHER_RANGE_ART_Z > 0.0);
        assert!(TETHER_LINK_Z > TETHER_RANGE_ART_Z);
        assert!(TETHER_LINK_Z < 1.0);
        assert!(TETHER_RANGE_TENDRIL_ALPHA < TETHER_RANGE_OUTLINE_ALPHA);
        assert!(TETHER_RANGE_CLIP_INSET > 0.0);
    };

    #[test]
    fn power_pulse_moves_from_parent_toward_child() {
        let segments = [PowerLinkSegment {
            start: Vec2::ZERO,
            end: Vec2::new(100.0, 0.0),
            root_distance: 0.0,
        }];
        let mut initial = Vec::new();
        let mut advanced = Vec::new();

        append_power_wave_sections(
            &mut initial,
            &segments,
            0.0,
            18.0,
            2.75,
            90.0,
        );
        append_power_wave_sections(
            &mut advanced,
            &segments,
            12.0,
            18.0,
            2.75,
            90.0,
        );

        assert_eq!(initial[0].segment_index, 0);
        assert_eq!(initial[0].start_distance, 0.0);
        assert_eq!(initial[0].end_distance, 9.0);
        assert_eq!(advanced[0].segment_index, 0);
        assert_eq!(advanced[0].start_distance, 3.0);
        assert_eq!(advanced[0].end_distance, 21.0);
    }

    #[test]
    fn tether_network_uses_one_nearest_parent_link_per_tether() {
        let tether = Some(Entity::PLACEHOLDER);
        let connectors = [
            TetherConnector {
                entity: None,
                position: Vec2::ZERO,
            },
            TetherConnector {
                entity: tether,
                position: Vec2::new(100.0, 0.0),
            },
            TetherConnector {
                entity: tether,
                position: Vec2::new(150.0, 0.0),
            },
            TetherConnector {
                entity: tether,
                position: Vec2::new(200.0, 0.0),
            },
        ];

        assert_eq!(
            build_network(&connectors).links,
            vec![(0, 1), (1, 2), (2, 3)],
        );
    }

    #[test]
    fn tether_network_cache_rebuilds_only_for_connector_changes() {
        let connectors = vec![
            TetherConnector {
                entity: None,
                position: Vec2::ZERO,
            },
            TetherConnector {
                entity: Some(Entity::PLACEHOLDER),
                position: Vec2::new(100.0, 0.0),
            },
        ];
        let mut cache = TetherNetworkCache::default();

        assert!(cache.update(connectors.clone()));
        assert!(!cache.update(connectors.clone()));

        let mut moved = connectors;
        moved[1].position.x += 1.0;
        assert!(cache.update(moved));
    }

    #[test]
    fn tether_links_preserve_capsule_to_tether_direction() {
        let connectors = [
            TetherConnector {
                entity: Some(Entity::PLACEHOLDER),
                position: Vec2::new(100.0, 0.0),
            },
            TetherConnector {
                entity: Some(Entity::PLACEHOLDER),
                position: Vec2::new(200.0, 0.0),
            },
            TetherConnector {
                entity: None,
                position: Vec2::ZERO,
            },
        ];

        assert_eq!(build_network(&connectors).links, vec![(2, 0), (0, 1)]);
    }

    #[test]
    fn network_root_distances_follow_the_capsule_link_path() {
        let connectors = [
            TetherConnector {
                entity: None,
                position: Vec2::ZERO,
            },
            TetherConnector {
                entity: Some(Entity::PLACEHOLDER),
                position: Vec2::new(100.0, 0.0),
            },
            TetherConnector {
                entity: Some(Entity::PLACEHOLDER),
                position: Vec2::new(250.0, 0.0),
            },
        ];
        let network = build_network(&connectors);
        assert_eq!(network.links, vec![(0, 1), (1, 2)]);
        assert_eq!(
            network.root_distances,
            vec![Some(0.0), Some(100.0), Some(250.0)],
        );
    }

    #[test]
    fn nearest_links_do_not_bridge_beyond_connection_range() {
        let connectors = [
            TetherConnector {
                entity: None,
                position: Vec2::ZERO,
            },
            TetherConnector {
                entity: Some(Entity::PLACEHOLDER),
                position: Vec2::new(TETHER_LINK_RANGE, 0.0),
            },
            TetherConnector {
                entity: Some(Entity::PLACEHOLDER),
                position: Vec2::new(TETHER_LINK_RANGE * 2.0 + 1.0, 0.0),
            },
        ];

        assert_eq!(build_network(&connectors).links, vec![(0, 1)]);
    }

    #[test]
    fn power_pulse_continues_across_tether_nodes() {
        let segments = [
            PowerLinkSegment {
                start: Vec2::ZERO,
                end: Vec2::new(10.0, 0.0),
                root_distance: 0.0,
            },
            PowerLinkSegment {
                start: Vec2::new(10.0, 0.0),
                end: Vec2::new(20.0, 0.0),
                root_distance: 10.0,
            },
        ];
        let mut sections = Vec::new();

        append_power_wave_sections(
            &mut sections,
            &segments,
            10.0,
            4.0,
            2.0,
            90.0,
        );

        assert_eq!(sections.len(), 2);
        assert_eq!(sections[0].segment_index, 0);
        assert_eq!(sections[0].start_distance, 8.0);
        assert_eq!(sections[0].end_distance, 10.0);
        assert_eq!(sections[1].segment_index, 1);
        assert_eq!(sections[1].start_distance, 0.0);
        assert_eq!(sections[1].end_distance, 2.0);
    }

    fn tether_heartbeat_scale(elapsed_seconds: f32) -> f32 {
        1.0 + tether_heartbeat_displacement(tether_heartbeat_phase(elapsed_seconds))
    }

    #[test]
    fn heartbeat_is_periodic_and_restrained() {
        let baseline = tether_heartbeat_scale(0.0);
        let repeated =
            tether_heartbeat_scale(TETHER_HEARTBEAT_PERIOD_SECONDS);
        let primary_peak = tether_heartbeat_scale(
            TETHER_HEARTBEAT_PERIOD_SECONDS
                * TETHER_HEARTBEAT_PRIMARY_PHASE,
        );

        assert!((baseline - repeated).abs() < 1e-6);
        assert!(primary_peak > baseline);
        assert!(primary_peak <= 1.021);
    }

    #[test]
    fn circle_heartbeat_arrives_with_the_vein_pulse() {
        let root = NetworkInfluenceCircle {
            circle: InfluenceCircle {
                center: Vec2::ZERO,
                radius: TETHER_RANGE,
            },
            root_distance: 0.0,
        };
        let travel_distance = TETHER_PULSE_SPEED
            * TETHER_HEARTBEAT_PERIOD_SECONDS
            * 0.25;
        let remote = NetworkInfluenceCircle {
            circle: InfluenceCircle {
                center: Vec2::new(travel_distance, 0.0),
                radius: TETHER_RANGE,
            },
            root_distance: travel_distance,
        };
        let launch_phase = TETHER_HEARTBEAT_PRIMARY_PHASE;
        let arrival_phase = launch_phase + 0.25;

        let root_at_launch = visual_influence_circle(root, launch_phase);
        let remote_at_arrival =
            visual_influence_circle(remote, arrival_phase);
        let remote_at_launch = visual_influence_circle(remote, launch_phase);

        assert_eq!(root_at_launch.radius, remote_at_arrival.radius);
        assert_eq!(
            root_at_launch.deformation,
            remote_at_arrival.deformation,
        );
        assert!(root_at_launch.deformation >= 3.0);
        assert_ne!(remote_at_launch.radius, remote_at_arrival.radius);
    }

    #[test]
    fn fog_influence_uses_the_unpulsed_tether_radius() {
        let influence = NetworkInfluenceCircle {
            circle: InfluenceCircle {
                center: Vec2::new(64.0, 32.0),
                radius: TETHER_RANGE,
            },
            root_distance: 0.0,
        };
        let visible = visual_influence_circle(
            influence,
            TETHER_HEARTBEAT_PRIMARY_PHASE,
        );
        let fog = fog_influence_circle(influence);

        assert!(visible.radius > fog.radius);
        assert!(visible.deformation > 0.0);
        assert_eq!(fog.radius, TETHER_RANGE);
        assert_eq!(fog.deformation, 0.0);
    }

    #[test]
    fn pulse_deformation_profile_is_unique_to_each_circle() {
        let first = VisualInfluenceCircle {
            center: Vec2::new(20.0, -15.0),
            radius: TETHER_RANGE,
            deformation: 2.0,
        };
        let second = VisualInfluenceCircle {
            center: Vec2::new(210.0, 40.0),
            ..first
        };
        let first_idle = VisualInfluenceCircle {
            deformation: 0.0,
            ..first
        };
        let second_idle = VisualInfluenceCircle {
            deformation: 0.0,
            ..second
        };
        let angle = 0.7;
        let first_change = tether_range_artwork_deformed_radius(first, angle)
            - tether_range_artwork_deformed_radius(first_idle, angle);
        let second_change =
            tether_range_artwork_deformed_radius(second, angle)
                - tether_range_artwork_deformed_radius(second_idle, angle);

        assert!((first_change - second_change).abs() > 0.1);
    }

    #[test]
    fn tether_artwork_uses_black_alpha_profile_and_opaque_links() {
        let color = TETHER_LINK_COLOR.to_linear();
        assert_eq!(Vec3::new(color.red, color.green, color.blue), Vec3::ZERO);
        assert_eq!(TETHER_LINK_COLOR.alpha(), 1.0);
        assert_eq!(TETHER_RANGE_OUTLINE_ALPHA, 0.5);
        assert_eq!(TETHER_RANGE_TENDRIL_ALPHA, 0.22);

        let link_material = tether_link_material();
        assert_eq!(link_material.color.alpha(), 1.0);
        assert_eq!(link_material.alpha_mode, AlphaMode2d::Blend);
    }

    #[test]
    fn vein_pulses_launch_on_both_heartbeat_beats() {
        let segments = [PowerLinkSegment {
            start: Vec2::ZERO,
            end: Vec2::new(100.0, 0.0),
            root_distance: 0.0,
        }];

        let primary = power_pulse_sections(
            &segments,
            TETHER_HEARTBEAT_PRIMARY_PHASE,
        );
        let secondary = power_pulse_sections(
            &segments,
            TETHER_HEARTBEAT_SECONDARY_PHASE,
        );

        assert!(primary.iter().any(|section| {
            section.segment_index == 0
                && section.start_distance == 0.0
                && section.width == TETHER_PRIMARY_PULSE_WIDTH
        }));
        assert!(secondary.iter().any(|section| {
            section.segment_index == 0
                && section.start_distance == 0.0
                && section.width == TETHER_SECONDARY_PULSE_WIDTH
        }));
    }

    #[test]
    fn pulse_mesh_tracks_each_fixed_tick_at_high_speed() {
        let first = tether_pulse_animation_frame(0.0);
        let adjacent = tether_pulse_animation_frame(1.0 / 96.0);
        let following = tether_pulse_animation_frame(2.0 / 96.0);

        assert_ne!(first, adjacent);
        assert_ne!(adjacent, following);
    }

    #[test]
    fn vein_pulse_width_replaces_base_width_without_overlap() {
        let sections = [PowerPulseSection {
            segment_index: 0,
            start_distance: 3.0,
            end_distance: 21.0,
            width: TETHER_PRIMARY_PULSE_WIDTH,
        }];

        assert_eq!(
            vein_spans(0, 100.0, &sections),
            vec![
                VeinSpan {
                    start_distance: 0.0,
                    end_distance: 3.0,
                    width: TETHER_LINK_WIDTH,
                },
                VeinSpan {
                    start_distance: 3.0,
                    end_distance: 21.0,
                    width: TETHER_PRIMARY_PULSE_WIDTH,
                },
                VeinSpan {
                    start_distance: 21.0,
                    end_distance: 100.0,
                    width: TETHER_LINK_WIDTH,
                },
            ],
        );
    }

    #[test]
    fn tether_range_artwork_batches_one_quad_per_circle() {
        use bevy::mesh::VertexAttributeValues;

        let circles = [
            VisualInfluenceCircle {
                center: Vec2::new(20.0, -15.0),
                radius: TETHER_RANGE,
                deformation: 0.0,
            },
            VisualInfluenceCircle {
                center: Vec2::new(120.0, 25.0),
                radius: TETHER_RANGE,
                deformation: 2.0,
            },
        ];
        let mesh = tether_range_artwork_mesh(&circles);

        assert_eq!(mesh.count_vertices(), 8);
        assert_eq!(
            mesh.indices().map(bevy::mesh::Indices::len),
            Some(12),
        );
        let Some(VertexAttributeValues::Float32x4(indices)) =
            mesh.attribute(Mesh::ATTRIBUTE_COLOR)
        else {
            panic!("artwork mesh must encode circle indices in vertex colors");
        };
        assert!(indices[..4].iter().all(|index| index[0] == 0.0));
        assert!(indices[4..].iter().all(|index| index[0] == 1.0));
    }

    #[test]
    fn tether_range_artwork_buffer_matches_shader_layout() {
        let circles = [VisualInfluenceCircle {
            center: Vec2::new(20.0, -15.0),
            radius: 180.0,
            deformation: 2.5,
        }];

        assert_eq!(
            tether_range_artwork_buffer_data(&circles),
            vec![[20.0, -15.0, 180.0, 2.5]],
        );
    }

    #[test]
    #[ignore = "requires a GPU adapter; run explicitly without a window or audio"]
    fn tether_artwork_rebinds_when_a_pylon_expands_the_circle_buffer() {
        use crate::graphics::tests::{Headless, cleared_to};
        use bevy::asset::RenderAssetUsages;
        use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};

        let left = VisualInfluenceCircle { center: Vec2::new(-15.0, 0.0), radius: 6.0, deformation: 0.0 };
        let right = VisualInfluenceCircle { center: Vec2::new(15.0, 0.0), ..left };
        let mut headless = Headless::new(cleared_to(Color::WHITE), |app| {
            app.add_plugins(Material2dPlugin::<TetherRangeArtworkMaterial>::default());
            let world = app.world_mut();
            let artwork = world.resource_mut::<Assets<Image>>().add(Image::new_fill(
                Extent3d { width: 1, height: 1, depth_or_array_layers: 1 },
                TextureDimension::D2, &[255; 4], TextureFormat::Rgba8UnormSrgb, RenderAssetUsages::all(),
            ));
            let buffer = world.resource_mut::<Assets<ShaderBuffer>>()
                .add(tether_range_artwork_buffer(&tether_range_artwork_buffer_data(&[left])));
            let mut material = TetherRangeArtworkMaterial::new(artwork, buffer);
            material.settings.y = 1.0;
            material.settings.z = 1.0;
            let material = world.resource_mut::<Assets<TetherRangeArtworkMaterial>>().add(material);
            let mesh = world.resource_mut::<Assets<Mesh>>().add(tether_range_artwork_mesh(&[left]));
            world.spawn((TetherRangeArtwork, Mesh2d(mesh), MeshMaterial2d(material),
                Transform::from_translation(Vec3::Z * TETHER_RANGE_ART_Z), RenderLayers::layer(0)));
        });
        headless.run(100);
        assert_eq!(headless.pixel(17, 32), [0, 0, 0, 255]);
        assert_eq!(headless.pixel(47, 32), [255; 4]);

        let world = headless.app.world_mut();
        let (entity, material) = world.query_filtered::<(Entity, &MeshMaterial2d<TetherRangeArtworkMaterial>), With<TetherRangeArtwork>>()
            .single(world).map(|(entity, material)| (entity, material.clone())).unwrap();
        let mesh = world.resource_mut::<Assets<Mesh>>().add(tether_range_artwork_mesh(&[left, right]));
        world.entity_mut(entity).insert(Mesh2d(mesh));
        world.resource_scope(|world, mut materials: Mut<Assets<TetherRangeArtworkMaterial>>| {
            update_tether_range_artwork_buffer(&material, &mut materials, &mut world.resource_mut::<Assets<ShaderBuffer>>(),
                &tether_range_artwork_buffer_data(&[left, right]));
        });
        headless.run(60);
        assert_eq!(headless.pixel(17, 32), [0, 0, 0, 255]);
        assert_eq!(headless.pixel(47, 32), [0, 0, 0, 255], "the newly deployed pylon publishes its own artwork");
        assert_eq!(headless.pixel(25, 32), [255; 4]);
        headless.app.world_mut().resource_scope(|world, mut materials: Mut<Assets<TetherRangeArtworkMaterial>>| {
            update_tether_range_artwork_buffer(&material, &mut materials, &mut world.resource_mut::<Assets<ShaderBuffer>>(),
                &tether_range_artwork_buffer_data(&[VisualInfluenceCircle { radius: 10.0, ..left }, right]));
        });
        headless.run(30);
        assert_eq!(headless.pixel(25, 32), [0, 0, 0, 255], "growth updates the buffer already bound by the material");
    }

    #[test]
    fn overlapping_tether_regions_clip_only_intersecting_artwork_pixels() {
        let circles = [
            VisualInfluenceCircle {
                center: Vec2::ZERO,
                radius: TETHER_RANGE,
                deformation: 0.0,
            },
            VisualInfluenceCircle {
                center: Vec2::new(100.0, 0.0),
                radius: TETHER_RANGE,
                deformation: 0.0,
            },
        ];

        assert!(tether_range_artwork_fragment_hidden(
            0,
            Vec2::new(180.0, 0.0),
            false,
            &circles,
        ));
        assert!(!tether_range_artwork_fragment_hidden(
            0,
            Vec2::new(-190.0, 0.0),
            false,
            &circles,
        ));
    }

    #[test]
    fn outline_clip_inset_preserves_the_intersection_seam() {
        let circles = [
            VisualInfluenceCircle {
                center: Vec2::ZERO,
                radius: TETHER_RANGE,
                deformation: 0.0,
            },
            VisualInfluenceCircle {
                center: Vec2::new(382.0, 0.0),
                radius: TETHER_RANGE,
                deformation: 0.0,
            },
        ];

        assert!(!tether_range_artwork_fragment_hidden(
            0,
            Vec2::new(TETHER_RANGE, 0.0),
            false,
            &circles,
        ));
    }

    #[test]
    fn tendril_is_hidden_when_its_outline_root_is_cropped() {
        let circles = [
            VisualInfluenceCircle {
                center: Vec2::ZERO,
                radius: TETHER_RANGE,
                deformation: 0.0,
            },
            VisualInfluenceCircle {
                center: Vec2::new(300.0, 0.0),
                radius: TETHER_RANGE,
                deformation: 0.0,
            },
        ];
        let detached_tendril_pixel = Vec2::new(80.0, 0.0);

        assert!(!tether_range_artwork_fragment_hidden(
            0,
            detached_tendril_pixel,
            false,
            &circles,
        ));
        assert!(tether_range_artwork_fragment_hidden(
            0,
            detached_tendril_pixel,
            true,
            &circles,
        ));
    }
}
