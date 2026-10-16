use crate::graphics::quad_mesh::{QuadMesh, add_mesh, publish_mesh};
use std::collections::{HashMap, HashSet, VecDeque};

#[cfg(test)]
use bevy::asset::RenderAssetUsages;
use bevy::mesh::{Mesh, Mesh2d};
use bevy::render::render_resource::{TextureFormat};
use bevy::camera::visibility::{NoFrustumCulling, RenderLayers};
use bevy::sprite_render::{ColorMaterial, MeshMaterial2d};
use rand::Rng;
use crate::game::actors::catalog::ActorCatalog;
use crate::game::actors::{ActorDefinition, EnemyArchetype};
use theseeker_engine::assets::animation::SpriteAnimation;
use theseeker_engine::physics::{LinearVelocity, TileCollisionWorld};
use theseeker_engine::time::{GameTickUpdate, GameTime};

use super::{DamageInfo, Health};
use crate::game::enemy::Enemy;
use crate::game::physics::projectile::Projectile;
use crate::graphics::pixel_view::PixelAlignedWorldMesh;
use crate::prelude::*;

const MAX_BLOOD_ELEMENTS: usize = 2_048;
const REFERENCE_FPS: f32 = 60.0;
const STAIN_FADE_UPDATE_TICKS: u64 = 8;
const STAIN_ALPHA_STEPS: u8 = 8;
const BLOOD_STAIN_Z: f32 = -0.000_002;
const BLOOD_MOVING_Z: f32 = -0.000_001;
const DIRECTION_EPSILON_SQUARED: f32 = 0.0001;

// Fallback values are linear RGB because mesh vertex colors are consumed directly.
const FALLBACK_BLOOD_PALETTE: [[f32; 3]; 4] = [
    [0.045, 0.052, 0.064],
    [0.058, 0.066, 0.079],
    [0.072, 0.081, 0.096],
    [0.087, 0.096, 0.112],
];

// Linear RGB equivalent of the player's #a2324e health-bar fill.
const HEALTH_BAR_BLOOD_RED: [f32; 3] = [
    0.361_306_8,
    0.031_896_032,
    0.076_185_38,
];

pub struct BloodFxPlugin;

impl Plugin for BloodFxPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<BloodFx>()
            .init_resource::<BloodRenderMeshes>()
            .init_resource::<EnemyBloodProfiles>()
            .init_resource::<BloodSampleRegistry>()
            .init_resource::<BloodForegroundOcclusion>()
            .add_systems(
                OnEnter(AppState::InGame),
                (initialize_enemy_blood_profiles, setup_blood_renderers)
                    .chain(),
            )
            .add_systems(
                GameTickUpdate,
                (
                    emit_enemy_blood
                        .after(super::damage_source::apply_damage)
                        .before(super::damage_source::damage_source_cleanup),
                    simulate_blood.after(emit_enemy_blood),
                ),
            )
            .add_systems(
                Update,
                sync_blood_meshes.run_if(in_state(AppState::InGame)),
            )
;
    }
}

#[derive(Component, Clone, Copy, Debug)]
pub(crate) struct BloodSampleSource {
    animation_key: &'static str,
}

impl BloodSampleSource {
    pub(crate) const fn animation(animation_key: &'static str) -> Self {
        Self { animation_key }
    }
}

#[derive(Resource, Default)]
struct BloodSampleRegistry {
    animation_keys: HashSet<&'static str>,
}

pub(crate) fn register_blood_sample_animation(
    app: &mut App,
    animation_key: &'static str,
) {
    app.init_resource::<BloodSampleRegistry>();
    app.world_mut()
        .resource_mut::<BloodSampleRegistry>()
        .animation_keys
        .insert(animation_key);
}

#[derive(Resource, Default, Debug)]
pub(crate) struct BloodForegroundOcclusion {
    pixels: Vec<u64>,
}

impl BloodForegroundOcclusion {
    pub(crate) fn from_pixels(
        pixels: impl IntoIterator<Item = IVec2>,
    ) -> Self {
        let mut pixels: Vec<_> = pixels.into_iter().map(pixel_key).collect();
        pixels.sort_unstable();
        pixels.dedup();
        Self { pixels }
    }

    fn contains(&self, pixel: IVec2) -> bool {
        self.pixels.binary_search(&pixel_key(pixel)).is_ok()
    }
}

fn pixel_key(pixel: IVec2) -> u64 {
    (u64::from(pixel.x as u32) << 32) | u64::from(pixel.y as u32)
}

#[derive(Clone, Copy, Debug)]
struct WeightedBloodColor {
    rgb: [f32; 3],
    cumulative_weight: u32,
}

#[derive(Clone, Debug)]
struct EnemyBloodProfile {
    colors: Vec<WeightedBloodColor>,
    opaque_pixel_count: u32,
}

impl EnemyBloodProfile {
    fn from_histogram(mut histogram: Vec<([u8; 3], u32)>, srgb: bool) -> Self {
        histogram.sort_unstable_by_key(|(rgb, _)| *rgb);
        let mut cumulative_weight = 0u32;
        let colors = histogram
            .into_iter()
            .filter(|(_, count)| *count != 0)
            .map(|(rgb, count)| {
                cumulative_weight = cumulative_weight.saturating_add(count);
                WeightedBloodColor {
                    rgb: decode_rgb(rgb, srgb),
                    cumulative_weight,
                }
            })
            .collect();
        Self {
            colors,
            opaque_pixel_count: cumulative_weight,
        }
    }

    fn fallback() -> Self {
        let weight_per_color = 25;
        Self {
            colors: FALLBACK_BLOOD_PALETTE
                .iter()
                .enumerate()
                .map(|(index, &rgb)| WeightedBloodColor {
                    rgb,
                    cumulative_weight: (index as u32 + 1) * weight_per_color,
                })
                .collect(),
            opaque_pixel_count: 100,
        }
    }

    fn element_count(&self, lethal: bool) -> usize {
        let divisor = if lethal { 3 } else { 5 };
        ((self.opaque_pixel_count + divisor / 2) / divisor) as usize
    }

    fn random_color(
        &self,
        rng: &mut impl Rng,
        alpha_range: std::ops::RangeInclusive<f32>,
    ) -> BloodColor {
        let rgb = if rng.random_bool(0.5) {
            HEALTH_BAR_BLOOD_RED
        } else {
            self.random_sampled_rgb(rng)
        };
        BloodColor {
            rgb,
            alpha: rng.random_range(alpha_range),
        }
    }

    fn random_sampled_rgb(&self, rng: &mut impl Rng) -> [f32; 3] {
        let pick = rng.random_range(0..self.opaque_pixel_count);
        let index = self
            .colors
            .partition_point(|color| color.cumulative_weight <= pick);
        self.colors[index].rgb
    }
}

#[derive(Resource)]
struct EnemyBloodProfiles {
    initialized: bool,
    profiles: Vec<EnemyBloodProfile>,
    by_archetype: HashMap<String, usize>,
    by_animation_key: HashMap<&'static str, usize>,
}

impl Default for EnemyBloodProfiles {
    fn default() -> Self {
        Self {
            initialized: false,
            profiles: vec![EnemyBloodProfile::fallback()],
            by_archetype: HashMap::new(),
            by_animation_key: HashMap::new(),
        }
    }
}

impl EnemyBloodProfiles {
    fn profile_for(
        &self,
        actor: Option<&ActorDefinition>,
        source: Option<&BloodSampleSource>,
    ) -> &EnemyBloodProfile {
        let profile_index = actor
            .and_then(|actor| self.by_archetype.get(&actor.archetype.id).copied())
            .or_else(|| self.special_profile_index(source))
            .unwrap_or_default();
        &self.profiles[profile_index]
    }

    fn special_profile_index(
        &self,
        source: Option<&BloodSampleSource>,
    ) -> Option<usize> {
        self.by_animation_key
            .get(source?.animation_key)
            .copied()
    }
}

fn initialize_enemy_blood_profiles(
    catalog: Res<ActorCatalog>,
    animations: Res<Assets<SpriteAnimation>>,
    images: Res<Assets<Image>>,
    layouts: Res<Assets<TextureAtlasLayout>>,
    preloaded: Res<PreloadedAssets>,
    registry: Res<BloodSampleRegistry>,
    mut profiles: ResMut<EnemyBloodProfiles>,
) {
    if profiles.initialized {
        return;
    }

    for archetype in catalog.actors.values().map(|actor| &actor.archetype) {
        let Some(animation_key) = preferred_sample_animation(archetype) else {
            warn!("blood profile: archetype '{}' has no animation", archetype.id);
            continue;
        };
        let Some(profile) = sample_animation_profile(
            animation_key,
            &animations,
            &images,
            &layouts,
            &preloaded,
        ) else {
            warn!(
                "blood profile: could not sample '{}' for archetype '{}'",
                animation_key, archetype.id
            );
            continue;
        };
        let profile_index = profiles.profiles.len();
        profiles.profiles.push(profile);
        profiles.by_archetype.insert(archetype.id.clone(), profile_index);
    }

    let mut special_keys: Vec<_> =
        registry.animation_keys.iter().copied().collect();
    special_keys.sort_unstable();
    for animation_key in special_keys {
        let Some(profile) = sample_animation_profile(
            animation_key,
            &animations,
            &images,
            &layouts,
            &preloaded,
        ) else {
            warn!("blood profile: could not sample special source '{animation_key}'");
            continue;
        };
        let profile_index = profiles.profiles.len();
        profiles.profiles.push(profile);
        profiles
            .by_animation_key
            .insert(animation_key, profile_index);
    }

    profiles.initialized = true;
    info!(
        "blood profiles: cached {} archetypes and {} special sources",
        profiles.by_archetype.len(),
        profiles.by_animation_key.len()
    );
}

fn preferred_sample_animation(archetype: &EnemyArchetype) -> Option<&str> {
    ["Idle", "Fly", "Body"]
        .into_iter()
        .find_map(|name| archetype.anim.get(name).map(String::as_str))
        .or_else(|| {
            archetype
                .anim
                .iter()
                .min_by(|left, right| left.0.cmp(right.0))
                .map(|(_, key)| key.as_str())
        })
}

fn sample_animation_profile(
    animation_key: &str,
    animations: &Assets<SpriteAnimation>,
    images: &Assets<Image>,
    layouts: &Assets<TextureAtlasLayout>,
    preloaded: &PreloadedAssets,
) -> Option<EnemyBloodProfile> {
    let animation_handle =
        preloaded.get_single_asset::<SpriteAnimation>(animation_key)?;
    let animation = animations.get(&animation_handle)?;
    let (image_handle, layout_handle) =
        animation.resolve_image_atlas(preloaded, Some(animation_key))?;
    let image = images.get(&image_handle)?;
    let layout = layouts.get(&layout_handle)?;
    let frame = layout
        .textures
        .get(animation.settings.extended.frame_start.as_sprite_index())?;
    sample_image_region(image, *frame)
}

fn sample_image_region(image: &Image, frame: URect) -> Option<EnemyBloodProfile> {
    let (srgb, bgra) = match image.texture_descriptor.format {
        TextureFormat::Rgba8Unorm => (false, false),
        TextureFormat::Rgba8UnormSrgb => (true, false),
        TextureFormat::Bgra8Unorm => (false, true),
        TextureFormat::Bgra8UnormSrgb => (true, true),
        format => {
            warn!("blood profile: unsupported image format {format:?}");
            return None;
        }
    };
    let mut histogram: HashMap<[u8; 3], u32> = HashMap::new();
    for y in frame.min.y..frame.max.y {
        sample_image_row(image, frame.min.x..frame.max.x, y, bgra, &mut histogram)?;
    }
    (!histogram.is_empty()).then(|| {
        EnemyBloodProfile::from_histogram(histogram.into_iter().collect(), srgb)
    })
}

fn sample_image_row(
    image: &Image,
    x_range: std::ops::Range<u32>,
    y: u32,
    bgra: bool,
    histogram: &mut HashMap<[u8; 3], u32>,
) -> Option<()> {
    for x in x_range {
        let pixel = image.pixel_bytes(UVec3::new(x, y, 0)).ok()?;
        let Some(rgb) = opaque_pixel_rgb(pixel, bgra) else {
            continue;
        };
        *histogram.entry(rgb).or_default() += 1;
    }
    Some(())
}

fn opaque_pixel_rgb(pixel: &[u8], bgra: bool) -> Option<[u8; 3]> {
    if pixel[3] == 0 {
        return None;
    }
    Some(if bgra {
        [pixel[2], pixel[1], pixel[0]]
    } else {
        [pixel[0], pixel[1], pixel[2]]
    })
}

fn decode_rgb(rgb: [u8; 3], srgb: bool) -> [f32; 3] {
    rgb.map(|channel| {
        let value = f32::from(channel) / 255.0;
        if !srgb {
            value
        } else if value <= 0.040_45 {
            value / 12.92
        } else {
            ((value + 0.055) / 1.055).powf(2.4)
        }
    })
}

#[derive(Clone, Copy, Debug)]
struct BloodColor {
    rgb: [f32; 3],
    alpha: f32,
}

impl BloodColor {
    fn vertex_color(self, alpha_multiplier: f32) -> [f32; 4] {
        [
            self.rgb[0],
            self.rgb[1],
            self.rgb[2],
            self.alpha * alpha_multiplier,
        ]
    }
}

#[derive(Clone, Copy, Debug)]
struct BloodDroplet {
    position: Vec2,
    velocity: Vec2,
    gravity: f32,
    frame_drag: f32,
    remaining_seconds: f32,
    fade_seconds: f32,
    color: BloodColor,
}

#[derive(Clone, Copy, Debug)]
struct BloodStreak {
    head: Vec2,
    direction: Vec2,
    speed: f32,
    length: f32,
    frame_drag: f32,
    remaining_seconds: f32,
    total_seconds: f32,
    color: BloodColor,
}

#[derive(Clone, Copy, Debug)]
struct BloodStain {
    min: IVec2,
    size: IVec2,
    remaining_seconds: f32,
    fade_seconds: f32,
    alpha_step: u8,
    color: BloodColor,
}

impl BloodStain {
    fn alpha_multiplier(self) -> f32 {
        f32::from(self.alpha_step) / f32::from(STAIN_ALPHA_STEPS)
    }

    fn desired_alpha_step(self) -> u8 {
        if self.remaining_seconds >= self.fade_seconds {
            return STAIN_ALPHA_STEPS;
        }
        let fraction =
            (self.remaining_seconds / self.fade_seconds).clamp(0.0, 1.0);
        (fraction * f32::from(STAIN_ALPHA_STEPS)).ceil() as u8
    }
}

#[derive(Resource, Debug)]
struct BloodFx {
    droplets: Vec<BloodDroplet>,
    streaks: Vec<BloodStreak>,
    stains: VecDeque<BloodStain>,
    pending_stains: Vec<BloodStain>,
    lethal_bursts: HashSet<Entity>,
    moving_mesh_dirty: bool,
    stain_mesh_dirty: bool,
}

impl Default for BloodFx {
    fn default() -> Self {
        Self {
            droplets: Vec::with_capacity(256),
            streaks: Vec::with_capacity(64),
            stains: VecDeque::with_capacity(MAX_BLOOD_ELEMENTS),
            pending_stains: Vec::with_capacity(128),
            lethal_bursts: HashSet::new(),
            moving_mesh_dirty: true,
            stain_mesh_dirty: true,
        }
    }
}

impl BloodFx {
    fn clear(&mut self) {
        self.droplets.clear();
        self.streaks.clear();
        self.stains.clear();
        self.pending_stains.clear();
        self.lethal_bursts.clear();
        self.moving_mesh_dirty = true;
        self.stain_mesh_dirty = true;
    }

    fn element_count(&self) -> usize {
        self.droplets.len() + self.streaks.len() + self.stains.len()
    }

    fn make_room(&mut self) {
        while self.element_count() >= MAX_BLOOD_ELEMENTS && self.evict_one() {}
    }

    fn evict_one(&mut self) -> bool {
        if self.stains.pop_front().is_some() {
            self.stain_mesh_dirty = true;
            return true;
        }
        if !self.droplets.is_empty() {
            self.droplets.swap_remove(0);
            self.moving_mesh_dirty = true;
            return true;
        }
        if !self.streaks.is_empty() {
            self.streaks.swap_remove(0);
            self.moving_mesh_dirty = true;
            return true;
        }
        false
    }

    fn push_droplet(&mut self, droplet: BloodDroplet) {
        self.make_room();
        self.droplets.push(droplet);
        self.moving_mesh_dirty = true;
    }

    fn push_streak(&mut self, streak: BloodStreak) {
        self.make_room();
        self.streaks.push(streak);
        self.moving_mesh_dirty = true;
    }

    fn push_stain(&mut self, stain: BloodStain) {
        self.make_room();
        self.stains.push_back(stain);
        self.stain_mesh_dirty = true;
    }
}

#[derive(Resource, Default)]
struct BloodRenderMeshes {
    moving: Option<Entity>,
    stains: Option<Entity>,
}

#[derive(Component)]
struct MovingBloodMesh;

#[derive(Component)]
struct BloodStainMesh;

fn setup_blood_renderers(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
    mut fx: ResMut<BloodFx>,
    mut render_meshes: ResMut<BloodRenderMeshes>,
) {
    fx.clear();

    let moving = add_mesh(&mut meshes, empty_blood_mesh());
    let stains = add_mesh(&mut meshes, empty_blood_mesh());
    let material = materials.add(ColorMaterial::default());

    let moving_entity = commands.spawn((
        MovingBloodMesh,
        PixelAlignedWorldMesh,
        NoFrustumCulling,
        Mesh2d(moving.clone()),
        MeshMaterial2d(material.clone()),
        Transform::from_translation(Vec3::Z * BLOOD_MOVING_Z),
        GlobalTransform::default(),
        Visibility::Visible,
        InheritedVisibility::VISIBLE,
        ViewVisibility::default(),
        RenderLayers::layer(0),
        StateDespawnMarker,
        Name::new("MovingBloodFx"),
    )).id();
    let stains_entity = commands.spawn((
        BloodStainMesh,
        PixelAlignedWorldMesh,
        NoFrustumCulling,
        Mesh2d(stains.clone()),
        MeshMaterial2d(material),
        Transform::from_translation(Vec3::Z * BLOOD_STAIN_Z),
        GlobalTransform::default(),
        Visibility::Visible,
        InheritedVisibility::VISIBLE,
        ViewVisibility::default(),
        RenderLayers::layer(0),
        StateDespawnMarker,
        Name::new("BloodStainFx"),
    )).id();

    render_meshes.moving = Some(moving_entity);
    render_meshes.stains = Some(stains_entity);
}



#[derive(Default)]
struct BloodHitSource {
    position: Option<Vec2>,
    projectile_velocity: Option<Vec2>,
    linear_velocity: Option<Vec2>,
}

fn read_blood_hit_source(
    source: Entity,
    source_query: &Query<(
        Option<&GlobalTransform>,
        Option<&Projectile>,
        Option<&LinearVelocity>,
    )>,
) -> BloodHitSource {
    source_query
        .get(source)
        .map(|(transform, projectile, velocity)| BloodHitSource {
            position: transform.map(|value| value.translation().truncate()),
            projectile_velocity: projectile.map(|value| value.vel.0),
            linear_velocity: velocity.map(|value| value.0),
        })
        .unwrap_or_default()
}

fn usable_direction(direction: Vec2) -> Option<Vec2> {
    (direction.is_finite()
        && direction.length_squared() > DIRECTION_EPSILON_SQUARED)
        .then(|| direction.normalize())
}

fn resolve_impact_direction(
    projectile_velocity: Option<Vec2>,
    linear_velocity: Option<Vec2>,
    source_position: Option<Vec2>,
    owner_position: Option<Vec2>,
    target_position: Vec2,
) -> Vec2 {
    projectile_velocity
        .and_then(usable_direction)
        .or_else(|| linear_velocity.and_then(usable_direction))
        .or_else(|| {
            source_position
                .and_then(|source| usable_direction(target_position - source))
        })
        .or_else(|| {
            owner_position
                .and_then(|owner| usable_direction(target_position - owner))
        })
        .unwrap_or(Vec2::X)
}

fn emit_enemy_blood(
    mut damage_events: MessageReader<DamageInfo>,
    targets: Query<
        (
            &GlobalTransform,
            &Health,
            Option<&ActorDefinition>,
            Option<&BloodSampleSource>,
        ),
        With<Enemy>,
    >,
    transforms: Query<&GlobalTransform>,
    sources: Query<(
        Option<&GlobalTransform>,
        Option<&Projectile>,
        Option<&LinearVelocity>,
    )>,
    profiles: Res<EnemyBloodProfiles>,
    mut fx: ResMut<BloodFx>,
) {
    fx.lethal_bursts.retain(|entity| targets.contains(*entity));
    let mut rng = rand::rng();

    for damage in damage_events.read() {
        if damage.amount <= 0.0 {
            continue;
        }
        let Ok((target_transform, health, archetype, sample_source)) =
            targets.get(damage.target)
        else {
            continue;
        };

        let lethal = health.current == 0;
        if lethal && !fx.lethal_bursts.insert(damage.target) {
            continue;
        }

        let target_position = target_transform.translation().truncate();
        let source = read_blood_hit_source(damage.source, &sources);
        let owner_position = transforms
            .get(damage.owner)
            .ok()
            .map(|transform| transform.translation().truncate());
        let direction = resolve_impact_direction(
            source.projectile_velocity,
            source.linear_velocity,
            source.position,
            owner_position,
            target_position,
        );
        let profile = profiles.profile_for(archetype, sample_source);
        emit_blood_spray(
            &mut fx,
            &mut rng,
            target_position,
            direction,
            lethal,
            profile,
        );
    }
}

fn emit_blood_spray(
    fx: &mut BloodFx,
    rng: &mut impl Rng,
    origin: Vec2,
    impact_direction: Vec2,
    lethal: bool,
    profile: &EnemyBloodProfile,
) {
    let element_count = profile.element_count(lethal);
    if element_count == 0 {
        return;
    }
    let angle_spread = if lethal { 0.42 } else { 0.24 };
    let max_streaks = if lethal { 6 } else { 4 };
    let streak_count = element_count.div_ceil(24).clamp(1, max_streaks);

    emit_back_streak(
        fx,
        rng,
        origin,
        impact_direction,
        angle_spread,
        profile,
    );
    emit_front_streaks(
        fx,
        rng,
        origin,
        -impact_direction,
        streak_count - 1,
        angle_spread,
        profile,
    );
    emit_falling_droplets(
        fx,
        rng,
        origin,
        impact_direction,
        element_count - streak_count,
        angle_spread,
        profile,
    );
}

fn emit_back_streak(
    fx: &mut BloodFx,
    rng: &mut impl Rng,
    origin: Vec2,
    direction: Vec2,
    spread: f32,
    profile: &EnemyBloodProfile,
) {
    let direction = rotate(direction, rng.random_range(-spread..=spread));
    let lifetime = rng.random_range(0.07..=0.11);
    fx.push_streak(BloodStreak {
        head: jittered_origin(origin, rng),
        direction,
        speed: rng.random_range(68.0..=96.0),
        length: rng.random_range(6.0..=9.0),
        frame_drag: rng.random_range(0.91..=0.92),
        remaining_seconds: lifetime,
        total_seconds: lifetime,
        color: profile.random_color(rng, 0.9..=1.0),
    });
}

fn emit_front_streaks(
    fx: &mut BloodFx,
    rng: &mut impl Rng,
    origin: Vec2,
    direction: Vec2,
    count: usize,
    spread: f32,
    profile: &EnemyBloodProfile,
) {
    for _ in 0..count {
        let direction = rotate(direction, rng.random_range(-spread..=spread));
        let lifetime = rng.random_range(0.06..=0.12);
        fx.push_streak(BloodStreak {
            head: jittered_origin(origin, rng),
            direction,
            speed: rng.random_range(28.0..=54.0),
            length: rng.random_range(3.0..=5.0),
            frame_drag: rng.random_range(0.91..=0.92),
            remaining_seconds: lifetime,
            total_seconds: lifetime,
            color: profile.random_color(rng, 0.45..=0.72),
        });
    }
}

fn emit_falling_droplets(
    fx: &mut BloodFx,
    rng: &mut impl Rng,
    origin: Vec2,
    direction: Vec2,
    count: usize,
    spread: f32,
    profile: &EnemyBloodProfile,
) {
    for _ in 0..count {
        let spray_direction =
            rotate(direction, rng.random_range(-spread..=spread));
        let velocity = spray_direction * rng.random_range(16.0..=108.0)
            + Vec2::Y * rng.random_range(-11.0..=45.0);
        fx.push_droplet(random_droplet(
            rng,
            origin,
            velocity,
            270.0..=338.0,
            0.96..=0.97,
            profile,
        ));
    }
}

fn random_droplet(
    rng: &mut impl Rng,
    origin: Vec2,
    velocity: Vec2,
    gravity_range: std::ops::RangeInclusive<f32>,
    drag_range: std::ops::RangeInclusive<f32>,
    profile: &EnemyBloodProfile,
) -> BloodDroplet {
    BloodDroplet {
        position: jittered_origin(origin, rng),
        velocity,
        gravity: rng.random_range(gravity_range),
        frame_drag: rng.random_range(drag_range),
        remaining_seconds: rng.random_range(12.0..=15.0),
        fade_seconds: rng.random_range(5.0..=7.0),
        color: profile.random_color(rng, 0.82..=1.0),
    }
}

fn jittered_origin(origin: Vec2, rng: &mut impl Rng) -> Vec2 {
    origin
        + Vec2::new(
            rng.random_range(-1.25..=1.25),
            rng.random_range(-2.25..=2.25),
        )
}

fn rotate(vector: Vec2, angle: f32) -> Vec2 {
    let (sin, cos) = angle.sin_cos();
    Vec2::new(
        vector.x * cos - vector.y * sin,
        vector.x * sin + vector.y * cos,
    )
}

fn simulate_blood(
    game_time: Res<GameTime>,
    collision_world: Res<TileCollisionWorld>,
    mut fx: ResMut<BloodFx>,
) {
    let dt = (1.0 / game_time.hz) as f32;
    let update_stain_fade =
        game_time.tick().is_multiple_of(STAIN_FADE_UPDATE_TICKS);
    let mut rng = rand::rng();
    let had_moving_blood = !fx.droplets.is_empty() || !fx.streaks.is_empty();

    fx.pending_stains.clear();
    let mut droplet_index = 0;
    while droplet_index < fx.droplets.len() {
        let result = advance_droplet(
            &mut fx.droplets[droplet_index],
            dt,
            &collision_world,
            &mut rng,
        );
        match result {
            DropletStep::Moving => droplet_index += 1,
            DropletStep::Settled(stain) => {
                fx.droplets.swap_remove(droplet_index);
                fx.pending_stains.push(stain);
            }
            DropletStep::Expired => {
                fx.droplets.swap_remove(droplet_index);
            }
        }
    }

    let mut streak_index = 0;
    while streak_index < fx.streaks.len() {
        if advance_streak(&mut fx.streaks[streak_index], dt) {
            streak_index += 1;
        } else {
            fx.streaks.swap_remove(streak_index);
        }
    }
    fx.moving_mesh_dirty |= had_moving_blood
        || !fx.droplets.is_empty()
        || !fx.streaks.is_empty();

    let pending = std::mem::take(&mut fx.pending_stains);
    for stain in pending.iter().copied() {
        fx.push_stain(stain);
    }
    fx.pending_stains = pending;
    fx.pending_stains.clear();

    let old_stain_count = fx.stains.len();
    let mut stain_alpha_changed = false;
    for stain in &mut fx.stains {
        stain.remaining_seconds -= dt;
        if update_stain_fade {
            let desired_step = stain.desired_alpha_step();
            stain_alpha_changed |= desired_step != stain.alpha_step;
            stain.alpha_step = desired_step;
        }
    }
    fx.stains.retain(|stain| stain.remaining_seconds > 0.0);
    fx.stain_mesh_dirty |=
        stain_alpha_changed || fx.stains.len() != old_stain_count;
}

enum DropletStep {
    Moving,
    Settled(BloodStain),
    Expired,
}

fn advance_droplet(
    droplet: &mut BloodDroplet,
    dt: f32,
    collision_world: &TileCollisionWorld,
    rng: &mut impl Rng,
) -> DropletStep {
    droplet.remaining_seconds -= dt;
    if droplet.remaining_seconds <= 0.0 {
        return DropletStep::Expired;
    }

    let drag = droplet.frame_drag.powf(dt * REFERENCE_FPS);
    droplet.velocity *= drag;
    droplet.velocity.y -= droplet.gravity * dt;
    let next_position = droplet.position + droplet.velocity * dt;

    let Some(hit) = collision_world
        .segment_cast_solid(droplet.position, next_position)
    else {
        droplet.position = next_position;
        return DropletStep::Moving;
    };

    let movement = next_position - droplet.position;
    let direction = movement.normalize_or_zero();
    let ray_contact = droplet.position + direction * hit.time_of_impact;
    let (contact, normal) = if hit.normal == Vec2::ZERO {
        embedded_surface(collision_world, ray_contact, -direction)
            .unwrap_or((ray_contact, dominant_opposite_normal(direction)))
    } else {
        (ray_contact, hit.normal)
    };
    DropletStep::Settled(stain_from_impact(
        contact,
        normal,
        droplet,
        rng,
    ))
}

fn embedded_surface(
    collision_world: &TileCollisionWorld,
    position: Vec2,
    preferred: Vec2,
) -> Option<(Vec2, Vec2)> {
    let directions = [Vec2::Y, Vec2::NEG_Y, Vec2::X, Vec2::NEG_X];
    let mut best: Option<(f32, f32, Vec2)> = None;
    let max_steps = collision_world.tile_size.ceil() as u32 + 1;

    for direction in directions {
        let Some(distance) = first_free_distance(
            collision_world,
            position,
            direction,
            max_steps,
        ) else {
            continue;
        };
        let preference = direction.dot(preferred);
        let candidate = (distance, preference, direction);
        if best.is_none_or(|current| {
            better_embedded_surface(distance, preference, current)
        }) {
            best = Some(candidate);
        }
    }
    best.map(|(distance, _, normal)| {
        let surface_distance = refine_surface_distance(
            collision_world,
            position,
            normal,
            distance,
        );
        (position + normal * surface_distance, normal)
    })
}

fn first_free_distance(
    collision_world: &TileCollisionWorld,
    position: Vec2,
    direction: Vec2,
    max_steps: u32,
) -> Option<f32> {
    (1..=max_steps).find_map(|step| {
        let distance = step as f32;
        (!collision_world
            .is_solid_at_world_point(position + direction * distance))
        .then_some(distance)
    })
}

fn better_embedded_surface(
    distance: f32,
    preference: f32,
    current: (f32, f32, Vec2),
) -> bool {
    distance < current.0
        || (distance == current.0 && preference > current.1)
}

fn refine_surface_distance(
    collision_world: &TileCollisionWorld,
    position: Vec2,
    direction: Vec2,
    first_free_distance: f32,
) -> f32 {
    let mut solid_distance = (first_free_distance - 1.0).max(0.0);
    let mut free_distance = first_free_distance;
    for _ in 0..6 {
        let midpoint = (solid_distance + free_distance) * 0.5;
        if collision_world
            .is_solid_at_world_point(position + direction * midpoint)
        {
            solid_distance = midpoint;
        } else {
            free_distance = midpoint;
        }
    }
    free_distance
}

fn dominant_opposite_normal(direction: Vec2) -> Vec2 {
    if direction.x.abs() > direction.y.abs() {
        Vec2::new(-direction.x.signum(), 0.0)
    } else {
        Vec2::new(0.0, -direction.y.signum())
    }
}

fn stain_from_impact(
    contact: Vec2,
    normal: Vec2,
    droplet: &BloodDroplet,
    rng: &mut impl Rng,
) -> BloodStain {
    let surface = contact.round().as_ivec2();
    let length = rng.random_range(2..=4);
    let (min, size) = if normal.x.abs() > normal.y.abs() {
        let x = if normal.x >= 0.0 {
            surface.x
        } else {
            surface.x - 1
        };
        (
            IVec2::new(x, surface.y - length / 2),
            IVec2::new(1, length),
        )
    } else {
        let y = if normal.y >= 0.0 {
            surface.y
        } else {
            surface.y - 1
        };
        (
            IVec2::new(surface.x - length / 2, y),
            IVec2::new(length, 1),
        )
    };

    BloodStain {
        min,
        size,
        remaining_seconds: droplet.remaining_seconds,
        fade_seconds: droplet.fade_seconds,
        alpha_step: STAIN_ALPHA_STEPS,
        color: droplet.color,
    }
}

fn advance_streak(streak: &mut BloodStreak, dt: f32) -> bool {
    streak.remaining_seconds -= dt;
    if streak.remaining_seconds <= 0.0 {
        return false;
    }
    let drag = streak.frame_drag.powf(dt * REFERENCE_FPS);
    streak.speed *= drag;
    streak.length = (streak.length * drag).max(1.0);
    streak.head += streak.direction * streak.speed * dt;
    true
}

fn sync_blood_meshes(
    mut fx: ResMut<BloodFx>,
    render_meshes: Res<BloodRenderMeshes>,
    occlusion: Res<BloodForegroundOcclusion>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut mesh_entities: Query<&mut Mesh2d>,
) {
    if fx.moving_mesh_dirty {
        if let Some(mut handle) = render_meshes.moving.and_then(|entity| mesh_entities.get_mut(entity).ok()) {
            publish_mesh(&mut meshes, &mut handle.0, moving_blood_mesh(&fx, &occlusion));
        }
        fx.moving_mesh_dirty = false;
    }

    if fx.stain_mesh_dirty {
        if let Some(mut handle) = render_meshes.stains.and_then(|entity| mesh_entities.get_mut(entity).ok()) {
            publish_mesh(&mut meshes, &mut handle.0, stain_blood_mesh(&fx.stains, &occlusion));
        }
        fx.stain_mesh_dirty = false;
    }
}

fn empty_blood_mesh() -> Mesh {
    QuadMesh::default().finish_colored()
}

fn moving_blood_mesh(
    fx: &BloodFx,
    occlusion: &BloodForegroundOcclusion,
) -> Mesh {
    let estimated_pixels = fx.droplets.len() + fx.streaks.len() * 8;
    let mut builder = QuadMesh::with_capacity(estimated_pixels);

    for droplet in &fx.droplets {
        let alpha = if droplet.remaining_seconds < droplet.fade_seconds {
            (droplet.remaining_seconds / droplet.fade_seconds)
                .clamp(0.0, 1.0)
        } else {
            1.0
        };
        let pixel = droplet.position.round().as_ivec2();
        if !occlusion.contains(pixel) {
            builder.push_pixel(pixel, droplet.color.vertex_color(alpha));
        }
    }

    for streak in &fx.streaks {
        let tail = streak.head - streak.direction * streak.length;
        let start = tail.round().as_ivec2();
        let end = streak.head.round().as_ivec2();
        let alpha = (streak.remaining_seconds / streak.total_seconds)
            .clamp(0.0, 1.0);
        append_pixel_line(
            start,
            end,
            streak.color.vertex_color(alpha),
            &mut builder,
            occlusion,
        );
    }
    builder.finish_colored()
}

fn stain_blood_mesh(
    stains: &VecDeque<BloodStain>,
    occlusion: &BloodForegroundOcclusion,
) -> Mesh {
    let estimated_pixels = stains
        .iter()
        .map(|stain| (stain.size.x * stain.size.y) as usize)
        .sum();
    let mut builder = QuadMesh::with_capacity(estimated_pixels);
    for stain in stains {
        append_stain_pixels(stain, occlusion, &mut builder);
    }
    builder.finish_colored()
}

fn append_stain_pixels(
    stain: &BloodStain,
    occlusion: &BloodForegroundOcclusion,
    builder: &mut QuadMesh,
) {
    let color = stain.color.vertex_color(stain.alpha_multiplier());
    for y in 0..stain.size.y {
        append_stain_row(stain, y, color, occlusion, builder);
    }
}

fn append_stain_row(
    stain: &BloodStain,
    y: i32,
    color: [f32; 4],
    occlusion: &BloodForegroundOcclusion,
    builder: &mut QuadMesh,
) {
    for x in 0..stain.size.x {
        let pixel = stain.min + IVec2::new(x, y);
        if !occlusion.contains(pixel) {
            builder.push_pixel(pixel, color);
        }
    }
}

fn append_pixel_line(
    start: IVec2,
    end: IVec2,
    color: [f32; 4],
    builder: &mut QuadMesh,
    occlusion: &BloodForegroundOcclusion,
) {
    let mut point = start;
    let delta = (end - start).abs();
    let step = IVec2::new(
        if start.x < end.x { 1 } else { -1 },
        if start.y < end.y { 1 } else { -1 },
    );
    let mut error = delta.x - delta.y;

    loop {
        if !occlusion.contains(point) {
            builder.push_pixel(point, color);
        }
        if point == end {
            break;
        }
        let doubled_error = error * 2;
        if doubled_error > -delta.y {
            error -= delta.y;
            point.x += step.x;
        }
        if doubled_error < delta.x {
            error += delta.x;
            point.y += step.y;
        }
    }
}


#[cfg(test)]
mod tests {
    use bevy::render::render_resource::{
        Extent3d, TextureDimension,
    };
    use bevy::mesh::VertexAttributeValues;
    use rand::{rngs::StdRng, SeedableRng};

    use super::*;

    fn test_color() -> BloodColor {
        BloodColor {
            rgb: FALLBACK_BLOOD_PALETTE[0],
            alpha: 1.0,
        }
    }

    fn test_profile(opaque_pixel_count: u32) -> EnemyBloodProfile {
        EnemyBloodProfile::from_histogram(
            vec![([10, 20, 30], opaque_pixel_count)],
            false,
        )
    }

    fn test_droplet(position: Vec2, velocity: Vec2) -> BloodDroplet {
        BloodDroplet {
            position,
            velocity,
            gravity: 0.0,
            frame_drag: 1.0,
            remaining_seconds: 12.0,
            fade_seconds: 5.0,
            color: test_color(),
        }
    }

    #[test]
    fn death_spray_is_larger_than_regular_hit_spray() {
        let mut normal = BloodFx::default();
        let mut death = BloodFx::default();
        let mut normal_rng = StdRng::seed_from_u64(4);
        let mut death_rng = StdRng::seed_from_u64(4);
        let profile = test_profile(100);

        emit_blood_spray(
            &mut normal,
            &mut normal_rng,
            Vec2::ZERO,
            Vec2::X,
            false,
            &profile,
        );
        emit_blood_spray(
            &mut death,
            &mut death_rng,
            Vec2::ZERO,
            Vec2::X,
            true,
            &profile,
        );

        assert_eq!(normal.element_count(), 20);
        assert_eq!(death.element_count(), 33);
    }

    #[test]
    fn emitted_droplets_all_use_fast_falling_gravity() {
        let mut fx = BloodFx::default();
        let mut rng = StdRng::seed_from_u64(12);
        let profile = test_profile(100);

        emit_blood_spray(
            &mut fx,
            &mut rng,
            Vec2::ZERO,
            Vec2::X,
            false,
            &profile,
        );

        assert!(!fx.droplets.is_empty());
        assert!(fx.droplets.iter().all(|droplet| droplet.gravity >= 270.0));
    }

    #[test]
    fn pool_never_exceeds_fixed_capacity() {
        let mut fx = BloodFx::default();
        for index in 0..(MAX_BLOOD_ELEMENTS + 100) {
            fx.push_droplet(test_droplet(Vec2::X * index as f32, Vec2::ZERO));
        }
        assert_eq!(fx.element_count(), MAX_BLOOD_ELEMENTS);
    }

    #[test]
    fn falling_droplet_becomes_horizontal_ground_stain() {
        let mut terrain = vec![false; 16];
        terrain[12..16].fill(true);
        let mut collision_world = TileCollisionWorld::default();
        collision_world.rebuild(4, 4, 6.0, &terrain);
        let mut droplet = test_droplet(Vec2::new(9.0, 7.0), Vec2::NEG_Y * 20.0);
        let mut rng = StdRng::seed_from_u64(7);

        let step = advance_droplet(
            &mut droplet,
            0.1,
            &collision_world,
            &mut rng,
        );
        let DropletStep::Settled(stain) = step else {
            panic!("droplet should settle on the ground");
        };

        assert_eq!(stain.min.y, 6);
        assert_eq!(stain.size.y, 1);
        assert!((2..=4).contains(&stain.size.x));
    }

    #[test]
    fn embedded_droplet_resolves_stain_to_nearest_surface() {
        let mut terrain = vec![false; 16];
        terrain[12..16].fill(true);
        let mut collision_world = TileCollisionWorld::default();
        collision_world.rebuild(4, 4, 6.0, &terrain);
        let mut droplet =
            test_droplet(Vec2::new(9.0, 5.3), Vec2::Y * 2.0);
        let mut rng = StdRng::seed_from_u64(9);

        let step = advance_droplet(
            &mut droplet,
            0.1,
            &collision_world,
            &mut rng,
        );
        let DropletStep::Settled(stain) = step else {
            panic!("embedded droplet should settle on the nearest surface");
        };

        assert_eq!(stain.min.y, 6);
        assert_eq!(stain.size.y, 1);
    }

    #[test]
    fn moving_mesh_vertices_are_on_integer_pixel_boundaries() {
        let mut fx = BloodFx::default();
        fx.droplets.push(test_droplet(
            Vec2::new(3.42, -7.61),
            Vec2::ZERO,
        ));
        fx.streaks.push(BloodStreak {
            head: Vec2::new(8.7, 3.3),
            direction: Vec2::new(0.8, 0.6),
            speed: 0.0,
            length: 5.0,
            frame_drag: 1.0,
            remaining_seconds: 1.0,
            total_seconds: 1.0,
            color: test_color(),
        });

        let mesh = moving_blood_mesh(
            &fx,
            &BloodForegroundOcclusion::default(),
        );
        let Some(VertexAttributeValues::Float32x3(positions)) =
            mesh.attribute(Mesh::ATTRIBUTE_POSITION)
        else {
            panic!("blood mesh should have Float32x3 positions");
        };
        assert!(positions.iter().all(|position| {
            position[0].fract() == 0.0 && position[1].fract() == 0.0
        }));
    }

    #[test]
    fn impact_direction_prefers_projectile_velocity() {
        let direction = resolve_impact_direction(
            Some(Vec2::Y * 20.0),
            Some(Vec2::X),
            Some(Vec2::ZERO),
            Some(Vec2::ZERO),
            Vec2::ONE,
        );
        assert_eq!(direction, Vec2::Y);
    }

    #[test]
    fn world_pass_alignment_keeps_blood_on_physical_pixel_boundaries() {
        let camera = Vec2::new(1.07, -2.11);
        let offset = crate::graphics::pixel_view::PixelViewMapping::new(camera.extend(0.0), 6, UVec2::ZERO).mesh_offset();
        let physical_screen_position =
            (Vec2::new(10.0, -3.0) + offset - camera) * 6.0;

        let rounding_error =
            (physical_screen_position - physical_screen_position.round()).abs();
        assert!(rounding_error.max_element() < 0.000_01);
    }

    #[test]
    fn alignment_system_camera_and_blood_queries_are_disjoint() {
        let mut app = App::new();
        app.init_resource::<crate::camera::GameplayPixelScale>().init_resource::<crate::graphics::pixel_view::PixelView>();
        app.world_mut().spawn((crate::camera::MainCamera, Transform::default()));
        app.world_mut()
            .spawn((MovingBloodMesh, PixelAlignedWorldMesh, Transform::default()));
        app.add_systems(Update, (crate::graphics::pixel_view::update_pixel_view, crate::graphics::pixel_view::align_world_meshes).chain());

        app.update();
    }

    #[test]
    fn profile_weights_and_amounts_follow_source_pixels() {
        let profile = EnemyBloodProfile::from_histogram(
            vec![([255, 255, 255], 1), ([0, 0, 0], 9)],
            false,
        );

        assert_eq!(profile.opaque_pixel_count, 10);
        assert_eq!(profile.colors[0].cumulative_weight, 9);
        assert_eq!(profile.colors[1].cumulative_weight, 10);
        assert_eq!(profile.element_count(false), 2);
        assert_eq!(profile.element_count(true), 3);
    }

    #[test]
    fn blood_colors_split_evenly_between_health_red_and_sampled_palette() {
        let profile = test_profile(100);
        let sampled_rgb = profile.colors[0].rgb;
        let mut rng = StdRng::seed_from_u64(91);
        let colors: Vec<_> = (0..1_000)
            .map(|_| profile.random_color(&mut rng, 1.0..=1.0).rgb)
            .collect();
        let red_count = colors
            .iter()
            .filter(|&&rgb| rgb == HEALTH_BAR_BLOOD_RED)
            .count();

        assert!(colors.iter().all(|&rgb| {
            rgb == HEALTH_BAR_BLOOD_RED || rgb == sampled_rgb
        }));
        assert!((450..=550).contains(&red_count));
    }

    #[test]
    fn profile_amounts_round_to_nearest_fifth_and_third() {
        let profile = test_profile(46);

        assert_eq!(profile.element_count(false), 9);
        assert_eq!(profile.element_count(true), 15);
    }

    #[test]
    fn image_sampling_ignores_transparency_and_counts_repeated_colors() {
        let image = Image::new(
            Extent3d {
                width: 2,
                height: 2,
                depth_or_array_layers: 1,
            },
            TextureDimension::D2,
            vec![
                0, 0, 0, 255, 0, 0, 0, 255, 255, 255, 255, 255, 40, 50,
                60, 0,
            ],
            TextureFormat::Rgba8UnormSrgb,
            RenderAssetUsages::MAIN_WORLD,
        );

        let profile = sample_image_region(
            &image,
            URect::from_corners(UVec2::ZERO, UVec2::splat(2)),
        )
        .expect("three opaque pixels should produce a profile");

        assert_eq!(profile.opaque_pixel_count, 3);
        assert_eq!(profile.colors.len(), 2);
        assert_eq!(profile.colors[0].cumulative_weight, 2);
        assert_eq!(profile.colors[1].cumulative_weight, 3);
    }

    #[test]
    fn foreground_occlusion_removes_only_covered_blood_pixels() {
        let mut fx = BloodFx::default();
        fx.droplets.push(test_droplet(Vec2::new(3.0, 4.0), Vec2::ZERO));
        fx.droplets.push(test_droplet(Vec2::new(5.0, 6.0), Vec2::ZERO));
        let occlusion = BloodForegroundOcclusion::from_pixels([
            IVec2::new(3, 4),
        ]);

        let mesh = moving_blood_mesh(&fx, &occlusion);
        let Some(VertexAttributeValues::Float32x3(positions)) =
            mesh.attribute(Mesh::ATTRIBUTE_POSITION)
        else {
            panic!("blood mesh should have Float32x3 positions");
        };
        assert_eq!(positions.len(), 4);
        assert!(positions.iter().all(|position| position[0] >= 5.0));
    }
}
