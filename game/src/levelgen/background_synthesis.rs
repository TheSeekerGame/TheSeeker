//! Exemplar-driven near-background detailing: image analogies on an opening mask.
//!
//! An exemplar pairs a context (near wall, openings, and solid terrain) with the artist's
//! IntGrid values for the same cells. Synthesis reads a target context and copies each value
//! from the exemplar cell whose surroundings match best, preferring to continue the copies of
//! already-written neighbours so the artist's strokes and clusters stay whole. Cells deep inside
//! the wall take its base value and cells deep inside an opening stay empty.
//! `tools/backgrounds/synthesis.py` details the middle and far planes the same way.
use super::*;
use std::collections::HashMap;

/// Context windows span 9x9 cells; the inner 5x5 counts double.
const RADIUS: i32 = 4;
const SIDE: i32 = 2 * RADIUS + 1;
/// Cells written before a cell in row order, within two cells of it.
const CAUSAL: [(i32, i32); 11] = [(-2, -2), (-2, -1), (-2, 0), (-2, 1), (-2, 2), (-1, -2), (-1, -1), (-1, 0), (-1, 1), (0, -2), (0, -1)];
/// Adjacent neighbours count double.
const CAUSAL_WEIGHT: [u32; 11] = [3, 3, 3, 3, 3, 3, 6, 6, 6, 3, 6];
const SAMPLES: u64 = 10;
/// How much worse a continued copy may match than the best fresh sample and still win.
const COHERENCE: f32 = 2.0;
/// Openings may lose cells this far in from their edge.
const OPENING_REACH: i32 = 2;
/// Detailing reaches at most this far into the wall.
const MOST_REACH: i32 = 8;

/// Exemplar colors (`tools/backgrounds/exemplars.py`): IntGrid values 0–4, black for terrain.
const VALUE_COLORS: [[u8; 3]; 5] = [[255, 255, 255], [220, 90, 90], [90, 150, 220], [150, 150, 150], [120, 190, 120]];
const SOLID_COLOR: [u8; 3] = [0, 0, 0];

/// Bit masks over a 9x9 window (row-major, bit `y * 9 + x`).
struct WindowMasks {
    inner: u128,
    outer: u128,
    small: u128,
}

fn window_masks() -> WindowMasks {
    let mut masks = WindowMasks { inner: 0, outer: 0, small: 0 };
    for bit in 0..SIDE * SIDE {
        let ring = (bit % SIDE - RADIUS).abs().max((bit / SIDE - RADIUS).abs());
        let flag = 1u128 << bit;
        if ring <= 2 { masks.inner |= flag } else { masks.outer |= flag }
        if ring <= 1 { masks.small |= flag }
    }
    masks
}

/// What synthesis matches: walls, openings, and solid terrain around every cell.
pub(super) struct Context {
    size: UVec2,
    wall: Vec<bool>,
    solid: Vec<bool>,
    /// Chessboard distance to the other side, positive in walls and negative in openings.
    distance: Vec<i32>,
    wall_windows: Vec<u128>,
    open_windows: Vec<u128>,
}

impl Context {
    pub(super) fn new(wall: Vec<bool>, solid: Vec<bool>, size: UVec2) -> Self {
        let inside = chessboard_distance(&wall, size);
        let opening: Vec<bool> = wall.iter().map(|&covered| !covered).collect();
        let outside = chessboard_distance(&opening, size);
        let distance = wall.iter().enumerate()
            .map(|(index, &covered)| if covered { inside[index] } else { -outside[index] }).collect();
        let not_solid: Vec<bool> = solid.iter().map(|&set| !set).collect();
        Self { wall_windows: windows(&wall, size), open_windows: windows(&not_solid, size), size, wall, solid, distance }
    }

    fn clipped(&self, index: usize, reach: i32) -> i32 {
        self.distance[index].clamp(-OPENING_REACH - 1, reach + 1)
    }

    fn keys(&self, index: usize, reach: i32, masks: &WindowMasks) -> [(u128, u128, i32); 3] {
        let (wall, open, distance) = (self.wall_windows[index], self.open_windows[index], self.clipped(index, reach));
        [(wall & masks.inner, open & masks.inner, distance), (wall & masks.small, open & masks.small, distance), (0, 0, distance)]
    }
}

/// Each cell's 9x9 neighbourhood of `mask` as bits; cells beyond the edge count as set.
fn windows(mask: &[bool], size: UVec2) -> Vec<u128> {
    let (width, height) = (size.x as i32, size.y as i32);
    let at = |x: i32, y: i32| x < 0 || y < 0 || x >= width || y >= height || mask[(y * width + x) as usize];
    let rows: Vec<u16> = (0..width * height).map(|index| {
        let (x, y) = (index % width, index / width);
        (0..SIDE).fold(0, |bits, dx| bits | u16::from(at(x + dx - RADIUS, y)) << dx)
    }).collect();
    let row_bits = |x: i32, row: i32| if (0..height).contains(&row) { rows[(row * width + x) as usize] } else { (1 << SIDE) - 1 };
    (0..width * height).map(|index| {
        let (x, y) = (index % width, index / width);
        (0..SIDE).fold(0, |bits, dy| bits | u128::from(row_bits(x, y + dy - RADIUS)) << (dy * SIDE))
    }).collect()
}

/// Chessboard distance from each set cell to the nearest unset cell; zero on unset cells.
fn chessboard_distance(mask: &[bool], size: UVec2) -> Vec<i32> {
    let width = size.x as usize;
    let far = (size.x + size.y) as i32;
    let mut distance: Vec<i32> = mask.iter().map(|&set| if set { far } else { 0 }).collect();
    let neighbour = |index: usize, (dx, dy): (i32, i32)| {
        let (x, y) = ((index % width) as i32 + dx, (index / width) as i32 + dy);
        (x >= 0 && y >= 0 && x < size.x as i32 && y < size.y as i32).then(|| y as usize * width + x as usize)
    };
    let passes: [(Vec<usize>, [(i32, i32); 4]); 2] = [
        ((0..mask.len()).collect(), [(-1, 0), (0, -1), (-1, -1), (1, -1)]),
        ((0..mask.len()).rev().collect(), [(1, 0), (0, 1), (1, 1), (-1, 1)]),
    ];
    for (order, offsets) in passes {
        for index in order {
            let nearest = offsets.iter().filter_map(|&offset| neighbour(index, offset)).map(|cell| distance[cell] + 1).min();
            distance[index] = distance[index].min(nearest.unwrap_or(far));
        }
    }
    distance
}

/// The artist's near background for one biome, with the context its values answer.
pub(super) struct Exemplar {
    context: Context,
    values: Vec<i32>,
    base: i32,
    reach: i32,
    /// Exemplar cells by exact, coarse, and distance-only context key.
    pools: [HashMap<(u128, u128, i32), Vec<usize>>; 3],
    valid: Vec<bool>,
    masks: WindowMasks,
}

impl Exemplar {
    /// Reads an exemplar image and derives its context the way near openings are tidied.
    pub(super) fn from_image(image: &Image) -> Option<Self> {
        let size = image.size();
        let pixels = image.data.as_ref()?.as_chunks::<4>().0.iter().take((size.x * size.y) as usize);
        let (values, solid): (Vec<i32>, Vec<bool>) = pixels.map(decode_pixel).unzip();
        let known: Vec<bool> = values.iter().zip(&solid).map(|(&value, &set)| value >= 0 || set).collect();
        let painted: Vec<bool> = values.iter().zip(&solid).map(|(&value, &set)| value != 0 || set).collect();
        let wall = background_openings::tidy(&painted, &solid, size);
        Some(Self::new(Context::new(wall, solid, size), values, &known))
    }

    fn new(context: Context, mut values: Vec<i32>, known: &[bool]) -> Self {
        let own: Vec<bool> = known.iter().zip(&context.solid).map(|(&set, &solid)| set && !solid).collect();
        let base = most_common((0..values.len())
            .filter(|&index| own[index] && context.wall[index] && context.distance[index] > 6)
            .map(|index| values[index]));
        let mut detail: Vec<i32> = (0..values.len())
            .filter(|&index| own[index] && context.wall[index] && values[index] != base)
            .map(|index| context.distance[index]).collect();
        detail.sort_unstable();
        let reach = detail.get(detail.len() * 99 / 100).copied().unwrap_or(1).clamp(1, MOST_REACH);
        // Terrain hides the near plane; the artist's cells under it count as plain wall.
        for (value, _) in values.iter_mut().zip(&context.solid).filter(|(_, &solid)| solid) { *value = base; }
        let known_distance = chessboard_distance(known, context.size);
        let (width, height) = (context.size.x as i32, context.size.y as i32);
        let valid: Vec<bool> = (0..values.len()).map(|index| {
            let (x, y) = (index as i32 % width, index as i32 / width);
            let inside = x >= RADIUS && y >= RADIUS && x < width - RADIUS && y < height - RADIUS;
            inside && own[index] && known_distance[index] > RADIUS && (-OPENING_REACH..=reach).contains(&context.distance[index])
        }).collect();
        let masks = window_masks();
        let mut pools: [HashMap<_, Vec<usize>>; 3] = Default::default();
        let keyed = (0..values.len()).filter(|&index| valid[index])
            .flat_map(|index| context.keys(index, reach, &masks).into_iter().enumerate().map(move |(level, key)| (level, key, index)));
        for (level, key, index) in keyed {
            pools[level].entry(key).or_default().push(index);
        }
        Self { context, values, base, reach, pools, valid, masks }
    }

    fn width(&self) -> i32 {
        self.context.size.x as i32
    }

    /// The exemplar cell `offset` away from `cell`; valid cells keep their causal window inside.
    fn offset(&self, cell: usize, (dy, dx): (i32, i32)) -> usize {
        (cell as i32 + dy * self.width() + dx) as usize
    }
}

fn decode_pixel(&[red, green, blue, alpha]: &[u8; 4]) -> (i32, bool) {
    let color = [red, green, blue];
    if alpha < 128 { return (-1, false); }
    let value = VALUE_COLORS.iter().position(|&known| known == color).map_or(-1, |value| value as i32);
    (value, color == SOLID_COLOR)
}

fn most_common(values: impl Iterator<Item = i32>) -> i32 {
    let mut counts = [0usize; VALUE_COLORS.len()];
    for value in values.filter_map(|value| usize::try_from(value).ok()).filter(|&value| value < VALUE_COLORS.len()) {
        counts[value] += 1;
    }
    (0..counts.len()).max_by_key(|&value| counts[value]).unwrap_or(0) as i32
}

/// IntGrid values for the target's cells where `detail` holds: 0 in openings, the exemplar's
/// values near their edges, and the exemplar's base value deeper in the wall.
pub(super) fn synthesize(target: &Context, exemplar: &Exemplar, detail: impl Fn(usize) -> bool, seed: u64) -> Vec<i32> {
    let _timer = diagnostics::StageTimer::new("near_detail_synthesis");
    let mut synthesis = Synthesis::new(target, exemplar, seed);
    let reach = exemplar.reach;
    let band = |index: usize| !target.solid[index] && (-OPENING_REACH..=reach).contains(&target.distance[index]) && detail(index);
    let pending: Vec<usize> = (0..target.wall.len()).filter(|&index| band(index)).collect();
    for &index in &pending { synthesis.out[index] = -1; }
    for index in pending { synthesis.write(index); }
    synthesis.out
}

/// Row-order synthesis state: written values and the exemplar cell each was copied from.
struct Synthesis<'a> {
    target: &'a Context,
    exemplar: &'a Exemplar,
    seed: u64,
    out: Vec<i32>,
    source: Vec<Option<usize>>,
    candidates: Vec<(usize, bool)>,
}

impl<'a> Synthesis<'a> {
    fn new(target: &'a Context, exemplar: &'a Exemplar, seed: u64) -> Self {
        let out = target.wall.iter().map(|&covered| if covered { exemplar.base } else { 0 }).collect();
        let source = vec![None; target.wall.len()];
        Self { target, exemplar, seed, out, source, candidates: Vec::with_capacity(CAUSAL.len() + SAMPLES as usize) }
    }

    fn neighbour(&self, index: usize, (dy, dx): (i32, i32)) -> Option<usize> {
        let width = self.target.size.x as i32;
        let (x, y) = (index as i32 % width + dx, index as i32 / width + dy);
        (x >= 0 && x < width && y >= 0).then(|| (y * width + x) as usize)
    }

    fn write(&mut self, index: usize) {
        self.gather(index);
        let scored: Vec<_> = self.candidates.iter().enumerate()
            .map(|(slot, &(candidate, coherent))| (self.score(index, candidate, slot as u64), candidate, coherent)).collect();
        let best = |coherent: bool| scored.iter().filter(|entry| entry.2 == coherent)
            .min_by(|a, b| a.0.total_cmp(&b.0)).map_or((f32::INFINITY, None), |entry| (entry.0, Some(entry.1)));
        let (best_coherent, best_sample) = (best(true), best(false));
        let chosen = if best_coherent.0 <= best_sample.0 + COHERENCE { best_coherent.1 } else { best_sample.1 };
        let fallback = if self.target.wall[index] { self.exemplar.base } else { 0 };
        self.out[index] = chosen.map_or(fallback, |cell| self.exemplar.values[cell]);
        self.source[index] = chosen;
    }

    /// Continuations of the neighbours' copies, then fresh samples with the closest context key.
    fn gather(&mut self, index: usize) {
        let ex = self.exemplar;
        self.candidates.clear();
        for offset in CAUSAL {
            let copied = self.neighbour(index, offset).and_then(|cell| self.source[cell]);
            let continued = copied.map(|cell| ex.offset(cell, (-offset.0, -offset.1))).filter(|&cell| ex.valid[cell]);
            self.candidates.extend(continued.map(|cell| (cell, true)));
        }
        let keys = self.target.keys(index, ex.reach, &ex.masks);
        let Some(pool) = ex.pools.iter().zip(keys).find_map(|(pool, key)| pool.get(&key)) else { return };
        let samples = (0..SAMPLES).map(|sample| {
            let hash = splitmix64(self.seed ^ (index as u64).wrapping_mul(0x9E37_79B9) ^ sample << 56);
            (pool[(hash % pool.len() as u64) as usize], false)
        });
        self.candidates.extend(samples);
    }

    /// Context bits, edge distance, and already-written neighbours that differ, plus a small
    /// random share that breaks ties.
    fn score(&self, index: usize, candidate: usize, slot: u64) -> f32 {
        let (ex, target, masks) = (self.exemplar, self.target, &self.exemplar.masks);
        let differs = |a: u128, b: u128| 2 * ((a ^ b) & masks.inner).count_ones() + ((a ^ b) & masks.outer).count_ones();
        let mut total = differs(ex.context.wall_windows[candidate], target.wall_windows[index])
            + differs(ex.context.open_windows[candidate], target.open_windows[index])
            + ex.context.clipped(candidate, ex.reach).abs_diff(target.clipped(index, ex.reach));
        total += CAUSAL.into_iter().zip(CAUSAL_WEIGHT).filter(|&(offset, _)| {
            let written = self.neighbour(index, offset).map_or(ex.base, |cell| self.out[cell]);
            written >= 0 && ex.values[ex.offset(candidate, offset)] != written
        }).map(|(_, weight)| weight).sum::<u32>();
        let noise = splitmix64(self.seed ^ slot ^ (index as u64) << 20) >> 41;
        total as f32 + noise as f32 / (1u64 << 23) as f32 * 0.5
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ICE: &str = "assets/levels/tiles/ice_biome/near_bg/exemplar.png";
    const TUNDRA: &str = "assets/levels/tiles/tundra_biome/near_bg/exemplar.png";

    /// The artist's exemplar, its terrain (cells outside the painted area count as terrain), and
    /// its decoded values.
    fn artist(path: &str) -> (Exemplar, Vec<bool>, Vec<i32>, UVec2) {
        let image = mapgen::load_png(Path::new(path)).unwrap();
        let (values, solid): (Vec<i32>, Vec<bool>) = image.data.as_ref().unwrap().as_chunks::<4>().0.iter().map(decode_pixel).unzip();
        let terrain = values.iter().zip(&solid).map(|(&value, &set)| set || value < 0).collect();
        (Exemplar::from_image(&image).unwrap(), terrain, values, image.size())
    }

    #[test]
    fn near_detailing_copies_the_artist_values_around_openings_only() {
        for path in [ICE, TUNDRA] {
            let (exemplar, terrain, values, size) = artist(path);
            let wall = background_openings::near_wall(&terrain, size, 7);
            let target = Context::new(wall.clone(), terrain.clone(), size);
            let grid = synthesize(&target, &exemplar, |_| true, 11);
            assert_eq!(grid, synthesize(&target, &exemplar, |_| true, 11), "{path}: a seed reproduces its detailing");
            assert!(grid.iter().all(|value| values.contains(value)), "{path}: only the artist's values");
            let deep_opening = |index: usize| target.distance[index] < -OPENING_REACH;
            let deep_wall = |index: usize| target.distance[index] > exemplar.reach || terrain[index];
            assert!((0..grid.len()).filter(|&index| deep_opening(index)).all(|index| grid[index] == 0), "{path}");
            assert!((0..grid.len()).filter(|&index| deep_wall(index)).all(|index| grid[index] == exemplar.base), "{path}");
            let detail = grid.iter().zip(&terrain).filter(|&(&value, &solid)| !solid && value != 0 && value != exemplar.base).count();
            assert!(detail > 0, "{path}: openings receive detailing");
        }
    }

    /// Renders the artist's near background and this generator's on the artist's own terrain:
    /// `SEEKER_NEAR_FIXTURE_OUTPUT=/absolute/directory cargo test near_background_on_artist_terrain`.
    #[test]
    fn near_background_on_artist_terrain() {
        let Ok(directory) = std::env::var("SEEKER_NEAR_FIXTURE_OUTPUT") else { return };
        let mut images = Assets::default();
        let rules = ldtk_rules::load_test_artwork(&mut images);
        let (exemplar, terrain, values, size) = artist(ICE);
        let level = CompiledLevel { width: size.x, height: size.y, terrain: terrain.clone(), ..default() };
        let target = Context::new(background_openings::near_wall(&terrain, size, 3), terrain.clone(), size);
        let generated = synthesize(&target, &exemplar, |_| true, 5);
        let authored: Vec<i32> = values.iter().zip(&terrain).map(|(&value, &solid)| if solid { exemplar.base } else { value.max(0) }).collect();
        fs::create_dir_all(&directory).unwrap();
        for (name, grid) in [("artist", authored), ("generated", generated)] {
            let mask: Vec<bool> = grid.iter().map(|&value| value != 0).collect();
            let surface = background::prepare_background_surface(&level, &grid, &mask, None, &rules, &images).unwrap();
            let mut image = surface.bake(URect { min: UVec2::ZERO, max: size });
            shade_terrain(&mut image, &terrain, size);
            image.try_into_dynamic().unwrap().save(Path::new(&directory).join(format!("near-{name}.png"))).unwrap();
        }
    }

    fn shade_terrain(image: &mut Image, terrain: &[bool], size: UVec2) {
        let width = image.width() as usize;
        let bytes = image.data.as_mut().unwrap();
        for (index, _) in terrain.iter().enumerate().filter(|&(_, &solid)| solid) {
            let (x, y) = (index % size.x as usize * 6, index / size.x as usize * 6);
            let pixels = (0..36).map(|pixel| ((y + pixel / 6) * width + x + pixel % 6) * 4);
            pixels.for_each(|offset| bytes[offset..offset + 4].copy_from_slice(&[18, 20, 28, 255]));
        }
    }
}
