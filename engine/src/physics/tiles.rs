//! Tile-grid collision for static terrain plus counted dynamic overlays.
use bevy::prelude::*;

use super::geometry::{self, Axis, ProbeDirection};
use super::{ray_aabb, rect_around, CastHit};

/// Tiles must overlap by more than this to collide; touching edges do not.
const OVERLAP_EPS: f32 = 0.0001;
/// Directional probes accept obstacles this far inside the probing edge, about one float ulp at
/// distant world coordinates.
const CONTACT_EPS: f32 = 0.001;

#[derive(Component, Debug)]
pub struct CoverCollisionTiles {
    pub tiles: Vec<IVec2>,
}

#[derive(Clone, Copy, Debug)]
pub struct TileRange {
    pub min_x: i32,
    pub max_x: i32,
    pub min_y: i32,
    pub max_y: i32,
}

impl TileRange {
    /// Tile coordinates in row-major order.
    pub fn tiles(self) -> impl Iterator<Item = IVec2> {
        (self.min_y..=self.max_y).flat_map(move |y| (self.min_x..=self.max_x).map(move |x| IVec2::new(x, y)))
    }
}

#[derive(Clone, Copy, Debug)]
pub struct TileSolidInfo {
    pub terrain: bool,
    pub dynamic: bool,
}

#[derive(Clone, Copy, Debug)]
pub struct MoveResult {
    pub position: Vec2,
    pub velocity: Vec2,
    pub grounded: bool,
    pub hit_wall: bool,
    pub hit_ceiling: bool,
}

/// Coordinates are world-space, y-up. Tile rows are stored in image order (top to bottom),
/// matching the PNG level pipeline.
#[derive(Resource, Clone, Debug)]
pub struct TileCollisionWorld {
    pub tile_size: f32,
    pub width: u32,
    pub height: u32,
    terrain: Vec<bool>,
    /// Number of dynamic contributions (such as secret-room covers) occupying each tile.
    dynamic: Vec<u16>,
}

impl Default for TileCollisionWorld {
    fn default() -> Self {
        Self { tile_size: 1.0, width: 0, height: 0, terrain: Vec::new(), dynamic: Vec::new() }
    }
}

impl TileCollisionWorld {
    pub fn clear(&mut self) {
        *self = Self::default();
    }

    pub fn is_ready(&self) -> bool {
        self.width > 0
            && self.height > 0
            && self.tile_size > 0.0
            && self.terrain.len() == (self.width * self.height) as usize
    }

    pub fn rebuild(&mut self, width: u32, height: u32, tile_size: f32, terrain: &[bool]) {
        *self = Self { tile_size, width, height, terrain: terrain.to_vec(), dynamic: vec![0; terrain.len()] };
    }

    /// Changes base terrain without disturbing overlapping dynamic contributions.
    pub fn patch_terrain(&mut self, changes: impl IntoIterator<Item = (usize, bool)>) {
        let terrain = &mut self.terrain;
        let len = terrain.len();
        changes.into_iter().filter(|(index, _)| *index < len).for_each(|(index, solid)| terrain[index] = solid);
    }

    pub fn add_dynamic_tiles(&mut self, tiles: &[IVec2]) {
        self.count_dynamic_tiles(tiles, u16::saturating_add);
    }

    pub fn remove_dynamic_tiles(&mut self, tiles: &[IVec2]) {
        self.count_dynamic_tiles(tiles, u16::saturating_sub);
    }

    fn count_dynamic_tiles(&mut self, tiles: &[IVec2], count: fn(u16, u16) -> u16) {
        let (width, height) = (self.width, self.height);
        for index in tiles.iter().filter_map(|tile| tile_index(width, height, tile.x, tile.y)) {
            self.dynamic[index] = count(self.dynamic[index], 1);
        }
    }

    /// Resolves vertical displacement first, then horizontal displacement from the new height.
    pub fn resolve_movement(&self, position: Vec2, velocity: Vec2, half_extents: Vec2) -> MoveResult {
        if !self.is_ready() || half_extents.x <= 0.0 || half_extents.y <= 0.0 {
            let position = position + velocity;
            return MoveResult { position, velocity, grounded: false, hit_wall: false, hit_ceiling: false };
        }
        let (y, hit_y) = self.resolve_axis(Axis::Y, position, velocity.y, half_extents);
        let (x, hit_x) = self.resolve_axis(Axis::X, Vec2::new(position.x, y), velocity.x, half_extents);
        MoveResult {
            position: Vec2::new(x, y),
            velocity: Vec2::new(if hit_x { 0.0 } else { velocity.x }, if hit_y { 0.0 } else { velocity.y }),
            grounded: hit_y && velocity.y < 0.0,
            hit_wall: hit_x,
            hit_ceiling: hit_y && velocity.y > 0.0,
        }
    }

    /// Moves along one axis and stops at the nearest solid tile. Grid cells select the candidates
    /// around the destination; only the cross axis is tested for overlap.
    pub fn resolve_axis(&self, axis: Axis, position: Vec2, delta: f32, half_extents: Vec2) -> (f32, bool) {
        let i = axis.index();
        if delta == 0.0 {
            return (position[i], false);
        }
        let mut target = position;
        target[i] += delta;
        let body = rect_around(target, half_extents);
        self.solid_tiles(body)
            .filter(|tile| overlaps_on(body, *tile, 1 - i))
            .fold((target[i], false), |(resolved, blocked), tile| {
                geometry::nearest_axis_stop(tile.min[i], tile.max[i], delta, half_extents[i], resolved)
                    .map_or((resolved, blocked), |stop| (stop, true))
            })
    }

    /// Distance to the nearest solid tile in `direction`, at most `max_distance`. Ground probes
    /// measure from the bottom edge; other directions measure from the center.
    pub fn distance(&self, direction: ProbeDirection, position: Vec2, half_extents: Vec2, max_distance: f32) -> f32 {
        let axis = direction.axis().index();
        let body = rect_around(position, half_extents);
        let mut probe = body;
        if direction.positive() {
            probe.min[axis] = body.max[axis];
            probe.max[axis] = body.max[axis] + max_distance;
        } else {
            probe.max[axis] = body.min[axis];
            probe.min[axis] = body.min[axis] - max_distance;
        }
        self.solid_tiles(probe)
            .filter(|tile| overlaps_on(body, *tile, 1 - axis))
            .filter_map(|tile| {
                geometry::distance_to_interval(
                    direction,
                    tile.min[axis],
                    tile.max[axis],
                    position[axis],
                    half_extents[axis],
                    CONTACT_EPS,
                )
            })
            .fold(max_distance, f32::min)
    }

    pub fn overlaps_solid(&self, position: Vec2, half_extents: Vec2) -> bool {
        let body = rect_around(position, half_extents);
        self.solid_tiles(body).any(|tile| overlaps_on(body, tile, 0) && overlaps_on(body, tile, 1))
    }

    /// Grid traversal from `origin`. A hit inside the starting tile reports a zero normal.
    /// Traversal distances are measured from the point where the ray enters the world bounds.
    pub fn ray_cast_solid(&self, origin: Vec2, dir: Vec2, max_distance: f32) -> Option<CastHit> {
        let length = dir.length();
        if !self.is_ready() || max_distance < 0.0 || length <= f32::EPSILON {
            return None;
        }
        let dir = dir / length;
        let bounds = self.bounds_rect();
        let (entry, start) = if bounds.contains(origin) {
            (0.0, origin)
        } else {
            let (entry, _) = ray_aabb(origin, dir, max_distance, bounds)?;
            (entry, origin + dir * entry)
        };
        let tile = IVec2::new(
            self.world_to_tile_x(start.x).clamp(0, self.width as i32 - 1),
            self.world_to_tile_y(start.y).clamp(0, self.height as i32 - 1),
        );
        if self.is_solid(tile.x, tile.y) {
            return Some(CastHit { time_of_impact: entry, normal: Vec2::ZERO });
        }
        let (min_y, max_y) = self.tile_bounds_y(tile.y);
        let x = ray_axis(start.x, dir.x, (tile.x as f32 * self.tile_size, (tile.x + 1) as f32 * self.tile_size));
        let y = ray_axis(start.y, dir.y, (min_y, max_y));
        let per_tile = Vec2::splat(self.tile_size) / dir.abs();
        GridRay { tile, sign: Vec2::new(x.0, y.0), next: Vec2::new(x.1, y.1), per_tile }
            .take_while(|step| step.distance <= max_distance && self.index(step.tile.x, step.tile.y).is_some())
            .find(|step| self.is_solid(step.tile.x, step.tile.y))
            .map(|step| CastHit { time_of_impact: step.distance, normal: step.normal })
    }

    pub fn segment_cast_solid(&self, start: Vec2, end: Vec2) -> Option<CastHit> {
        let delta = end - start;
        let length = delta.length();
        if length <= f32::EPSILON {
            return self
                .is_solid_at_world_point(start)
                .then_some(CastHit { time_of_impact: 0.0, normal: Vec2::ZERO });
        }
        self.ray_cast_solid(start, delta / length, length)
    }

    pub fn bounds_rect(&self) -> Rect {
        Rect::from_corners(Vec2::ZERO, Vec2::new(self.width as f32, self.height as f32) * self.tile_size)
    }

    /// Tiles whose grid cells contain part of `rect`, clamped to the world.
    pub fn tile_range_for_rect(&self, rect: Rect) -> Option<TileRange> {
        if !self.is_ready() {
            return None;
        }
        let (max_tx, max_ty) = (self.width as i32 - 1, self.height as i32 - 1);
        let (min_x, max_x) = (self.world_to_tile_x(rect.min.x), self.world_to_tile_x(rect.max.x));
        let (top, bottom) = (self.world_to_tile_y(rect.max.y), self.world_to_tile_y(rect.min.y));
        let (min_y, max_y) = (top.min(bottom), top.max(bottom));
        if max_x < 0 || min_x > max_tx || max_y < 0 || min_y > max_ty || min_x > max_x {
            return None;
        }
        Some(TileRange {
            min_x: min_x.clamp(0, max_tx),
            max_x: max_x.clamp(0, max_tx),
            min_y: min_y.clamp(0, max_ty),
            max_y: max_y.clamp(0, max_ty),
        })
    }

    pub fn tile_bounds(&self, tx: i32, ty: i32) -> (Vec2, Vec2) {
        let min_x = tx as f32 * self.tile_size;
        let (min_y, max_y) = self.tile_bounds_y(ty);
        (Vec2::new(min_x, min_y), Vec2::new(min_x + self.tile_size, max_y))
    }

    pub fn tile_solid_info(&self, tx: i32, ty: i32) -> Option<TileSolidInfo> {
        let index = self.index(tx, ty)?;
        let terrain = self.terrain.get(index).copied().unwrap_or(false);
        let dynamic = self.dynamic.get(index).is_some_and(|count| *count > 0);
        (terrain || dynamic).then_some(TileSolidInfo { terrain, dynamic })
    }

    pub fn is_solid_at_world_point(&self, point: Vec2) -> bool {
        self.is_ready() && self.is_solid(self.world_to_tile_x(point.x), self.world_to_tile_y(point.y))
    }

    /// Solid tiles whose grid cells contain part of `region`, in row-major order.
    fn solid_tiles(&self, region: Rect) -> impl Iterator<Item = Rect> + '_ {
        self.tile_range_for_rect(region)
            .into_iter()
            .flat_map(TileRange::tiles)
            .filter(|tile| self.is_solid(tile.x, tile.y))
            .map(|tile| {
                let (min, max) = self.tile_bounds(tile.x, tile.y);
                Rect { min, max }
            })
    }

    fn index(&self, tx: i32, ty: i32) -> Option<usize> {
        tile_index(self.width, self.height, tx, ty)
    }

    fn is_solid(&self, tx: i32, ty: i32) -> bool {
        self.tile_solid_info(tx, ty).is_some()
    }

    fn tile_bounds_y(&self, ty: i32) -> (f32, f32) {
        let min_y = (self.height as f32 - 1.0 - ty as f32) * self.tile_size;
        (min_y, min_y + self.tile_size)
    }

    fn world_to_tile_x(&self, world_x: f32) -> i32 {
        (world_x / self.tile_size).floor() as i32
    }

    fn world_to_tile_y(&self, world_y: f32) -> i32 {
        (self.height as f32 - 1.0 - (world_y / self.tile_size).floor()) as i32
    }
}

fn overlaps_on(a: Rect, b: Rect, axis: usize) -> bool {
    geometry::intervals_overlap(a.min[axis], a.max[axis], b.min[axis], b.max[axis], OVERLAP_EPS)
}

fn tile_index(width: u32, height: u32, tx: i32, ty: i32) -> Option<usize> {
    let inside = (0..width as i32).contains(&tx) && (0..height as i32).contains(&ty);
    inside.then(|| (ty as u32 * width + tx as u32) as usize)
}

/// World-space step sign and parametric distance to the first cell boundary along one axis.
fn ray_axis(origin: f32, dir: f32, (min, max): (f32, f32)) -> (f32, f32) {
    if dir.abs() < f32::EPSILON {
        (0.0, f32::INFINITY)
    } else if dir > 0.0 {
        (1.0, (max - origin) / dir)
    } else {
        (-1.0, (min - origin) / dir)
    }
}

/// Cells entered by a ray, nearest boundary first. Rows count downward, so a positive world y
/// step moves to a smaller row index.
struct GridRay {
    tile: IVec2,
    sign: Vec2,
    next: Vec2,
    per_tile: Vec2,
}

struct GridStep {
    tile: IVec2,
    distance: f32,
    normal: Vec2,
}

impl Iterator for GridRay {
    type Item = GridStep;

    fn next(&mut self) -> Option<GridStep> {
        let axis = usize::from(self.next.x >= self.next.y);
        let distance = self.next[axis];
        let tile_step = if axis == 0 { self.sign.x } else { -self.sign.y };
        self.tile[axis] += tile_step as i32;
        self.next[axis] += self.per_tile[axis];
        let mut normal = Vec2::ZERO;
        normal[axis] = -self.sign[axis];
        Some(GridStep { tile: self.tile, distance, normal })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn world(width: u32, height: u32, solids: &[(i32, i32)]) -> TileCollisionWorld {
        let mut terrain = vec![false; (width * height) as usize];
        for &(x, y) in solids {
            terrain[(y as u32 * width + x as u32) as usize] = true;
        }
        let mut world = TileCollisionWorld::default();
        world.rebuild(width, height, 10.0, &terrain);
        world
    }

    #[test]
    fn exact_touching_wall_blocks_motion_into_wall() {
        let world = world(4, 4, &[(2, 2)]);
        let result = world.resolve_movement(Vec2::new(15.0, 15.0), Vec2::X, Vec2::splat(5.0));

        assert_eq!(result.position, Vec2::new(15.0, 15.0));
        assert_eq!(result.velocity, Vec2::ZERO);
        assert!(result.hit_wall);
        assert!(!result.grounded);
    }

    #[test]
    fn corner_contact_resolves_y_before_x() {
        let world = world(4, 4, &[(1, 3), (2, 2)]);
        let result = world.resolve_movement(Vec2::new(15.0, 18.0), Vec2::new(10.0, -8.0), Vec2::splat(5.0));

        assert_eq!(result.position, Vec2::new(15.0, 15.0));
        assert_eq!(result.velocity, Vec2::ZERO);
        assert!(result.grounded);
        assert!(result.hit_wall);
    }

    #[test]
    fn distance_queries_report_ground_wall_and_ceiling_contacts() {
        let world = world(4, 4, &[(1, 3), (2, 2), (1, 0)]);
        let pos = Vec2::new(15.0, 15.0);
        let half = Vec2::splat(5.0);

        assert_eq!(world.distance(ProbeDirection::Down, pos, half, 20.0), 0.0);
        assert_eq!(world.distance(ProbeDirection::Right, pos, half, 20.0), 5.0);
        assert_eq!(world.distance(ProbeDirection::Up, pos, half, 30.0), 15.0);
        assert_eq!(world.distance(ProbeDirection::Up, Vec2::new(15.0, 25.0), half, 20.0), 5.0);
    }

    #[test]
    fn dynamic_tiles_participate_in_overlap_queries_and_can_be_removed() {
        let mut world = world(4, 4, &[]);
        let tile = IVec2::new(1, 2);
        let pos = Vec2::new(15.0, 15.0);

        assert!(!world.overlaps_solid(pos, Vec2::splat(4.0)));
        world.add_dynamic_tiles(&[tile]);
        assert!(world.overlaps_solid(pos, Vec2::splat(4.0)));
        world.remove_dynamic_tiles(&[tile]);
        assert!(!world.overlaps_solid(pos, Vec2::splat(4.0)));
    }

    #[test]
    fn world_points_map_to_top_down_tile_rows() {
        let world = world(4, 4, &[(0, 0)]);

        assert!(world.is_solid_at_world_point(Vec2::new(5.0, 35.0)));
        assert!(!world.is_solid_at_world_point(Vec2::new(5.0, 5.0)));
    }

    #[test]
    fn ray_cast_solid_reports_toi_and_surface_normal() {
        let world = world(4, 4, &[(1, 3), (2, 2)]);

        let ground = world.ray_cast_solid(Vec2::new(15.0, 35.0), Vec2::NEG_Y, 40.0).expect("ground hit");
        assert_eq!(ground.time_of_impact, 25.0);
        assert_eq!(ground.normal, Vec2::Y);

        let wall = world.segment_cast_solid(Vec2::new(0.0, 15.0), Vec2::new(35.0, 15.0)).expect("wall hit");
        assert_eq!(wall.time_of_impact, 20.0);
        assert_eq!(wall.normal, Vec2::NEG_X);
    }
}
