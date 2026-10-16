//! Where the near background opens onto the planes behind it.
//!
//! The near wall follows the platforms, as the artist's does: it hugs terrain surfaces with a
//! skin a few tiles thick, leaves others bare, swells into occasional masses beside the terrain,
//! and bridges narrow gaps between platforms. Everything farther from terrain stays open.
//! Openings are never slivers, the near wall never floats or thins to a strand in the air, and
//! small openings are closed.
use super::*;
use distance::DistanceField;

/// Range of the near wall's skin over terrain, in tiles; below one the terrain stays bare.
const SKIN: (f32, f32) = (-2.5, 5.0);
const SKIN_JITTER: f32 = 1.5;
/// Where the mass noise passes `MASS_THRESHOLD`, the skin thickens by up to `MASS_REACH` tiles.
const MASS_THRESHOLD: f32 = 0.62;
const MASS_REACH: f32 = 6.0;
/// Gaps up to about twice this radius, in tiles, are bridged.
const BRIDGE_RADIUS: f32 = 3.0;
pub(super) const MIN_OPENING: usize = 40;

/// Whether each tile is covered by the near background: all terrain and the cave beside it.
pub(super) fn near_wall(terrain: &[bool], size: UVec2, seed: u64) -> Vec<bool> {
    let _timer = diagnostics::StageTimer::new("near_openings");
    let (width, height) = (size.x as usize, size.y as usize);
    let mut field = DistanceField::default();
    field.compute(width, height, |x, y| terrain[y * width + x]);
    let terrain_distance: Vec<f32> = field.squared.iter().map(|squared| squared.sqrt()).collect();
    let bridged = closing(&field, width, height, &mut DistanceField::default());
    let noise = |salt: u64, scale: Vec2, x: usize, y: usize| value_noise(Vec2::new(x as f32, y as f32) / scale, seed ^ salt);
    let candidate: Vec<bool> = (0..width * height).map(|index| {
        let (x, y) = (index % width, index / width);
        let mass = ((noise(0x53, Vec2::new(28.0, 18.0), x, y) - MASS_THRESHOLD) / (1.0 - MASS_THRESHOLD)).clamp(0.0, 1.0);
        let skin = SKIN.0 + (SKIN.1 - SKIN.0) * noise(0x51, Vec2::new(13.0, 9.0), x, y)
            + SKIN_JITTER * (noise(0x52, Vec2::new(2.0, 3.0), x, y) - 0.5) + MASS_REACH * mass;
        !terrain[index] && terrain_distance[index] > skin && !bridged[index]
    }).collect();
    let wall: Vec<bool> = candidate.iter().map(|&open| !open).collect();
    tidy(&wall, terrain, size)
}

/// Terrain closed with a disk of `BRIDGE_RADIUS`: gaps narrower than the disk fill in.
fn closing(terrain: &DistanceField, width: usize, height: usize, scratch: &mut DistanceField) -> Vec<bool> {
    let reach = BRIDGE_RADIUS * BRIDGE_RADIUS + BRIDGE_RADIUS;
    scratch.compute(width, height, |x, y| terrain.squared[y * width + x] > reach);
    scratch.squared.iter().map(|&squared| squared > reach).collect()
}

/// Keeps a near-wall mask within the artist's rules: no one-tile openings or strands, no near
/// wall detached from terrain, and no opening smaller than `MIN_OPENING` tiles.
pub(super) fn tidy(wall: &[bool], terrain: &[bool], size: UVec2) -> Vec<bool> {
    let opening: Vec<bool> = wall.iter().map(|&covered| !covered).collect();
    let opening = open_square(&opening, size);
    let exposed: Vec<bool> = opening.iter().map(|&open| !open).collect();
    let mut wall: Vec<bool> = open_square(&exposed, size).iter().zip(terrain).map(|(&covered, &solid)| covered || solid).collect();
    let touching = square(terrain, size, true);
    let floating: Vec<bool> = wall.iter().zip(terrain).map(|(&covered, &solid)| covered && !solid).collect();
    let detached = components(&floating, size).into_iter().filter(|component| !component.iter().any(|&index| touching[index]));
    for index in detached.flatten() { wall[index] = false; }
    let opening: Vec<bool> = wall.iter().map(|&covered| !covered).collect();
    let small = components(&opening, size).into_iter().filter(|component| component.len() < MIN_OPENING);
    for index in small.flatten() { wall[index] = true; }
    wall
}

/// Morphological opening with a 3x3 square; tiles beyond the edge count as unset.
fn open_square(mask: &[bool], size: UVec2) -> Vec<bool> {
    square(&square(mask, size, false), size, true)
}

/// A 3x3 dilation (`grow`) or erosion of `mask`.
fn square(mask: &[bool], size: UVec2, grow: bool) -> Vec<bool> {
    let (width, height) = (size.x as i32, size.y as i32);
    let at = |x: i32, y: i32| x >= 0 && y >= 0 && x < width && y < height && mask[(y * width + x) as usize];
    (0..width * height).map(|index| {
        let (x, y) = (index % width, index / width);
        let mut neighbours = (-1..=1).flat_map(|dy| (-1..=1).map(move |dx| (x + dx, y + dy)));
        if grow { neighbours.any(|(nx, ny)| at(nx, ny)) } else { neighbours.all(|(nx, ny)| at(nx, ny)) }
    }).collect()
}

/// Four-connected components of the set tiles.
pub(super) fn components(mask: &[bool], size: UVec2) -> Vec<Vec<usize>> {
    let mut seen = vec![false; mask.len()];
    let mut found = Vec::new();
    for start in 0..mask.len() {
        if mask[start] && !seen[start] { found.push(flood(mask, &mut seen, start, size)); }
    }
    found
}

fn flood(mask: &[bool], seen: &mut [bool], start: usize, size: UVec2) -> Vec<usize> {
    let width = size.x as usize;
    let mut component = Vec::new();
    let mut stack = vec![start];
    while let Some(index) = stack.pop() {
        if seen[index] { continue; }
        seen[index] = true;
        component.push(index);
        let (x, y) = (index % width, index / width);
        let neighbours = [(x > 0).then(|| index - 1), (x + 1 < width).then(|| index + 1),
            (y > 0).then(|| index - width), (y + 1 < size.y as usize).then(|| index + width)];
        stack.extend(neighbours.into_iter().flatten().filter(|&cell| mask[cell] && !seen[cell]));
    }
    component
}

/// Smooth value noise in [0, 1) with one lattice point per unit of `position`.
fn value_noise(position: Vec2, seed: u64) -> f32 {
    let cell = position.floor();
    let t = position - cell;
    let t = t * t * (Vec2::splat(3.0) - 2.0 * t);
    let lattice = |dx: i32, dy: i32| {
        let hash = tile_hash(cell.x as i32 + dx, cell.y as i32 + dy, 0, seed);
        (hash >> 40) as f32 / (1u64 << 24) as f32
    };
    let top = lattice(0, 0) + (lattice(1, 0) - lattice(0, 0)) * t.x;
    let bottom = lattice(0, 1) + (lattice(1, 1) - lattice(0, 1)) * t.x;
    top + (bottom - top) * t.y
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Platforms of assorted widths with gaps of assorted heights.
    fn caves(size: UVec2) -> Vec<bool> {
        (0..size.x * size.y).map(|index| {
            let (x, y) = (index % size.x, index / size.x);
            let band = y % 37;
            band < 6 + x % 5 || (x / 23) % 4 == 0 && band < 20 || x < 2 || y < 2 || x + 2 >= size.x || y + 2 >= size.y
        }).collect()
    }

    #[test]
    fn near_wall_covers_terrain_and_follows_it_without_slivers_or_islands() {
        let size = UVec2::new(160, 150);
        let terrain = caves(size);
        for seed in [1, 2, 3] {
            let wall = near_wall(&terrain, size, seed);
            assert!(terrain.iter().zip(&wall).all(|(&solid, &covered)| !solid || covered));
            let open_cave = terrain.iter().filter(|&&solid| !solid).count() as f32;
            let opening: Vec<bool> = wall.iter().map(|&covered| !covered).collect();
            let share = opening.iter().filter(|&&open| open).count() as f32 / open_cave;
            assert!((0.3..=0.85).contains(&share), "seed {seed}: {share}");
            let mut field = DistanceField::default();
            field.compute(size.x as usize, size.y as usize, |x, y| terrain[y * size.x as usize + x]);
            let reach = SKIN.1 + SKIN_JITTER + MASS_REACH + 1.0;
            assert!(field.squared.iter().zip(&wall).all(|(&squared, &covered)| !covered || squared <= reach * reach),
                "seed {seed}: open cave away from platforms stays open");
            assert!(components(&opening, size).iter().all(|component| component.len() >= MIN_OPENING));
            assert_eq!(open_square(&opening, size), opening, "openings are at least 2 tiles across");
            assert_eq!(tidy(&wall, &terrain, size), wall, "a tidy mask stays unchanged");
            let touching = square(&terrain, size, true);
            let air_wall: Vec<bool> = wall.iter().zip(&terrain).map(|(&covered, &solid)| covered && !solid).collect();
            assert!(components(&air_wall, size).iter().all(|component| component.iter().any(|&index| touching[index])));
        }
    }
}
