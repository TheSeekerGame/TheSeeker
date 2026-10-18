pub mod geometry;
mod tiles;
pub use tiles::*;

use bevy::prelude::*;
use bevy::transform::systems::{mark_dirty_trees, propagate_parent_transforms, sync_simple_transforms};
use bevy::transform::TransformSystems::Propagate;

use crate::prelude::{GameTickUpdate, GameTime, HashMap, HashSet, Instant};
use crate::profile::{profiling_enabled, TickSystemProfile};
use crate::script::ScriptSet;

/// Physics plugin that wires the custom entity-collision world.
pub struct PhysicsPlugin;

impl Plugin for PhysicsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SpriteShapeMap>();
        app.init_resource::<EntityCollisionWorld>();
        app.configure_sets(GameTickUpdate, PhysicsSet.after(Propagate));
        app.add_systems(
            GameTickUpdate,
            (mark_dirty_trees, propagate_parent_transforms, sync_simple_transforms).chain().in_set(Propagate),
        );
        app.add_systems(
            GameTickUpdate,
            (
                update_sprite_colliders.after(ScriptSet::Run).before(PhysicsSet),
                rebuild_entity_collision_world.in_set(PhysicsSet),
            ),
        );
    }
}

#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct PhysicsSet;

/// The player collision group
pub const PLAYER: Group = Group(1 << 0);
/// The enemy collision group
pub const ENEMY: Group = Group(1 << 1);
/// The player attack collision group
pub const PLAYER_ATTACK: Group = Group(1 << 2);
/// The enemy attack collision group
pub const ENEMY_ATTACK: Group = Group(1 << 3);
/// The ground collision group
pub const GROUND: Group = Group(1 << 4);
/// Use when the other groups don't make sense, and you just want to detect something
pub const SENSOR: Group = Group(1 << 5);
/// Applied to the player while it is inside an enemy
pub const ENEMY_INSIDE: Group = Group(1 << 6);

/// Distance we keep the shape-cast above ground.
pub const GROUNDED_THRESHOLD: f32 = 1.0;

/// Canonical collision masks. Use these instead of hand-rolling `CollisionGroups::new(...)` so the
/// two-way membership/filter test stays consistent.
pub mod groups {
    use super::*;

    /// The filter each high-level category uses to find the categories it interacts with.
    pub fn default_filter(memberships: Group) -> Group {
        match memberships {
            PLAYER => ENEMY | ENEMY_ATTACK | GROUND,
            ENEMY => PLAYER | PLAYER_ATTACK | GROUND,
            PLAYER_ATTACK => ENEMY | GROUND,
            ENEMY_ATTACK => PLAYER | ENEMY_INSIDE,
            ENEMY_INSIDE => PLAYER,
            _ => Group::all(),
        }
    }

    pub fn groups(memberships: Group) -> CollisionGroups {
        CollisionGroups::new(memberships, default_filter(memberships))
    }

    /// Player body: blocks enemy bodies and ground.
    pub fn player_body() -> CollisionGroups {
        groups(PLAYER)
    }

    /// Enemy body: blocks player bodies/attacks and ground.
    pub fn enemy_body() -> CollisionGroups {
        groups(ENEMY)
    }

    /// Player-owned hitboxes (sword, arrow, etc.).
    pub fn player_attack() -> CollisionGroups {
        groups(PLAYER_ATTACK)
    }

    /// Enemy-generated damage (melee hulls, projectiles, …).
    pub fn enemy_attack() -> CollisionGroups {
        groups(ENEMY_ATTACK)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Group(u32);

impl Group {
    pub const NONE: Group = Group(0);

    pub const fn all() -> Group {
        Group(u32::MAX)
    }

    pub const fn bits(self) -> u32 {
        self.0
    }

    pub const fn from_bits_truncate(bits: u32) -> Group {
        Group(bits)
    }

    pub const fn intersects(self, other: Group) -> bool {
        self.0 & other.0 != 0
    }
}

impl std::ops::BitOr for Group {
    type Output = Group;

    fn bitor(self, rhs: Group) -> Group {
        Group(self.0 | rhs.0)
    }
}

impl std::ops::BitAnd for Group {
    type Output = Group;

    fn bitand(self, rhs: Group) -> Group {
        Group(self.0 & rhs.0)
    }
}

impl std::ops::Not for Group {
    type Output = Group;

    fn not(self) -> Group {
        Group(!self.0)
    }
}

/// Two colliders interact when each one's memberships intersect the other's filters.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct CollisionGroups {
    memberships: Group,
    filters: Group,
}

impl CollisionGroups {
    pub const fn new(memberships: Group, filters: Group) -> Self {
        Self { memberships, filters }
    }

    pub const fn memberships(self) -> Group {
        self.memberships
    }

    pub const fn filters(self) -> Group {
        self.filters
    }

    fn interacts_with(self, other: CollisionGroups) -> bool {
        self.memberships.intersects(other.filters) && other.memberships.intersects(self.filters)
    }
}

impl Default for CollisionGroups {
    fn default() -> Self {
        Self::new(Group::all(), Group::all())
    }
}

/// The rectangle centered on `center`; negative extents are not normalized.
pub fn rect_around(center: Vec2, half_extents: Vec2) -> Rect {
    Rect { min: center - half_extents, max: center + half_extents }
}

/// Collision shape in its entity's local space.
#[derive(Component, Clone, Debug)]
pub enum Collider {
    Empty,
    Cuboid { half_extents: Vec2 },
    Circle { radius: f32 },
    Capsule { a: Vec2, b: Vec2, radius: f32 },
    MultiRect { rects: Vec<Rect> },
}

impl Collider {
    pub fn cuboid(x_half: f32, y_half: f32) -> Self {
        Self::Cuboid { half_extents: Vec2::new(x_half, y_half) }
    }

    pub fn ball(radius: f32) -> Self {
        Self::Circle { radius }
    }

    pub fn capsule(a: Vec2, b: Vec2, radius: f32) -> Self {
        Self::Capsule { a, b, radius }
    }

    pub fn multi_rect(rects: Vec<Rect>) -> Self {
        if rects.is_empty() {
            return Self::Empty;
        }
        Self::MultiRect { rects }
    }

    pub fn is_empty(&self) -> bool {
        matches!(self, Self::Empty)
    }

    pub fn cuboid_half_extents(&self) -> Option<Vec2> {
        match self {
            Self::Cuboid { half_extents } => Some(*half_extents),
            _ => None,
        }
    }

    pub fn local_aabb(&self) -> Option<Rect> {
        match self {
            Self::Empty => None,
            Self::Cuboid { half_extents } => Some(rect_around(Vec2::ZERO, *half_extents)),
            Self::Circle { radius } => Some(rect_around(Vec2::ZERO, Vec2::splat(*radius))),
            Self::Capsule { a, b, radius } => Some(Rect { min: a.min(*b) - *radius, max: a.max(*b) + *radius }),
            Self::MultiRect { rects } => rects.iter().copied().reduce(|a, b| a.union(b)),
        }
    }

    /// Half extents of the local bounding box, or zero for an empty collider.
    pub fn half_extents(&self) -> Vec2 {
        self.local_aabb().map_or(Vec2::ZERO, |aabb| aabb.half_size())
    }

    pub fn world_aabb(&self, position: Vec2) -> Option<Rect> {
        self.local_aabb().map(|aabb| Rect { min: aabb.min + position, max: aabb.max + position })
    }

    /// Whether this shape at `pos` overlaps `other` at `other_pos`, counting touching edges.
    fn overlaps(&self, pos: Vec2, other: &Collider, other_pos: Vec2) -> bool {
        match self {
            Self::Empty => false,
            Self::Cuboid { half_extents } => other.overlaps_rect(other_pos, rect_around(pos, *half_extents)),
            Self::Circle { radius } => other.overlaps_circle(other_pos, pos, *radius),
            Self::Capsule { a, b, radius } => other.overlaps_capsule(other_pos, pos + *a, pos + *b, *radius),
            Self::MultiRect { rects } => {
                rects.iter().any(|rect| other.overlaps_rect(other_pos, offset_rect(*rect, pos)))
            },
        }
    }

    fn overlaps_rect(&self, pos: Vec2, rect: Rect) -> bool {
        match self {
            Self::Empty => false,
            Self::Cuboid { half_extents } => overlaps_aabb(rect_around(pos, *half_extents), rect),
            Self::Circle { radius } => circle_aabb_overlap(pos, *radius, rect),
            Self::Capsule { a, b, radius } => segment_intersects_aabb(pos + *a, pos + *b, expand(rect, *radius)),
            Self::MultiRect { rects } => rects.iter().any(|own| overlaps_aabb(offset_rect(*own, pos), rect)),
        }
    }

    fn overlaps_circle(&self, pos: Vec2, center: Vec2, radius: f32) -> bool {
        match self {
            Self::Empty => false,
            Self::Cuboid { half_extents } => circle_aabb_overlap(center, radius, rect_around(pos, *half_extents)),
            Self::Circle { radius: own } => pos.distance_squared(center) <= (radius + own) * (radius + own),
            Self::Capsule { a, b, radius: own } => segment_circle_overlap(pos + *a, pos + *b, center, radius + own),
            Self::MultiRect { rects } => {
                rects.iter().any(|rect| circle_aabb_overlap(center, radius, offset_rect(*rect, pos)))
            },
        }
    }

    /// Overlap with the segment `start..end` swept by `radius`.
    fn overlaps_capsule(&self, pos: Vec2, start: Vec2, end: Vec2, radius: f32) -> bool {
        match self {
            Self::Empty => false,
            Self::Cuboid { half_extents } => {
                segment_intersects_aabb(start, end, expand(rect_around(pos, *half_extents), radius))
            },
            Self::Circle { radius: own } => segment_circle_overlap(start, end, pos, radius + own),
            Self::Capsule { a, b, radius: own } => {
                segment_circle_overlap(pos + *a, pos + *b, start, radius + own)
                    || segment_circle_overlap(start, end, pos + *a, radius + own)
            },
            Self::MultiRect { rects } => {
                rects.iter().any(|rect| segment_intersects_aabb(start, end, expand(offset_rect(*rect, pos), radius)))
            },
        }
    }

    /// First hit of a normalized ray against this shape. Capsules use their bounding box.
    fn ray_hit(&self, pos: Vec2, origin: Vec2, dir: Vec2, max_toi: f32) -> Option<(f32, Vec2)> {
        match self {
            Self::Empty => None,
            Self::Cuboid { half_extents } => ray_aabb(origin, dir, max_toi, rect_around(pos, *half_extents)),
            Self::Circle { radius } => ray_circle(origin, dir, max_toi, pos, *radius),
            Self::Capsule { .. } => ray_aabb(origin, dir, max_toi, self.world_aabb(pos)?),
            Self::MultiRect { rects } => ray_rects(origin, dir, max_toi, rects.iter().map(|r| offset_rect(*r, pos))),
        }
    }

    /// Sweeps the bounding box of `moving` along a normalized ray against this shape's boxes.
    fn sweep_hit(&self, pos: Vec2, moving: &Collider, origin: Vec2, dir: Vec2, max_toi: f32) -> Option<(f32, Vec2)> {
        let half = moving.half_extents();
        let grow = |rect: Rect| Rect { min: rect.min - half, max: rect.max + half };
        match self {
            Self::MultiRect { rects } => {
                ray_rects(origin, dir, max_toi, rects.iter().map(|rect| grow(offset_rect(*rect, pos))))
            },
            _ => ray_aabb(origin, dir, max_toi, grow(self.world_aabb(pos)?)),
        }
    }
}

/// An animation collider accepts a target entity that has the image, texture atlas,
/// and sprite animation components.
#[derive(Component)]
pub struct AnimationCollider(pub Entity);

/// Marks colliders that are expected to remain mostly unchanged between ticks.
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct StaticCollider;

/// The frame and flips already applied to an animation collider.
#[derive(Component, Clone, Copy, Default, Debug, PartialEq, Eq)]
pub struct StoredColliderFrame {
    pub frame_index: usize,
    pub flip_x: bool,
    pub flip_y: bool,
}

#[derive(Resource, Default)]
pub struct SpriteShapeMap {
    /// Each frame's shape for no flip, flipped x, flipped y, and both, indexed by `flip_x + 2 * flip_y`.
    pub shapes: Vec<[Collider; 4]>,
    pub map: HashMap<AssetId<Image>, Vec<usize>>,
}

impl SpriteShapeMap {
    fn frame(&self, sprite: &Sprite) -> Option<StoredColliderFrame> {
        let atlas = sprite.texture_atlas.as_ref()?;
        let frame_index = *self.map.get(&sprite.image.id())?.get(atlas.index)?;
        Some(StoredColliderFrame { frame_index, flip_x: sprite.flip_x, flip_y: sprite.flip_y })
    }

    fn shape(&self, frame: StoredColliderFrame) -> Option<&Collider> {
        let shapes = self.shapes.get(frame.frame_index)?;
        Some(&shapes[usize::from(frame.flip_x) + 2 * usize::from(frame.flip_y)])
    }
}

#[derive(Default)]
pub struct SpriteColliderProfile {
    timing: TickSystemProfile,
    updated: u32,
    cached: u32,
    missing: u32,
    inserted: u32,
}

pub fn update_sprite_colliders(
    shape_map: Res<SpriteShapeMap>,
    sprites: Query<&Sprite>,
    mut colliders: Query<(Entity, &mut Collider, &AnimationCollider, Option<&mut StoredColliderFrame>)>,
    mut commands: Commands,
    time: Res<GameTime>,
    mut profile: Local<SpriteColliderProfile>,
) {
    let start = profiling_enabled().then(Instant::now);
    for (entity, mut collider, source, stored) in &mut colliders {
        // AnimationCollider points to the simulation sprite, never a PresentationProxy.
        let Some(frame) = sprites.get(source.0).ok().and_then(|sprite| shape_map.frame(sprite)) else {
            profile.missing += 1;
            continue;
        };
        if stored.as_deref() == Some(&frame) {
            profile.cached += 1;
            continue;
        }
        let Some(shape) = shape_map.shape(frame) else {
            profile.missing += 1;
            continue;
        };
        *collider = shape.clone();
        profile.updated += 1;
        if let Some(mut stored) = stored {
            *stored = frame;
        } else {
            commands.entity(entity).insert(frame);
            profile.inserted += 1;
        }
    }
    // Counts only accumulate while profiling, so they cannot overflow over a long session.
    let Some(start) = start else {
        *profile = SpriteColliderProfile::default();
        return;
    };
    let Some((avg_ms, max_ms, ticks)) = profile.timing.sample(start.elapsed(), time.hz) else { return };
    let SpriteColliderProfile { updated, cached, missing, inserted, .. } = std::mem::take(&mut *profile);
    info!(
        target: "theseeker_engine::tick_system_profile",
        "sprite_colliders: avg_ms={avg_ms:.2} max_ms={max_ms:.2} updated={updated} cached={cached} \
         missing={missing} inserted={inserted} ticks={ticks}"
    );
}

#[derive(Clone, Copy, Debug)]
pub struct CastHit {
    pub time_of_impact: f32,
    pub normal: Vec2,
}

const COLLISION_CELL_SIZE: f32 = 16.0;

#[derive(Clone, Debug)]
struct CollisionEntry {
    entity: Entity,
    position: Vec2,
    collider: Collider,
    groups: CollisionGroups,
}

/// Colliders bucketed by the uniform grid cells their bounds touch.
#[derive(Default, Debug)]
struct CollisionLayer {
    entries: Vec<CollisionEntry>,
    cells: HashMap<IVec2, Vec<usize>>,
}

impl CollisionLayer {
    fn clear(&mut self) {
        self.entries.clear();
        self.cells.clear();
    }

    fn insert(&mut self, entity: Entity, transform: &GlobalTransform, collider: &Collider, groups: Option<&CollisionGroups>) {
        let position = transform.translation().truncate();
        let Some(bounds) = collider.world_aabb(position) else { return };
        let index = self.entries.len();
        let groups = groups.copied().unwrap_or_default();
        self.entries.push(CollisionEntry { entity, position, collider: collider.clone(), groups });
        for cell in grid_cells(bounds) {
            self.cells.entry(cell).or_default().push(index);
        }
    }

    /// Entries whose cells touch `region`, each once, in cell order.
    fn candidates(&self, region: Rect) -> impl Iterator<Item = &CollisionEntry> {
        let mut seen = HashSet::new();
        let cells = (!self.entries.is_empty()).then(|| grid_cells(region)).into_iter().flatten();
        cells
            .filter_map(|cell| self.cells.get(&cell))
            .flatten()
            .filter(move |index| seen.insert(**index))
            .map(|index| &self.entries[*index])
    }
}

fn grid_cells(bounds: Rect) -> impl Iterator<Item = IVec2> {
    let min = (bounds.min / COLLISION_CELL_SIZE).floor().as_ivec2();
    let max = (bounds.max / COLLISION_CELL_SIZE).floor().as_ivec2();
    (min.y..=max.y).flat_map(move |y| (min.x..=max.x).map(move |x| IVec2::new(x, y)))
}

/// Entity colliders indexed each tick. Static colliders are re-indexed only when they change.
#[derive(Resource, Default, Debug)]
pub struct EntityCollisionWorld {
    static_layer: CollisionLayer,
    dynamic_layer: CollisionLayer,
}


#[derive(Default)]
pub struct CollisionWorldProfile {
    timing: TickSystemProfile,
    static_rebuilds: u32,
}

type ColliderItem<'a> = (Entity, &'a GlobalTransform, &'a Collider, Option<&'a CollisionGroups>);

pub fn rebuild_entity_collision_world(
    mut world: ResMut<EntityCollisionWorld>,
    static_query: Query<ColliderItem, With<StaticCollider>>,
    dynamic_query: Query<ColliderItem, Without<StaticCollider>>,
    static_changes: Query<
        (),
        (
            With<StaticCollider>,
            Or<(
                Added<StaticCollider>,
                Added<Collider>,
                Changed<GlobalTransform>,
                Changed<Collider>,
                Changed<CollisionGroups>,
            )>,
        ),
    >,
    mut removed_static: RemovedComponents<StaticCollider>,
    time: Res<GameTime>,
    mut profile: Local<CollisionWorldProfile>,
) {
    let start = profiling_enabled().then(Instant::now);
    let world = &mut *world;
    if world.static_layer.entries.is_empty() || !static_changes.is_empty() || removed_static.read().next().is_some() {
        world.static_layer.clear();
        for (entity, transform, collider, groups) in &static_query {
            world.static_layer.insert(entity, transform, collider, groups);
        }
        profile.static_rebuilds += 1;
    }
    world.dynamic_layer.clear();
    for (entity, transform, collider, groups) in &dynamic_query {
        world.dynamic_layer.insert(entity, transform, collider, groups);
    }
    let Some(start) = start else { return };
    let Some((avg_ms, max_ms, ticks)) = profile.timing.sample(start.elapsed(), time.hz) else { return };
    let rebuilds = std::mem::take(&mut profile.static_rebuilds);
    let (statics, dynamics) = (world.static_layer.entries.len(), world.dynamic_layer.entries.len());
    info!(
        target: "theseeker_engine::tick_system_profile",
        "collision_world: avg_ms={avg_ms:.2} max_ms={max_ms:.2} static_entries={statics} \
         dynamic_entries={dynamics} static_rebuilds={rebuilds} ticks={ticks}"
    );
}

/// Spatial queries against the entity collision world. Terrain uses [`TileCollisionWorld`].
pub type PhysicsWorld<'w> = Res<'w, EntityCollisionWorld>;

#[derive(Clone, Copy)]
pub struct ShapeCast<'a> {
    pub origin: Vec2,
    pub dir: Vec2,
    pub shape: &'a Collider,
    pub max_toi: f32,
    pub interaction: CollisionGroups,
    pub exclude: Option<Entity>,
}

impl EntityCollisionWorld {
    /// Entities overlapping `shape` at `origin`, touching edges included.
    pub fn intersect(
        &self,
        origin: Vec2,
        shape: &Collider,
        interaction: CollisionGroups,
        exclude: Option<Entity>,
    ) -> Vec<Entity> {
        let Some(region) = shape.world_aabb(origin) else { return Vec::new() };
        self.candidates(region)
            .filter(|entry| accepts(entry, interaction, exclude))
            .filter(|entry| shape.overlaps(origin, &entry.collider, entry.position))
            .map(|entry| entry.entity)
            .collect()
    }

    pub fn ray_cast(
        &self,
        origin: Vec2,
        dir: Vec2,
        max_toi: f32,
        interaction: CollisionGroups,
        exclude: Option<Entity>,
    ) -> Option<(Entity, CastHit)> {
        let dir = unit(dir)?;
        let region = Rect::from_corners(origin, origin + dir * max_toi);
        nearest(self.candidates(region), max_toi, |entry, best| {
            accepts(entry, interaction, exclude).then(|| entry.collider.ray_hit(entry.position, origin, dir, best))?
        })
    }

    /// The nearest accepted entity hit by `cast.shape`'s bounding box swept along `cast.dir`.
    pub fn cast_shape(&self, cast: ShapeCast<'_>, mut predicate: impl FnMut(Entity) -> bool) -> Option<(Entity, CastHit)> {
        let dir = unit(cast.dir)?;
        let start = cast.shape.world_aabb(cast.origin)?;
        let region = start.union(cast.shape.world_aabb(cast.origin + dir * cast.max_toi)?);
        nearest(self.candidates(region), cast.max_toi, |entry, best| {
            let accepted = accepts(entry, cast.interaction, cast.exclude) && predicate(entry.entity);
            accepted.then(|| entry.collider.sweep_hit(entry.position, cast.shape, cast.origin, dir, best))?
        })
    }

    fn candidates(&self, region: Rect) -> impl Iterator<Item = &CollisionEntry> {
        self.static_layer.candidates(region).chain(self.dynamic_layer.candidates(region))
    }
}

/// Candidates are tested in order against the best time so far; later ties win.
fn nearest<'a>(
    candidates: impl Iterator<Item = &'a CollisionEntry>,
    max_toi: f32,
    mut hit: impl FnMut(&CollisionEntry, f32) -> Option<(f32, Vec2)>,
) -> Option<(Entity, CastHit)> {
    candidates.fold(None, |best, entry| {
        let best_toi = best.map_or(max_toi, |(_, best): (Entity, CastHit)| best.time_of_impact);
        let hit = hit(entry, best_toi).map(|(time_of_impact, normal)| CastHit { time_of_impact, normal });
        hit.map(|hit| (entry.entity, hit)).or(best)
    })
}

fn accepts(entry: &CollisionEntry, interaction: CollisionGroups, exclude: Option<Entity>) -> bool {
    Some(entry.entity) != exclude && interaction.interacts_with(entry.groups)
}

/// Linear velocity is represented by the LinearVelocity component.
#[derive(Component, Default, Clone, Copy, Debug, PartialEq)]
pub struct LinearVelocity(pub Vec2);

#[derive(Component, Default, Debug)]
pub struct Sensor;

fn offset_rect(rect: Rect, offset: Vec2) -> Rect {
    Rect { min: rect.min + offset, max: rect.max + offset }
}

/// `dir` divided by its length, unless it is too short to have a direction.
fn unit(dir: Vec2) -> Option<Vec2> {
    let length = dir.length();
    (length > f32::EPSILON).then(|| dir / length)
}

fn expand(rect: Rect, amount: f32) -> Rect {
    Rect { min: rect.min - Vec2::splat(amount), max: rect.max + Vec2::splat(amount) }
}

fn overlaps_aabb(a: Rect, b: Rect) -> bool {
    const EPS: f32 = 0.0001;
    a.max.x > b.min.x - EPS && a.min.x < b.max.x + EPS && a.max.y > b.min.y - EPS && a.min.y < b.max.y + EPS
}

fn circle_aabb_overlap(center: Vec2, radius: f32, aabb: Rect) -> bool {
    center.distance_squared(center.clamp(aabb.min, aabb.max)) <= radius * radius
}

fn segment_intersects_aabb(start: Vec2, end: Vec2, aabb: Rect) -> bool {
    let delta = end - start;
    let length = delta.length();
    if length <= f32::EPSILON {
        return aabb.contains(start);
    }
    ray_aabb(start, delta / length, length, aabb).is_some()
}

fn segment_circle_overlap(start: Vec2, end: Vec2, center: Vec2, radius: f32) -> bool {
    let ab = end - start;
    let length_squared = ab.length_squared();
    let closest = if length_squared <= f32::EPSILON {
        start
    } else {
        start + ab * ((center - start).dot(ab) / length_squared).clamp(0.0, 1.0)
    };
    closest.distance_squared(center) <= radius * radius
}

/// Slab test for a normalized ray. Starting inside reports distance zero and a zero normal.
fn ray_aabb(origin: Vec2, dir: Vec2, max_toi: f32, aabb: Rect) -> Option<(f32, Vec2)> {
    let mut t_min = 0.0;
    let mut t_max = max_toi;
    let mut normal = Vec2::ZERO;
    for axis in 0..2 {
        let (near, far) = slab(origin[axis], dir[axis], aabb.min[axis], aabb.max[axis])?;
        if near > t_min {
            t_min = near;
            normal = Vec2::ZERO;
            normal[axis] = -dir[axis].signum();
        }
        t_max = t_max.min(far);
        if t_min > t_max {
            return None;
        }
    }
    Some((t_min, normal))
}

/// Parametric entry and exit of one axis slab. A parallel ray inside the slab never limits it.
fn slab(origin: f32, dir: f32, min: f32, max: f32) -> Option<(f32, f32)> {
    if dir.abs() >= f32::EPSILON {
        let inverse = 1.0 / dir;
        let (t1, t2) = ((min - origin) * inverse, (max - origin) * inverse);
        return Some((t1.min(t2), t1.max(t2)));
    }
    if origin < min || origin > max {
        return None;
    }
    Some((f32::NEG_INFINITY, f32::INFINITY))
}

fn ray_rects(origin: Vec2, dir: Vec2, max_toi: f32, rects: impl Iterator<Item = Rect>) -> Option<(f32, Vec2)> {
    let mut best = (max_toi, Vec2::ZERO);
    for rect in rects {
        if let Some(hit) = ray_aabb(origin, dir, best.0, rect) {
            best = hit;
        }
    }
    (best.0 < max_toi).then_some(best)
}

fn ray_circle(origin: Vec2, dir: Vec2, max_toi: f32, center: Vec2, radius: f32) -> Option<(f32, Vec2)> {
    let oc = origin - center;
    let b = oc.dot(dir);
    let discriminant = b * b - (oc.dot(oc) - radius * radius);
    if discriminant < 0.0 {
        return None;
    }
    let root = discriminant.sqrt();
    let t = if -b - root < 0.0 { -b + root } else { -b - root };
    if t < 0.0 || t > max_toi {
        return None;
    }
    Some((t, (origin + dir * t - center).normalize_or_zero()))
}

pub mod inside {
    use super::{groups, CollisionGroups, Group, ENEMY_INSIDE};
    use bevy::prelude::*;

    /// Marker put on the player while clipped inside an enemy.
    #[derive(Component)]
    pub struct PlayerInsideEnemy;

    /// Marker put on the enemy while the player is inside it.
    #[derive(Component)]
    pub struct EnemyInsidePlayer;

    /// Attach ENEMY_INSIDE group to `player`, add marker to `enemy`.
    pub fn set(commands: &mut Commands, player: Entity, enemy: Entity) {
        commands
            .entity(player)
            .try_insert(CollisionGroups::new(ENEMY_INSIDE, Group::all()))
            .try_insert(PlayerInsideEnemy);
        commands.entity(enemy).try_insert(EnemyInsidePlayer);
    }

    /// Revert both sides to their default bodies.
    pub fn clear(commands: &mut Commands, enemy: Entity, player: Entity) {
        commands.entity(player).try_remove::<PlayerInsideEnemy>().try_insert(groups::player_body());
        commands.entity(enemy).try_remove::<EnemyInsidePlayer>();
    }
}
