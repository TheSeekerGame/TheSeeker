use super::*;
use crate::camera::{GameplayPixelScale, MainCamera};
use crate::game::combat::PlanetImmunityLevel;
use crate::game::actors::catalog::{ActorCatalog, ResolvedActor};
use theseeker_engine::physics::TileCollisionWorld;
use crate::levelgen::{
    mapgen, BiomeKind, CompiledLevel, pipeline::GenerationBuild, LevelBiomeMask,
    LevelChunkOccupancyMask, SpawnKind,
};
use std::array;
use std::collections::{HashMap, HashSet};

const DIRECTOR_INTERVAL_TICKS: u64 = 16;
const RESPAWN_SECONDS: f64 = 5.0;
const ACTIVE_CHUNK_RADIUS: i32 = crate::levelgen::PLAYER_ACTIVE_CHUNK_RADIUS;
const DEACTIVATE_CHUNK_RADIUS: i32 = 3;
const MIN_SPAWN_DISTANCE_CHUNKS: f32 = 2.0;
const STREAM_DESPAWN_DISTANCE: f32 = 1280.0;
const OFFSCREEN_MARGIN: f32 = 24.0;
const PROTECTED_ANCHOR_RADIUS_TILES: i32 = 12;
const MIN_CANDIDATE_DISTANCE_TILES: i32 = 2;
const MIN_TIER_THREE_DEPTH: u32 = 4;
const TIER_THREE_PERCENT: u32 = 15;
const DEEP_ENEMY_DEPTH: u32 = 6;
const VOID_SLOT: usize = 0;

#[derive(Component, Clone, Copy, Debug)]
pub(super) struct SpawnDirectorOwned {
    region: IVec2,
    slot: usize,
    candidate_id: u32,
}

#[derive(Component, Reflect, Copy, Clone)]
pub(super) enum Role {
    Melee,
    Ranged,
}

#[derive(Component, Debug, Copy, Clone, PartialEq, Eq, Default)]
pub enum SpiderBiome {
    #[default]
    Ice,
    Tundra,
}

#[derive(Component, Default, Debug, Reflect, Clone, Copy, PartialEq, Eq)]
pub enum Tier {
    #[default]
    Base = 1,
    Two = 3,
    Three = 9,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum SpawnHabitat {
    Air,
    GroundSmall,
    GroundLarge,
    GroundTall,
}

impl SpawnHabitat {
    const ALL: [Self; 4] = [Self::Air, Self::GroundSmall, Self::GroundLarge, Self::GroundTall];

    const fn index(self) -> usize {
        match self {
            Self::Air => 0,
            Self::GroundSmall => 1,
            Self::GroundLarge => 2,
            Self::GroundTall => 3,
        }
    }

    const fn thinning_cell(self) -> IVec2 {
        match self {
            Self::Air => IVec2::new(8, 8),
            Self::GroundSmall => IVec2::new(6, 4),
            Self::GroundLarge | Self::GroundTall => IVec2::new(8, 4),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct HabitatMask(u8);

impl HabitatMask {
    fn insert(&mut self, habitat: SpawnHabitat) {
        self.0 |= 1 << habitat.index();
    }

    const fn contains(self, habitat: SpawnHabitat) -> bool {
        self.0 & (1 << habitat.index()) != 0
    }

    const fn is_empty(self) -> bool {
        self.0 == 0
    }
}

#[derive(Clone, Copy, Debug)]
struct SpawnCandidate {
    id: u32,
    tile: IVec2,
    world_position: Vec2,
    biome: BiomeKind,
    habitats: HabitatMask,
}

#[derive(Default)]
struct RegionCatalog {
    depth: u32,
    candidates: Vec<SpawnCandidate>,
}

#[derive(Resource)]
pub(super) struct EnemySpawnCatalog {
    tile_size: f32,
    width: u32,
    height: u32,
    regions: HashMap<IVec2, RegionCatalog>,
}

#[derive(Default)]
struct DirectorSlot {
    actor: Option<Entity>,
    candidate_id: Option<u32>,
    cooldown_ticks: u32,
    generation: u32,
    initial_distance_exemption: bool,
}

#[derive(Default)]
struct DirectorRegion {
    slots: Vec<DirectorSlot>,
    initialized: bool,
}

#[derive(Resource, Default)]
pub(super) struct EnemySpawnRuntime {
    regions: HashMap<IVec2, DirectorRegion>,
    leased_candidates: HashSet<u32>,
    initial_player_region: Option<IVec2>,
}

#[derive(Clone, Copy)]
struct GroundProfile {
    support_width: i32,
    side_clearance: i32,
    headroom: i32,
}

const GROUND_SMALL_PROFILE: GroundProfile = GroundProfile {
    support_width: 3,
    side_clearance: 1,
    headroom: 1,
};
const GROUND_LARGE_PROFILE: GroundProfile = GroundProfile {
    support_width: 3,
    side_clearance: 1,
    headroom: 2,
};
const GROUND_TALL_PROFILE: GroundProfile = GroundProfile {
    support_width: 3,
    side_clearance: 1,
    headroom: 5,
};

struct SummedAreaTable {
    width: usize,
    height: usize,
    sums: Vec<u32>,
}

impl SummedAreaTable {
    #[expect(
        clippy::excessive_nesting,
        reason = "A summed-area table is constructed by a fixed two-dimensional scan."
    )]
    fn new(width: usize, height: usize, cells: &[bool]) -> Self {
        let stride = width + 1;
        let mut sums = vec![0; stride * (height + 1)];
        for y in 0..height {
            let mut row_sum = 0u32;
            for x in 0..width {
                row_sum += u32::from(cells[y * width + x]);
                let dst = (y + 1) * stride + x + 1;
                sums[dst] = sums[y * stride + x + 1] + row_sum;
            }
        }
        Self {
            width,
            height,
            sums,
        }
    }

    fn count_inclusive(
        &self,
        min_x: i32,
        min_y: i32,
        max_x: i32,
        max_y: i32,
    ) -> Option<u32> {
        if min_x < 0
            || min_y < 0
            || max_x < min_x
            || max_y < min_y
            || max_x >= self.width as i32
            || max_y >= self.height as i32
        {
            return None;
        }

        let stride = self.width + 1;
        let x0 = min_x as usize;
        let y0 = min_y as usize;
        let x1 = max_x as usize + 1;
        let y1 = max_y as usize + 1;
        Some(
            self.sums[y1 * stride + x1]
                + self.sums[y0 * stride + x0]
                - self.sums[y0 * stride + x1]
                - self.sums[y1 * stride + x0],
        )
    }

    fn rect_is_empty(
        &self,
        min_x: i32,
        min_y: i32,
        max_x: i32,
        max_y: i32,
    ) -> bool {
        self.count_inclusive(min_x, min_y, max_x, max_y) == Some(0)
    }
}

struct SpawnCatalogGrid {
    width: usize,
    height: usize,
    tile_size: f32,
    blocked: SummedAreaTable,
    safe_ground: SummedAreaTable,
    protected: Vec<bool>,
    biomes: Vec<BiomeKind>,
}

impl SpawnCatalogGrid {
    fn index(&self, x: i32, y: i32) -> Option<usize> {
        if x < 0 || y < 0 || x >= self.width as i32 || y >= self.height as i32
        {
            return None;
        }
        Some(y as usize * self.width + x as usize)
    }

    fn is_protected(&self, x: i32, y: i32) -> bool {
        self.index(x, y)
            .and_then(|index| self.protected.get(index))
            .copied()
            .unwrap_or(true)
    }

    fn biome_at(&self, x: i32, y: i32) -> BiomeKind {
        self.index(x, y)
            .and_then(|index| self.biomes.get(index))
            .copied()
            .unwrap_or_default()
    }

    fn world_position(&self, x: i32, y: i32) -> Vec2 {
        Vec2::new(
            (x as f32 + 0.5) * self.tile_size,
            (self.height as f32 - y as f32 - 0.5) * self.tile_size,
        )
    }
}

#[derive(Clone, Copy)]
struct RawCandidate {
    tile: IVec2,
    habitats: HabitatMask,
    hash: u64,
}

#[expect(
    clippy::too_many_arguments,
    reason = "Catalog construction reads independent finalized level resources."
)]
pub(super) fn build_enemy_spawn_catalog(
    mut commands: Commands,
    existing: Option<Res<EnemySpawnCatalog>>,
    compiled: Option<Res<CompiledLevel>>,
    biomes: Option<Res<LevelBiomeMask>>,
    occupancy: Option<Res<LevelChunkOccupancyMask>>,
    ground_ready: Option<Res<GenerationBuild>>,
    collision_world: Res<TileCollisionWorld>,
    level_seed: Res<crate::game::actors::LevelSeed>,
) {
    if existing.is_some()
        || !ground_ready.as_ref().is_some_and(|ready| ready.ground_ready())
        || !collision_world.is_ready()
    {
        return;
    }
    let (Some(compiled), Some(biomes)) = (compiled, biomes) else {
        return;
    };
    if !catalog_inputs_match(&compiled, &biomes, &collision_world) {
        return;
    }

    let Some(grid) = build_catalog_grid(
        &compiled,
        &biomes,
        occupancy.as_deref(),
        &collision_world,
    ) else {
        return;
    };
    let Some(player_chunk) = player_spawn_chunk(&compiled) else {
        warn!("Enemy spawn catalog requires a player spawn marker");
        return;
    };

    let raw_candidates = collect_raw_candidates(&grid, player_chunk, level_seed.0);
    let candidates = thin_candidates(raw_candidates);
    let catalog = group_candidates_by_region(
        candidates,
        &grid,
        player_chunk,
        compiled.width,
        compiled.height,
    );
    let candidate_count = catalog
        .regions
        .values()
        .map(|region| region.candidates.len())
        .sum::<usize>();
    info!(
        "Built enemy spawn catalog: regions={}, candidates={candidate_count}",
        catalog.regions.len()
    );
    commands.insert_resource(catalog);
}

fn catalog_inputs_match(
    compiled: &CompiledLevel,
    biomes: &LevelBiomeMask,
    collision_world: &TileCollisionWorld,
) -> bool {
    compiled.width == collision_world.width
        && compiled.height == collision_world.height
        && compiled.terrain.len() == (compiled.width * compiled.height) as usize
        && biomes.matches_dims(compiled.width, compiled.height)
}

fn build_catalog_grid(
    compiled: &CompiledLevel,
    biomes: &LevelBiomeMask,
    occupancy: Option<&LevelChunkOccupancyMask>,
    collision_world: &TileCollisionWorld,
) -> Option<SpawnCatalogGrid> {
    let width = compiled.width as usize;
    let height = compiled.height as usize;
    let len = width.checked_mul(height)?;
    if compiled.spike_tiles.len() != len {
        warn!("Enemy spawn catalog terrain masks have mismatched dimensions");
        return None;
    }

    let occupancy = match occupancy {
        Some(mask) if occupancy_mask_matches(mask, compiled.width, compiled.height) => {
            Some(mask)
        },
        Some(_) => {
            warn!("Enemy spawn catalog chunk occupancy dimensions do not match the level");
            return None;
        },
        None => None,
    };
    let mut blocked = vec![false; len];
    let mut safe_ground = vec![false; len];
    let accessible = crate::levelgen::placement_area(compiled, occupancy);
    for y in 0..height {
        for x in 0..width {
            let index = y * width + x;
            let occupied = tile_is_in_occupied_chunk(occupancy, x as u32, y as u32);
            let solid = collision_world.tile_solid_info(x as i32, y as i32);
            let has_solid = solid.is_some();
            let terrain = solid.is_some_and(|info| info.terrain);
            blocked[index] = !occupied || !accessible[index] || has_solid || compiled.spike_tiles[index];
            safe_ground[index] = occupied
                && accessible[index]
                && terrain
                && !compiled.spike_tiles[index];
        }
    }

    let mut protected = vec![false; len];
    for spawn in &compiled.spawns {
        mark_protected_world_position(
            &mut protected,
            width,
            height,
            collision_world.tile_size,
            spawn.position,
        );
    }

    Some(SpawnCatalogGrid {
        width,
        height,
        tile_size: collision_world.tile_size,
        blocked: SummedAreaTable::new(width, height, &blocked),
        safe_ground: SummedAreaTable::new(width, height, &safe_ground),
        protected,
        biomes: biomes.cells.clone(),
    })
}

fn occupancy_mask_matches(
    occupancy: &LevelChunkOccupancyMask,
    width: u32,
    height: u32,
) -> bool {
    let chunks_w = width.div_ceil(mapgen::CHUNK_IMAGE_SIZE);
    let chunks_h = height.div_ceil(mapgen::CHUNK_IMAGE_SIZE);
    occupancy.matches_chunks(chunks_w, chunks_h)
}

fn tile_is_in_occupied_chunk(
    occupancy: Option<&LevelChunkOccupancyMask>,
    tile_x: u32,
    tile_y: u32,
) -> bool {
    let Some(mask) = occupancy else {
        return true;
    };
    let chunk_x = tile_x / mapgen::CHUNK_IMAGE_SIZE;
    let chunk_y = tile_y / mapgen::CHUNK_IMAGE_SIZE;
    if chunk_x >= mask.chunks_w || chunk_y >= mask.chunks_h {
        return false;
    }
    mask.occupied[(chunk_y * mask.chunks_w + chunk_x) as usize]
}

#[expect(
    clippy::excessive_nesting,
    reason = "Protected anchor masks are filled by a bounded square tile scan."
)]
fn mark_protected_world_position(
    protected: &mut [bool],
    width: usize,
    height: usize,
    tile_size: f32,
    position: Vec2,
) {
    let center_x = (position.x / tile_size).floor() as i32;
    let center_y = (height as f32 - 1.0 - (position.y / tile_size).floor()) as i32;
    for y in (center_y - PROTECTED_ANCHOR_RADIUS_TILES)
        ..=(center_y + PROTECTED_ANCHOR_RADIUS_TILES)
    {
        for x in (center_x - PROTECTED_ANCHOR_RADIUS_TILES)
            ..=(center_x + PROTECTED_ANCHOR_RADIUS_TILES)
        {
            if x < 0 || y < 0 || x >= width as i32 || y >= height as i32 {
                continue;
            }
            protected[y as usize * width + x as usize] = true;
        }
    }
}

fn player_spawn_chunk(compiled: &CompiledLevel) -> Option<IVec2> {
    compiled
        .spawns
        .iter()
        .find(|spawn| spawn.kind == SpawnKind::Player)
        .map(|spawn| spawn.chunk)
}

#[expect(
    clippy::excessive_nesting,
    reason = "Candidate discovery performs one bounded pass over the level tile grid."
)]
fn collect_raw_candidates(
    grid: &SpawnCatalogGrid,
    player_chunk: IVec2,
    level_seed: u32,
) -> Vec<RawCandidate> {
    let mut candidates = Vec::new();
    for y in 0..grid.height as i32 {
        for x in 0..grid.width as i32 {
            let chunk = tile_chunk(x, y);
            if chunk == player_chunk || grid.is_protected(x, y) {
                continue;
            }
            let habitats = candidate_habitats(grid, x, y);
            if habitats.is_empty() {
                continue;
            }
            candidates.push(RawCandidate {
                tile: IVec2::new(x, y),
                habitats,
                hash: position_hash(level_seed, x, y, 0),
            });
        }
    }
    candidates
}

fn candidate_habitats(grid: &SpawnCatalogGrid, x: i32, y: i32) -> HabitatMask {
    let mut habitats = HabitatMask::default();
    let radius = air_clearance_radius(grid.tile_size);
    if grid.blocked.rect_is_empty(x - radius, y - radius, x + radius, y + radius) {
        habitats.insert(SpawnHabitat::Air);
    }
    if ground_candidate_is_valid(grid, x, y, GROUND_SMALL_PROFILE) {
        habitats.insert(SpawnHabitat::GroundSmall);
    }
    if ground_candidate_is_valid(grid, x, y, GROUND_LARGE_PROFILE) {
        habitats.insert(SpawnHabitat::GroundLarge);
    }
    if ground_candidate_is_valid(grid, x, y, GROUND_TALL_PROFILE) {
        habitats.insert(SpawnHabitat::GroundTall);
    }
    habitats
}

fn air_clearance_radius(tile_size: f32) -> i32 {
    (8.0 / tile_size - 0.5).ceil().max(0.0) as i32
}

fn ground_candidate_is_valid(
    grid: &SpawnCatalogGrid,
    x: i32,
    y: i32,
    profile: GroundProfile,
) -> bool {
    if !grid.blocked.rect_is_empty(
        x - profile.side_clearance,
        y - profile.headroom,
        x + profile.side_clearance,
        y,
    ) {
        return false;
    }

    let support_y = y + 1;
    let support_left = (profile.support_width - 1) / 2;
    let support_right = profile.support_width - 1 - support_left;
    grid.safe_ground.count_inclusive(
        x - support_left,
        support_y,
        x + support_right,
        support_y,
    ) == Some(profile.support_width as u32)
}

fn thin_candidates(mut raw: Vec<RawCandidate>) -> Vec<RawCandidate> {
    raw.sort_unstable_by_key(|candidate| {
        (candidate.habitats.contains(SpawnHabitat::Air), candidate.hash, candidate.tile.y, candidate.tile.x)
    });
    let mut occupied_profile_cells: [HashSet<IVec2>; 4] =
        array::from_fn(|_| HashSet::new());
    let mut proximity_cells: HashMap<IVec2, Vec<IVec2>> = HashMap::new();
    let mut accepted = Vec::new();

    for mut candidate in raw {
        candidate.habitats = available_habitats(candidate, &occupied_profile_cells);
        if candidate.habitats.is_empty()
            || candidate_is_too_close(candidate.tile, &proximity_cells)
        {
            continue;
        }

        mark_profile_cells(candidate, &mut occupied_profile_cells);
        proximity_cells
            .entry(proximity_cell(candidate.tile))
            .or_default()
            .push(candidate.tile);
        accepted.push(candidate);
    }
    accepted
}

fn mark_profile_cells(
    candidate: RawCandidate,
    occupied_profile_cells: &mut [HashSet<IVec2>; 4],
) {
    for habitat in SpawnHabitat::ALL {
        if candidate.habitats.contains(habitat) {
            occupied_profile_cells[habitat.index()]
                .insert(profile_cell(candidate.tile, habitat));
        }
    }
}

fn available_habitats(
    candidate: RawCandidate,
    occupied_profile_cells: &[HashSet<IVec2>; 4],
) -> HabitatMask {
    let mut available = HabitatMask::default();
    for habitat in SpawnHabitat::ALL {
        if candidate.habitats.contains(habitat)
            && !occupied_profile_cells[habitat.index()]
                .contains(&profile_cell(candidate.tile, habitat))
        {
            available.insert(habitat);
        }
    }
    available
}

fn profile_cell(tile: IVec2, habitat: SpawnHabitat) -> IVec2 {
    let cell = habitat.thinning_cell();
    IVec2::new(tile.x.div_euclid(cell.x), tile.y.div_euclid(cell.y))
}

fn proximity_cell(tile: IVec2) -> IVec2 {
    IVec2::new(
        tile.x.div_euclid(MIN_CANDIDATE_DISTANCE_TILES),
        tile.y.div_euclid(MIN_CANDIDATE_DISTANCE_TILES),
    )
}

fn candidate_is_too_close(
    tile: IVec2,
    proximity_cells: &HashMap<IVec2, Vec<IVec2>>,
) -> bool {
    let center = proximity_cell(tile);
    (-1..=1)
        .flat_map(|offset_y| {
            (-1..=1).map(move |offset_x| {
                center + IVec2::new(offset_x, offset_y)
            })
        })
        .filter_map(|cell| proximity_cells.get(&cell))
        .flatten()
        .any(|neighbor| {
            tile.distance_squared(*neighbor)
                < MIN_CANDIDATE_DISTANCE_TILES.pow(2)
        })
}

fn group_candidates_by_region(
    candidates: Vec<RawCandidate>,
    grid: &SpawnCatalogGrid,
    player_chunk: IVec2,
    width: u32,
    height: u32,
) -> EnemySpawnCatalog {
    let mut regions: HashMap<IVec2, RegionCatalog> = HashMap::new();
    for (id, raw) in candidates.into_iter().enumerate() {
        let region_key = tile_chunk(raw.tile.x, raw.tile.y);
        let biome_y = if raw.habitats.contains(SpawnHabitat::GroundSmall)
            || raw.habitats.contains(SpawnHabitat::GroundLarge)
            || raw.habitats.contains(SpawnHabitat::GroundTall)
        {
            raw.tile.y + 1
        } else {
            raw.tile.y
        };
        let region = regions.entry(region_key).or_insert_with(|| RegionCatalog {
            depth: region_key.y.saturating_sub(player_chunk.y).max(0) as u32,
            candidates: Vec::new(),
        });
        region.candidates.push(SpawnCandidate {
            id: u32::try_from(id).unwrap_or(u32::MAX),
            tile: raw.tile,
            world_position: grid.world_position(raw.tile.x, raw.tile.y),
            biome: grid.biome_at(raw.tile.x, biome_y),
            habitats: raw.habitats,
        });
    }
    EnemySpawnCatalog {
        tile_size: grid.tile_size,
        width,
        height,
        regions,
    }
}

fn tile_chunk(tile_x: i32, tile_y: i32) -> IVec2 {
    let chunk_size = mapgen::CHUNK_IMAGE_SIZE as i32;
    IVec2::new(
        tile_x.div_euclid(chunk_size),
        tile_y.div_euclid(chunk_size),
    )
}

fn position_hash(seed: u32, x: i32, y: i32, salt: u32) -> u64 {
    let mut value = u64::from(seed)
        ^ (x as u32 as u64).wrapping_mul(0x9E37_79B1)
        ^ (y as u32 as u64).wrapping_mul(0x85EB_CA77)
        ^ u64::from(salt).wrapping_mul(0xC2B2_AE3D);
    value ^= value >> 30;
    value = value.wrapping_mul(0xBF58_476D_1CE4_E5B9);
    value ^= value >> 27;
    value = value.wrapping_mul(0x94D0_49BB_1331_11EB);
    value ^ (value >> 31)
}

#[expect(
    clippy::too_many_arguments,
    reason = "The director system consumes independent world, progression, and camera state."
)]
pub(super) fn run_enemy_spawn_director(
    mut commands: Commands,
    catalog: Option<Res<EnemySpawnCatalog>>,
    compiled: Option<Res<CompiledLevel>>,
    collision: Res<TileCollisionWorld>,
    actors: Res<ActorCatalog>,
    mut runtime: ResMut<EnemySpawnRuntime>,
    game_time: Res<GameTime>,
    level_seed: Res<crate::game::actors::LevelSeed>,
    kill_count: Res<KillCount>,
    pixel_scale: Res<GameplayPixelScale>,
    player_q: Query<&Transform, (With<Player>, Without<Dead>)>,
    camera_q: Query<&GlobalTransform, With<MainCamera>>,
    actor_q: Query<
        (&Transform, Option<&Gent>, Option<&Dead>),
        With<SpawnDirectorOwned>,
    >,
) {
    if !game_time.tick().is_multiple_of(DIRECTOR_INTERVAL_TICKS) {
        return;
    }
    let (Some(catalog), Some(compiled)) = (catalog, compiled) else {
        return;
    };
    let Ok(player_transform) = player_q.single() else {
        return;
    };
    let Ok(camera_transform) = camera_q.single() else {
        return;
    };
    let visible_size = pixel_scale.visible_game_size();
    if visible_size.x <= 0.0 || visible_size.y <= 0.0 {
        return;
    }

    let player_position = player_transform.translation.xy();
    let player_region = world_region(&catalog, player_position);
    let initial_player_region =
        *runtime.initial_player_region.get_or_insert(player_region);
    let camera_position = camera_transform.translation().xy();
    let view_half_size = visible_size * 0.5;
    update_existing_spawn_slots(
        &mut commands,
        &mut runtime,
        &actor_q,
        player_region,
        player_position,
        camera_position,
        view_half_size,
    );

    let immunity = PlanetImmunityLevel::from_kills(**kill_count).get();
    let active_regions = catalog
        .regions
        .iter()
        .filter(|(region, _)| {
            chunk_distance(**region, player_region) <= ACTIVE_CHUNK_RADIUS
        })
        .map(|(region, data)| (*region, data.depth))
        .collect::<Vec<_>>();

    for (region_key, depth) in active_regions {
        let target = target_population(immunity, depth);
        let initial_region =
            chunk_distance(region_key, initial_player_region) <= ACTIVE_CHUNK_RADIUS;
        let slots = prepare_region_for_spawn(&mut runtime, region_key, target,
            initial_region, immunity == PlanetImmunityLevel::MAX);
        let context = SpawnContext {
            catalog: &catalog, compiled: &compiled, collision: &collision, actors: &actors,
            leased_candidates: &runtime.leased_candidates, level_seed: level_seed.0,
            player_position, camera_position, view_half_size,
        };
        let Some((slot_index, selection)) = select_ready_spawn(&context, &runtime, region_key, slots) else { continue; };
        let entity = spawn_selection(&mut commands, selection, depth, level_seed.0);
        commands.entity(entity).insert(SpawnDirectorOwned {
            region: region_key,
            slot: slot_index,
            candidate_id: selection.candidate.id,
        });
        runtime.leased_candidates.insert(selection.candidate.id);
        let region = runtime.regions.get_mut(&region_key).expect("region exists");
        let slot = &mut region.slots[slot_index];
        slot.actor = Some(entity);
        slot.candidate_id = Some(selection.candidate.id);
        slot.generation = slot.generation.wrapping_add(1);
    }
}

fn update_existing_spawn_slots(
    commands: &mut Commands,
    runtime: &mut EnemySpawnRuntime,
    actor_q: &Query<
        (&Transform, Option<&Gent>, Option<&Dead>),
        With<SpawnDirectorOwned>,
    >,
    player_region: IVec2,
    player_position: Vec2,
    camera_position: Vec2,
    view_half_size: Vec2,
) {
    let EnemySpawnRuntime {
        regions,
        leased_candidates,
        ..
    } = runtime;
    for (region_key, region) in regions.iter_mut() {
        let outside_stream =
            chunk_distance(*region_key, player_region) > DEACTIVATE_CHUNK_RADIUS;
        let stream = StreamContext {
            outside_stream,
            player_position,
            camera_position,
            view_half_size,
        };
        update_region_slots(
            commands,
            region,
            leased_candidates,
            actor_q,
            stream,
        );
    }
}

#[derive(Clone, Copy)]
struct StreamContext {
    outside_stream: bool,
    player_position: Vec2,
    camera_position: Vec2,
    view_half_size: Vec2,
}

fn update_region_slots(
    commands: &mut Commands,
    region: &mut DirectorRegion,
    leased_candidates: &mut HashSet<u32>,
    actor_q: &Query<
        (&Transform, Option<&Gent>, Option<&Dead>),
        With<SpawnDirectorOwned>,
    >,
    stream: StreamContext,
) {
    for slot in &mut region.slots {
        update_spawn_slot(commands, slot, leased_candidates, actor_q, stream);
    }
}

fn update_spawn_slot(
    commands: &mut Commands,
    slot: &mut DirectorSlot,
    leased_candidates: &mut HashSet<u32>,
    actor_q: &Query<
        (&Transform, Option<&Gent>, Option<&Dead>),
        With<SpawnDirectorOwned>,
    >,
    stream: StreamContext,
) {
    slot.cooldown_ticks = slot
        .cooldown_ticks
        .saturating_sub(DIRECTOR_INTERVAL_TICKS as u32);
    let Some(actor) = slot.actor else { return; };
    let Ok((transform, gent, dead)) = actor_q.get(actor) else {
        release_slot(slot, leased_candidates, 0);
        return;
    };
    if dead.is_some() {
        return;
    }
    let actor_position = transform.translation.xy();
    let may_stream_out = stream.outside_stream
        && actor_position.distance(stream.player_position)
            >= STREAM_DESPAWN_DISTANCE
        && position_is_offscreen(
            actor_position,
            stream.camera_position,
            stream.view_half_size,
        );
    if !may_stream_out {
        return;
    }

    if let Some(gent) = gent {
        despawn_enemy_with_gfx(commands, actor, gent);
    } else {
        despawn_entity_if_exists(commands, actor);
    }
    release_slot(slot, leased_candidates, 0);
}

fn release_slot(
    slot: &mut DirectorSlot,
    leased_candidates: &mut HashSet<u32>,
    cooldown_ticks: u32,
) {
    if let Some(candidate_id) = slot.candidate_id.take() {
        leased_candidates.remove(&candidate_id);
    }
    slot.actor = None;
    slot.cooldown_ticks = cooldown_ticks;
}

fn prepare_region_for_spawn(
    runtime: &mut EnemySpawnRuntime,
    region_key: IVec2,
    target: usize,
    initial_region: bool,
    escalation: bool,
) -> [Option<(usize, bool)>; 2] {
    let region = runtime.regions.entry(region_key).or_default();
    let initializing_region = !region.initialized;
    region.initialized = true;
    // Population contraction cannot discard living actors' leases or their cleanup indexes.
    let required = (target + usize::from(!escalation)).max(region.slots.len());
    region.slots.resize_with(required, || DirectorSlot {
        initial_distance_exemption: initializing_region && initial_region, ..default()
    });
    region.slots[VOID_SLOT].initial_distance_exemption = false;
    if region.slots.iter().filter(|slot| slot.actor.is_some()).count() >= target {
        return [None, None];
    }
    let void = &region.slots[VOID_SLOT];
    let void_slot = (escalation && void.actor.is_none() && void.cooldown_ticks == 0)
        .then_some((VOID_SLOT, false));
    let ordinary = first_ready_ordinary_slot(region, target - usize::from(escalation)).map(|slot| {
        let distance_exempt = std::mem::take(&mut region.slots[slot].initial_distance_exemption);
        (slot, distance_exempt)
    });
    [void_slot, ordinary]
}

fn first_ready_ordinary_slot(region: &DirectorRegion, limit: usize) -> Option<usize> {
    let ready = |(_, slot): &(usize, &DirectorSlot)| slot.actor.is_none() && slot.cooldown_ticks == 0;
    region.slots.iter().enumerate().skip(1).take(limit)
        .find(|slot| ready(slot) && slot.1.initial_distance_exemption)
        .or_else(|| region.slots.iter().enumerate().skip(1).take(limit).find(ready))
        .map(|(index, _)| index)
}

fn world_region(catalog: &EnemySpawnCatalog, position: Vec2) -> IVec2 {
    let tile_x = (position.x / catalog.tile_size).floor() as i32;
    let tile_y = (catalog.height as f32
        - 1.0
        - (position.y / catalog.tile_size).floor()) as i32;
    let bounded_x = tile_x.clamp(0, catalog.width.saturating_sub(1) as i32);
    let bounded_y = tile_y.clamp(0, catalog.height.saturating_sub(1) as i32);
    tile_chunk(bounded_x, bounded_y)
}

fn chunk_distance(a: IVec2, b: IVec2) -> i32 {
    (a - b).abs().max_element()
}

fn target_population(immunity: u8, depth: u32) -> usize {
    let immunity_increase = u32::from(immunity.saturating_sub(1)) / 2;
    let depth_increase = (depth / 2).min(2);
    let ordinary = (3 + immunity_increase + depth_increase).min(7) as usize;
    ordinary + usize::from(immunity == PlanetImmunityLevel::MAX)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EnemyFamily {
    BatSmall, BatMedium, BatBig, BatHuge, SpiderSmall, SpiderLarge,
    Buzzer, Squid, Opi, Void,
}

impl EnemyFamily {
    const fn habitat(self) -> SpawnHabitat {
        match self {
            Self::BatSmall | Self::BatMedium | Self::BatBig | Self::BatHuge
                | Self::Buzzer | Self::Void => SpawnHabitat::Air,
            Self::SpiderSmall | Self::Squid => SpawnHabitat::GroundSmall,
            Self::SpiderLarge => SpawnHabitat::GroundLarge,
            Self::Opi => SpawnHabitat::GroundTall,
        }
    }
}

#[derive(Clone, Copy)]
struct WeightedFamily {
    family: EnemyFamily,
    weight: u32,
    min_depth: u32,
    tundra_only: bool,
}

// Weights describe families, independently of how many air or ground sites exist.
const ROSTER: &[WeightedFamily] = &[
    WeightedFamily { family: EnemyFamily::BatSmall, weight: 25, min_depth: 0, tundra_only: false },
    WeightedFamily { family: EnemyFamily::BatMedium, weight: 20, min_depth: 1, tundra_only: false },
    WeightedFamily { family: EnemyFamily::BatBig, weight: 20, min_depth: 3, tundra_only: false },
    WeightedFamily { family: EnemyFamily::BatHuge, weight: 15, min_depth: 5, tundra_only: false },
    WeightedFamily { family: EnemyFamily::SpiderSmall, weight: 45, min_depth: 0, tundra_only: false },
    WeightedFamily { family: EnemyFamily::SpiderLarge, weight: 30, min_depth: 2, tundra_only: false },
    WeightedFamily { family: EnemyFamily::Buzzer, weight: 24, min_depth: DEEP_ENEMY_DEPTH, tundra_only: true },
    WeightedFamily { family: EnemyFamily::Squid, weight: 24, min_depth: DEEP_ENEMY_DEPTH, tundra_only: false },
    WeightedFamily { family: EnemyFamily::Opi, weight: 18, min_depth: DEEP_ENEMY_DEPTH, tundra_only: true },
];
const VOID_ROSTER: &[WeightedFamily] = &[
    WeightedFamily { family: EnemyFamily::Void, weight: 1, min_depth: 0, tundra_only: false },
];

fn roster(depth: u32, escalation: bool) -> impl Iterator<Item = WeightedFamily> {
    let entries = if escalation { VOID_ROSTER } else { ROSTER };
    entries.iter().copied().filter(move |entry| depth >= entry.min_depth)
}

#[derive(Clone, Copy)]
struct SpawnSelection {
    family: EnemyFamily,
    candidate: SpawnCandidate,
    archetype: &'static str,
    blueprint_position: Vec2,
}

struct SpawnContext<'a> {
    catalog: &'a EnemySpawnCatalog,
    compiled: &'a CompiledLevel,
    collision: &'a TileCollisionWorld,
    actors: &'a ActorCatalog,
    leased_candidates: &'a HashSet<u32>,
    level_seed: u32,
    player_position: Vec2,
    camera_position: Vec2,
    view_half_size: Vec2,
}

fn select_ready_spawn(
    context: &SpawnContext,
    runtime: &EnemySpawnRuntime,
    region_key: IVec2,
    slots: [Option<(usize, bool)>; 2],
) -> Option<(usize, SpawnSelection)> {
    for (slot_index, distance_exempt) in slots.into_iter().flatten() {
        let generation = runtime.regions[&region_key].slots[slot_index].generation;
        if let Some(selection) = select_spawn(context, region_key, slot_index, generation, distance_exempt) {
            return Some((slot_index, selection));
        }
    }
    None
}

fn select_spawn(
    context: &SpawnContext,
    region_key: IVec2,
    slot_index: usize,
    generation: u32,
    distance_exempt: bool,
) -> Option<SpawnSelection> {
    let region = context.catalog.regions.get(&region_key)?;
    let salt = (slot_index as u32).wrapping_mul(0x9E37_79B9).wrapping_add(generation);
    let available = roster(region.depth, slot_index == VOID_SLOT)
        .filter(|entry| region.candidates.iter().any(|candidate| {
            spawn_candidate(context, *entry, *candidate, region.depth, distance_exempt).is_some()
        })).collect::<Vec<_>>();
    let total_weight = available.iter().map(|entry| entry.weight).sum::<u32>();
    if total_weight == 0 { return None; }
    let roll = position_hash(context.level_seed, region_key.x, region_key.y, salt) % u64::from(total_weight);
    let family = weighted_family_at_roll(&available, roll as u32)?;
    let entry = available.into_iter().find(|entry| entry.family == family)?;
    region.candidates.iter().filter_map(|candidate| {
        spawn_candidate(context, entry, *candidate, region.depth, distance_exempt)
    }).min_by_key(|selection| position_hash(context.level_seed, selection.candidate.tile.x,
        selection.candidate.tile.y, salt ^ selection.candidate.id))
}

fn spawn_candidate(
    context: &SpawnContext,
    entry: WeightedFamily,
    candidate: SpawnCandidate,
    depth: u32,
    distance_exempt: bool,
) -> Option<SpawnSelection> {
    if depth < entry.min_depth || !candidate.habitats.contains(entry.family.habitat())
        || (entry.tundra_only && !candidate.biome.is_tundra())
        || context.leased_candidates.contains(&candidate.id)
        || !dynamic_candidate_habitat_is_valid(context.compiled, candidate.tile,
            entry.family.habitat(), context.catalog.tile_size) { return None; }
    let archetype = family_archetype(entry.family, candidate, depth, context.level_seed);
    let actor = context.actors.actors.get(archetype)?;
    let center = candidate_actor_position(candidate, entry.family.habitat(),
        context.catalog.tile_size, actor);
    let minimum = if distance_exempt { 0.0 } else {
        context.catalog.tile_size * mapgen::CHUNK_IMAGE_SIZE as f32 * MIN_SPAWN_DISTANCE_CHUNKS
    };
    if !position_is_spawnable(center, context.player_position, minimum * minimum,
        context.camera_position, context.view_half_size)
        || !spawn_body_is_clear(context, center, actor.body.collider_half_extents)
        || (entry.family.habitat() != SpawnHabitat::Air
            && !body_has_safe_support(context, center, actor.body.collider_half_extents)) { return None; }
    Some(SpawnSelection { family: entry.family, candidate, archetype,
        blueprint_position: center - actor.body.spawn_offset.truncate() })
}

fn candidate_actor_position(candidate: SpawnCandidate, habitat: SpawnHabitat,
    tile_size: f32, actor: &ResolvedActor,
) -> Vec2 {
    if habitat == SpawnHabitat::Air { return candidate.world_position; }
    candidate.world_position + Vec2::Y * (actor.body.collider_half_extents.y - tile_size * 0.5)
}

fn spawn_body_is_clear(context: &SpawnContext, center: Vec2, half: Vec2) -> bool {
    let bounds = Rect::from_corners(center - half, center + half);
    let world_max = Vec2::new(context.catalog.width as f32, context.catalog.height as f32) * context.catalog.tile_size;
    if bounds.min.min_element() < 0.0 || bounds.max.x > world_max.x || bounds.max.y > world_max.y
        || context.collision.overlaps_solid(center, half) { return false; }
    let Some(range) = context.collision.tile_range_for_rect(bounds) else { return false; };
    range.tiles().all(|tile| !context.compiled.spike_tiles[(tile.y as u32 * context.compiled.width + tile.x as u32) as usize])
}

fn body_has_safe_support(context: &SpawnContext, center: Vec2, half: Vec2) -> bool {
    let tile_size = context.catalog.tile_size;
    let left = ((center.x - half.x + 0.001) / tile_size).floor() as i32;
    let right = ((center.x + half.x - 0.001) / tile_size).floor() as i32;
    let row = context.catalog.height as i32 - 1 - ((center.y - half.y - 0.001) / tile_size).floor() as i32;
    (left..=right).all(|x| compiled_tile_is_safe_ground(context.compiled, x, row))
}

fn family_archetype(family: EnemyFamily, candidate: SpawnCandidate, depth: u32, seed: u32) -> &'static str {
    match family {
        EnemyFamily::BatSmall => "bat_small",
        EnemyFamily::BatMedium => "bat_medium",
        EnemyFamily::BatBig => "bat_big",
        EnemyFamily::BatHuge => "bat_huge",
        EnemyFamily::Buzzer => "buzzer",
        EnemyFamily::Squid => "squid",
        EnemyFamily::Opi => "opi",
        EnemyFamily::Void => "void",
        EnemyFamily::SpiderSmall | EnemyFamily::SpiderLarge => {
            let biome = if candidate.biome.is_tundra() { SpiderBiome::Tundra } else { SpiderBiome::Ice };
            let role = if family == EnemyFamily::SpiderSmall { Role::Melee } else { Role::Ranged };
            let tier = spider_tier(biome, depth, position_hash(seed, candidate.tile.x, candidate.tile.y, 0x5350_4944) as u32);
            spider_archetype(biome, role, tier).id
        },
    }
}

fn dynamic_candidate_habitat_is_valid(
    compiled: &CompiledLevel,
    tile: IVec2,
    habitat: SpawnHabitat,
    tile_size: f32,
) -> bool {
    let radius = air_clearance_radius(tile_size);
    match habitat {
        SpawnHabitat::Air => compiled_rect_is_clear(
            compiled,
            tile.x - radius,
            tile.y - radius,
            tile.x + radius,
            tile.y + radius,
        ),
        SpawnHabitat::GroundSmall => {
            compiled_ground_candidate_is_valid(compiled, tile, GROUND_SMALL_PROFILE)
        },
        SpawnHabitat::GroundLarge => {
            compiled_ground_candidate_is_valid(compiled, tile, GROUND_LARGE_PROFILE)
        },
        SpawnHabitat::GroundTall => {
            compiled_ground_candidate_is_valid(compiled, tile, GROUND_TALL_PROFILE)
        },
    }
}

fn compiled_ground_candidate_is_valid(
    compiled: &CompiledLevel,
    tile: IVec2,
    profile: GroundProfile,
) -> bool {
    if !compiled_rect_is_clear(
        compiled,
        tile.x - profile.side_clearance,
        tile.y - profile.headroom,
        tile.x + profile.side_clearance,
        tile.y,
    ) {
        return false;
    }

    let support_y = tile.y + 1;
    let support_left = (profile.support_width - 1) / 2;
    let support_right = profile.support_width - 1 - support_left;
    (tile.x - support_left..=tile.x + support_right)
        .all(|x| compiled_tile_is_safe_ground(compiled, x, support_y))
}

fn compiled_rect_is_clear(
    compiled: &CompiledLevel,
    min_x: i32,
    min_y: i32,
    max_x: i32,
    max_y: i32,
) -> bool {
    if min_x < 0
        || min_y < 0
        || max_x >= compiled.width as i32
        || max_y >= compiled.height as i32
    {
        return false;
    }
    (min_y..=max_y).all(|y| {
        (min_x..=max_x).all(|x| {
            let index = (y as u32 * compiled.width + x as u32) as usize;
            !compiled.terrain.get(index).copied().unwrap_or(true)
                && !compiled.spike_tiles.get(index).copied().unwrap_or(true)
        })
    })
}

fn compiled_tile_is_safe_ground(compiled: &CompiledLevel, x: i32, y: i32) -> bool {
    if x < 0 || y < 0 || x >= compiled.width as i32 || y >= compiled.height as i32 {
        return false;
    }
    let index = (y as u32 * compiled.width + x as u32) as usize;
    compiled.terrain.get(index).copied().unwrap_or(false)
        && !compiled.spike_tiles.get(index).copied().unwrap_or(true)
}

fn weighted_family_at_roll(
    entries: &[WeightedFamily],
    mut roll: u32,
) -> Option<EnemyFamily> {
    for entry in entries {
        if roll < entry.weight {
            return Some(entry.family);
        }
        roll -= entry.weight;
    }
    None
}

fn position_is_spawnable(
    position: Vec2,
    player_position: Vec2,
    minimum_distance_squared: f32,
    camera_position: Vec2,
    view_half_size: Vec2,
) -> bool {
    if position.distance_squared(player_position) < minimum_distance_squared {
        return false;
    }
    position_is_offscreen(position, camera_position, view_half_size)
}

fn position_is_offscreen(
    position: Vec2,
    camera_position: Vec2,
    view_half_size: Vec2,
) -> bool {
    let delta = (position - camera_position).abs();
    delta.x > view_half_size.x + OFFSCREEN_MARGIN
        || delta.y > view_half_size.y + OFFSCREEN_MARGIN
}

fn spawn_selection(commands: &mut Commands, selection: SpawnSelection, depth: u32, seed: u32) -> Entity {
    let transform = enemy_spawn_transform(selection.blueprint_position);
    if matches!(selection.family, EnemyFamily::SpiderSmall | EnemyFamily::SpiderLarge) {
        spawn_spider_selection(commands, selection, transform, depth, seed)
    } else {
        spawn_enemy_actor_blueprint(commands, selection.archetype, transform)
    }
}

fn enemy_spawn_transform(position: Vec2) -> Transform {
    Transform::from_translation(position.extend(10.0 * 0.000_001))
}

fn spawn_spider_selection(
    commands: &mut Commands,
    selection: SpawnSelection,
    transform: Transform,
    depth: u32,
    level_seed: u32,
) -> Entity {
    let biome = if selection.candidate.biome.is_tundra() {
        SpiderBiome::Tundra
    } else {
        SpiderBiome::Ice
    };
    let role = if selection.family == EnemyFamily::SpiderSmall {
        Role::Melee
    } else {
        Role::Ranged
    };
    let tier = spider_tier(
        biome,
        depth,
        position_hash(
            level_seed,
            selection.candidate.tile.x,
            selection.candidate.tile.y,
            0x5350_4944,
        ) as u32,
    );
    let archetype = spider_archetype(biome, role, tier);
    let spider = SpiderActorSetup {
        role,
        tier,
        biome,
        small: archetype.small,
        large_freeze: archetype.large_freeze,
        spawn_z: spider_z(role, tier),
    };
    spawn_spider_actor_blueprint(commands, archetype.id, transform, spider)
}

fn spider_tier(biome: SpiderBiome, depth: u32, roll: u32) -> Tier {
    if biome == SpiderBiome::Tundra || depth < MIN_TIER_THREE_DEPTH {
        return Tier::Base;
    }
    if roll % 100 < TIER_THREE_PERCENT {
        Tier::Three
    } else {
        Tier::Base
    }
}

#[derive(Clone, Copy)]
struct SpiderArchetype {
    id: &'static str,
    small: bool,
    large_freeze: bool,
}

fn spider_archetype(
    biome: SpiderBiome,
    role: Role,
    tier: Tier,
) -> SpiderArchetype {
    let (id, small, large_freeze) = match (biome, role, tier) {
        (SpiderBiome::Tundra, Role::Melee, _) => ("spider_small_tundra", true, false),
        (SpiderBiome::Tundra, Role::Ranged, _) => ("spider_big_tundra", false, true),
        (SpiderBiome::Ice, Role::Melee, Tier::Base) => ("spider_small", true, false),
        (SpiderBiome::Ice, Role::Melee, Tier::Two) => ("spider_small_t2", true, false),
        (SpiderBiome::Ice, Role::Melee, Tier::Three) => ("spider_small_t3", true, false),
        (SpiderBiome::Ice, Role::Ranged, Tier::Base) => ("spider_big", false, true),
        (SpiderBiome::Ice, Role::Ranged, Tier::Two) => ("spider_big_t2", false, true),
        (SpiderBiome::Ice, Role::Ranged, Tier::Three) => ("spider_big_t3", false, true),
    };
    SpiderArchetype { id, small, large_freeze }
}

fn spider_z(role: Role, tier: Tier) -> f32 {
    let role_offset = if matches!(role, Role::Melee) { 0.000_000_1 } else { 0.0 };
    let tier_offset = match tier {
        Tier::Base => 0.0,
        Tier::Two => 0.000_000_01,
        Tier::Three => 0.000_000_02,
    };
    14.0 * 0.000_001 + role_offset + tier_offset
}

pub(super) fn release_dead_spawn_leases(
    mut commands: Commands,
    mut runtime: ResMut<EnemySpawnRuntime>,
    game_time: Res<GameTime>,
    dead_q: Query<(Entity, &SpawnDirectorOwned), Added<Dead>>,
) {
    let cooldown_ticks = (RESPAWN_SECONDS * game_time.hz).round() as u32;
    for (entity, lease) in dead_q.iter() {
        let Some(region) = runtime.regions.get_mut(&lease.region) else {
            continue;
        };
        let Some(slot) = region.slots.get_mut(lease.slot) else {
            continue;
        };
        if slot.actor != Some(entity)
            || slot.candidate_id != Some(lease.candidate_id)
        {
            continue;
        }
        slot.actor = None;
        slot.candidate_id = None;
        slot.cooldown_ticks = cooldown_ticks;
        runtime.leased_candidates.remove(&lease.candidate_id);
        commands.entity(entity).remove::<SpawnDirectorOwned>();
    }
}

pub(crate) fn reset_enemy_spawn_runtime(commands: &mut Commands) {
    commands.remove_resource::<EnemySpawnCatalog>();
    commands.insert_resource(EnemySpawnRuntime::default());
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_grid(width: usize, height: usize) -> SpawnCatalogGrid {
        let blocked = vec![false; width * height];
        let safe_ground = vec![false; width * height];
        SpawnCatalogGrid {
            width,
            height,
            tile_size: 6.0,
            blocked: SummedAreaTable::new(width, height, &blocked),
            safe_ground: SummedAreaTable::new(width, height, &safe_ground),
            protected: vec![false; width * height],
            biomes: vec![BiomeKind::Ice; width * height],
        }
    }

    fn empty_test_level(width: u32, height: u32) -> CompiledLevel {
        let len = (width * height) as usize;
        CompiledLevel {
            width,
            height,
            bounds: Rect::new(0.0, 0.0, width as f32 * 6.0, height as f32 * 6.0),
            terrain: vec![false; len],
            spike_tiles: vec![false; len],
            optional_spike_tiles: vec![false; len],
            ..Default::default()
        }
    }

    fn grid_with_ground(
        width: usize,
        height: usize,
        ground_y: usize,
        start_x: usize,
        support_width: usize,
    ) -> SpawnCatalogGrid {
        let mut blocked = vec![false; width * height];
        let mut safe_ground = vec![false; width * height];
        for x in start_x..start_x + support_width {
            blocked[ground_y * width + x] = true;
            safe_ground[ground_y * width + x] = true;
        }
        SpawnCatalogGrid {
            width,
            height,
            tile_size: 6.0,
            blocked: SummedAreaTable::new(width, height, &blocked),
            safe_ground: SummedAreaTable::new(width, height, &safe_ground),
            protected: vec![false; width * height],
            biomes: vec![BiomeKind::Ice; width * height],
        }
    }

    #[test]
    fn air_profile_fits_the_largest_flying_body() {
        let grid = test_grid(12, 12);
        assert!(candidate_habitats(&grid, 5, 5).contains(SpawnHabitat::Air));
        assert!(!candidate_habitats(&grid, 0, 5).contains(SpawnHabitat::Air));
    }

    #[test]
    fn activated_optional_spike_invalidates_cataloged_air_candidate() {
        let mut compiled = empty_test_level(20, 20);
        let candidate = IVec2::new(10, 10);
        assert!(dynamic_candidate_habitat_is_valid(
            &compiled,
            candidate,
            SpawnHabitat::Air,
            6.0,
        ));

        let spike_index = (10 * compiled.width + 11) as usize;
        compiled.spike_tiles[spike_index] = true;
        assert!(!dynamic_candidate_habitat_is_valid(
            &compiled,
            candidate,
            SpawnHabitat::Air,
            6.0,
        ));
    }

    #[test]
    fn unoccupied_chunks_block_cross_chunk_air_clearance() {
        let width = 192u32;
        let height = 32u32;
        let len = (width * height) as usize;
        let terrain = vec![false; len];
        let compiled = CompiledLevel {
            bounds: Rect::new(0.0, 0.0, width as f32 * 6.0, height as f32 * 6.0),
            width,
            height,
            terrain: terrain.clone(),
            spike_tiles: vec![false; len],
            spawns: vec![crate::levelgen::SpawnSpec { kind: SpawnKind::Player, position: Vec2::ZERO, chunk: IVec2::ZERO }],
            ..Default::default()
        };
        let biomes = LevelBiomeMask {
            width,
            height,
            cells: vec![BiomeKind::Ice; len],
            tundra_inner: vec![false; len],
        };
        let occupancy = LevelChunkOccupancyMask {
            chunks_w: 2,
            chunks_h: 1,
            occupied: vec![true, false],
        };
        let mut collision = TileCollisionWorld::default();
        collision.rebuild(width, height, 6.0, &terrain);
        let grid = build_catalog_grid(
            &compiled,
            &biomes,
            Some(&occupancy),
            &collision,
        )
        .expect("catalog grid");

        assert!(candidate_habitats(&grid, 90, 10).contains(SpawnHabitat::Air));
        assert!(!candidate_habitats(&grid, 95, 10).contains(SpawnHabitat::Air));
        assert!(!candidate_habitats(&grid, 100, 10).contains(SpawnHabitat::Air));
    }

    #[test]
    fn large_ground_profile_accepts_three_tile_support_and_clearance() {
        let grid = grid_with_ground(30, 20, 14, 13, 3);
        assert!(ground_candidate_is_valid(&grid, 14, 13, GROUND_LARGE_PROFILE));
    }

    #[test]
    fn large_ground_profile_rejects_two_support_tiles() {
        let grid = grid_with_ground(30, 20, 14, 13, 2);
        assert!(!ground_candidate_is_valid(&grid, 14, 13, GROUND_LARGE_PROFILE));
    }

    #[test]
    fn ground_profiles_reject_unsafe_support_and_level_edges() {
        let mut blocked = vec![false; 30 * 20];
        for x in 9..21 {
            blocked[14 * 30 + x] = true;
        }
        let unsafe_ground = vec![false; 30 * 20];
        let grid = SpawnCatalogGrid {
            width: 30,
            height: 20,
            tile_size: 6.0,
            blocked: SummedAreaTable::new(30, 20, &blocked),
            safe_ground: SummedAreaTable::new(30, 20, &unsafe_ground),
            protected: vec![false; 30 * 20],
            biomes: vec![BiomeKind::Ice; 30 * 20],
        };
        assert!(!ground_candidate_is_valid(&grid, 14, 13, GROUND_LARGE_PROFILE));
        assert!(!ground_candidate_is_valid(&grid, 3, 2, GROUND_LARGE_PROFILE));
    }

    #[test]
    fn large_ground_profile_rejects_side_and_ceiling_obstacles() {
        let mut blocked = vec![false; 30 * 20];
        let mut safe_ground = vec![false; 30 * 20];
        for x in 9..21 {
            blocked[14 * 30 + x] = true;
            safe_ground[14 * 30 + x] = true;
        }
        blocked[13 * 30 + 13] = true;
        let side_blocked = SpawnCatalogGrid {
            width: 30,
            height: 20,
            tile_size: 6.0,
            blocked: SummedAreaTable::new(30, 20, &blocked),
            safe_ground: SummedAreaTable::new(30, 20, &safe_ground),
            protected: vec![false; 30 * 20],
            biomes: vec![BiomeKind::Ice; 30 * 20],
        };
        assert!(!ground_candidate_is_valid(&side_blocked, 14, 13, GROUND_LARGE_PROFILE));

        blocked[13 * 30 + 13] = false;
        blocked[11 * 30 + 14] = true;
        let ceiling_blocked = SpawnCatalogGrid {
            width: 30,
            height: 20,
            tile_size: 6.0,
            blocked: SummedAreaTable::new(30, 20, &blocked),
            safe_ground: SummedAreaTable::new(30, 20, &safe_ground),
            protected: vec![false; 30 * 20],
            biomes: vec![BiomeKind::Ice; 30 * 20],
        };
        assert!(!ground_candidate_is_valid(&ceiling_blocked, 14, 13, GROUND_LARGE_PROFILE));
    }

    #[test]
    fn thinning_keeps_ground_sites_before_adjacent_air_sites() {
        let raw = vec![
            RawCandidate { tile: IVec2::new(7, 7), habitats: HabitatMask(1), hash: 1 },
            RawCandidate { tile: IVec2::new(8, 7), habitats: HabitatMask(2), hash: 2 },
            RawCandidate { tile: IVec2::new(10, 7), habitats: HabitatMask(4), hash: 3 },
        ];
        let accepted = thin_candidates(raw);
        assert_eq!(accepted.len(), 2);
        assert_eq!(accepted[0].tile, IVec2::new(8, 7));
        assert_eq!(accepted[1].tile, IVec2::new(10, 7));
    }

    #[test]
    fn protected_anchor_mask_covers_twelve_tiles() {
        let mut protected = vec![false; 40 * 40];
        let center = Vec2::new(20.5 * 6.0, (40.0 - 20.5) * 6.0);
        mark_protected_world_position(&mut protected, 40, 40, 6.0, center);

        assert!(protected[20 * 40 + 20]);
        assert!(protected[8 * 40 + 8]);
        assert!(!protected[7 * 40 + 20]);
    }

    #[test]
    fn grouping_assigns_spawn_site_depth_and_support_biome() {
        let mut grid = test_grid(192, 192);
        grid.biomes[101 * 192 + 100] = BiomeKind::Tundra;
        let raw = RawCandidate {
            tile: IVec2::new(100, 100),
            habitats: HabitatMask(1 << SpawnHabitat::GroundSmall.index()),
            hash: 1,
        };
        let catalog = group_candidates_by_region(
            vec![raw],
            &grid,
            IVec2::new(0, 0),
            192,
            192,
        );
        let region = &catalog.regions[&IVec2::new(1, 1)];
        assert_eq!(region.depth, 1);
        assert_eq!(region.candidates[0].biome, BiomeKind::Tundra);
    }

    fn actor_catalog() -> ActorCatalog {
        let mut world = crate::game::actors::fixtures::fixture_world();
        crate::game::actors::fixtures::fixture_catalog(&mut world).expect("authored actor catalog")
    }

    fn selection_catalog(air_count: u32, biome: BiomeKind, depth: u32) -> (EnemySpawnCatalog, CompiledLevel, TileCollisionWorld) {
        let mut compiled = empty_test_level(96, 96);
        compiled.terrain[60 * 96..61 * 96].fill(true);
        let mut collision = TileCollisionWorld::default();
        collision.rebuild(96, 96, 6.0, &compiled.terrain);
        let grid = test_grid(96, 96);
        let mut candidates = (0..air_count).map(|id| SpawnCandidate {
            id, tile: IVec2::new(10 + id as i32, 20),
            world_position: grid.world_position(10 + id as i32, 20), biome, habitats: HabitatMask(1),
        }).collect::<Vec<_>>();
        candidates.extend([20, 50, 75].into_iter().enumerate().map(|(index, x)| SpawnCandidate {
            id: 100 + index as u32, tile: IVec2::new(x, 59),
            world_position: grid.world_position(x, 59), biome, habitats: HabitatMask(0b1110),
        }));
        (EnemySpawnCatalog { tile_size: 6.0, width: 96, height: 96,
            regions: HashMap::from([(IVec2::ZERO, RegionCatalog { depth, candidates })]) }, compiled, collision)
    }

    fn test_context<'a>(catalog: &'a EnemySpawnCatalog, compiled: &'a CompiledLevel,
        collision: &'a TileCollisionWorld, actors: &'a ActorCatalog, leases: &'a HashSet<u32>, seed: u32,
    ) -> SpawnContext<'a> {
        SpawnContext { catalog, compiled, collision, actors, leased_candidates: leases, level_seed: seed,
            player_position: Vec2::new(-2000.0, 0.0), camera_position: Vec2::ZERO, view_half_size: Vec2::splat(10.0) }
    }

    #[test]
    fn initial_distance_exemption_still_respects_candidate_leases() {
        let (catalog, compiled, collision) = selection_catalog(1, BiomeKind::Ice, 0);
        let actors = actor_catalog();
        let empty = HashSet::new();
        let context = test_context(&catalog, &compiled, &collision, &actors, &empty, 1);
        let selected = select_spawn(&context, IVec2::ZERO, 1, 0, true).expect("valid spawn");
        let leased = catalog.regions[&IVec2::ZERO].candidates.iter().map(|candidate| candidate.id).collect();
        let context = test_context(&catalog, &compiled, &collision, &actors, &leased, 1);
        assert!(select_spawn(&context, IVec2::ZERO, 1, 0, true).is_none());
        assert!(catalog.regions[&IVec2::ZERO].candidates.iter().any(|candidate| candidate.id == selected.candidate.id));
    }

    #[test]
    fn air_site_count_does_not_bias_family_selection() {
        let actors = actor_catalog();
        let leases = HashSet::new();
        let (sparse, compiled, collision) = selection_catalog(1, BiomeKind::Tundra, 6);
        let (dense, _, _) = selection_catalog(40, BiomeKind::Tundra, 6);
        let mut ground_spawns = 0;
        let mut families = HashSet::new();
        for seed in 0..400 {
            let sparse_context = test_context(&sparse, &compiled, &collision, &actors, &leases, seed);
            let dense_context = test_context(&dense, &compiled, &collision, &actors, &leases, seed);
            let a = select_spawn(&sparse_context, IVec2::ZERO, 1, 0, true).unwrap();
            let b = select_spawn(&dense_context, IVec2::ZERO, 1, 0, true).unwrap();
            assert_eq!(a.family, b.family);
            ground_spawns += usize::from(a.family.habitat() != SpawnHabitat::Air);
            families.insert(a.archetype);
        }
        assert!(ground_spawns > 140);
        for id in ["spider_small_tundra", "spider_big_tundra", "buzzer", "squid", "opi"] { assert!(families.contains(id)); }
        assert!(!families.contains("void"));
    }

    #[test]
    fn deep_species_follow_biome_and_depth_and_setup_lands_supported() {
        let actors = actor_catalog();
        let leases = HashSet::new();
        let (catalog, compiled, collision) = selection_catalog(1, BiomeKind::Ice, 6);
        let context = test_context(&catalog, &compiled, &collision, &actors, &leases, 1);
        let candidate = catalog.regions[&IVec2::ZERO].candidates[1];
        let squid = ROSTER.iter().find(|entry| entry.family == EnemyFamily::Squid).unwrap();
        let selection = spawn_candidate(&context, *squid, candidate, 6, true).unwrap();
        let body = &actors.actors[selection.archetype].body;
        let actual_center = selection.blueprint_position + body.spawn_offset.truncate();
        assert!((actual_center.y - body.collider_half_extents.y - 216.0).abs() < 0.001);
        assert!(!collision.overlaps_solid(actual_center, body.collider_half_extents));
        for entry in roster(5, false) { assert!(!matches!(entry.family, EnemyFamily::Buzzer | EnemyFamily::Squid | EnemyFamily::Opi)); }
        for family in [EnemyFamily::Buzzer, EnemyFamily::Opi] {
            let entry = *ROSTER.iter().find(|entry| entry.family == family).unwrap();
            assert!(spawn_candidate(&context, entry, candidate, 6, true).is_none());
        }
    }

    #[test]
    fn population_scales_from_three_to_eight() {
        assert_eq!(target_population(1, 0), 3);
        assert_eq!(target_population(8, 0), 7);
        assert_eq!(target_population(8, 4), 8);
        assert_eq!(target_population(8, u32::MAX), 8);
    }

    #[test]
    fn spawn_gate_requires_two_chunks_of_player_distance() {
        let chunk_world_size = mapgen::CHUNK_IMAGE_SIZE as f32 * 6.0;
        let camera = Vec2::ZERO;
        let view_half_size = Vec2::splat(100.0);

        assert!(!position_is_spawnable(
            Vec2::new(chunk_world_size * 2.0 - 0.1, 0.0),
            Vec2::ZERO,
            (chunk_world_size * MIN_SPAWN_DISTANCE_CHUNKS).powi(2),
            camera,
            view_half_size,
        ));
        assert!(position_is_spawnable(
            Vec2::new(chunk_world_size * 2.0, 0.0),
            Vec2::ZERO,
            (chunk_world_size * MIN_SPAWN_DISTANCE_CHUNKS).powi(2),
            camera,
            view_half_size,
        ));
        assert!(position_is_spawnable(
            Vec2::new(200.0, 0.0),
            Vec2::ZERO,
            0.0,
            camera,
            view_half_size,
        ));
    }

    #[test]
    fn void_slot_is_reserved_only_at_maximum_immunity_and_ordinary_indexes_stay_stable() {
        let mut runtime = EnemySpawnRuntime::default();
        let region = IVec2::ZERO;
        let slots = prepare_region_for_spawn(&mut runtime, region, target_population(1, 6), true, false);
        assert_eq!(slots, [None, Some((1, true))]);
        let ordinary = runtime.regions[&region].slots.len() - 1;
        let slots = prepare_region_for_spawn(&mut runtime, region, target_population(8, 6), true, true);
        assert_eq!(slots[0], Some((VOID_SLOT, false)));
        assert!(runtime.regions[&region].slots.len() > ordinary);
        assert_eq!(runtime.regions[&region].slots.len(), 8);
    }

    #[test]
    fn falling_immunity_preserves_live_leases_and_death_releases_a_retained_high_slot() {
        use bevy::ecs::system::RunSystemOnce;

        let mut world = World::new();
        world.insert_resource(GameTime::default());
        let mut runtime = EnemySpawnRuntime::default();
        let region = IVec2::ZERO;
        prepare_region_for_spawn(&mut runtime, region, target_population(8, 6), false, true);
        let mut last = None;
        for index in 5..8 {
            let candidate_id = index as u32;
            let entity = world.spawn(SpawnDirectorOwned { region, slot: index, candidate_id }).id();
            let slot = &mut runtime.regions.get_mut(&region).unwrap().slots[index];
            slot.actor = Some(entity);
            slot.candidate_id = Some(candidate_id);
            runtime.leased_candidates.insert(candidate_id);
            last = Some(entity);
        }
        assert_eq!(prepare_region_for_spawn(&mut runtime, region, 3, false, false), [None, None]);
        assert_eq!(runtime.regions[&region].slots.len(), 8);
        assert_eq!(runtime.leased_candidates.len(), 3);
        world.insert_resource(runtime);
        let actor = last.unwrap();
        world.entity_mut(actor).insert(Dead { ticks: 0 });
        world.run_system_once(release_dead_spawn_leases).unwrap();
        let mut runtime = world.resource_mut::<EnemySpawnRuntime>();
        assert!(!runtime.leased_candidates.contains(&7));
        assert!(runtime.regions[&region].slots[7].actor.is_none());
        assert!(runtime.regions[&region].slots[7].cooldown_ticks > 0);
        assert_eq!(prepare_region_for_spawn(&mut runtime, region, 3, false, false), [None, Some((1, false))]);
        assert!(world.get::<SpawnDirectorOwned>(actor).is_none());
    }

    #[test]
    fn void_uses_any_depth_and_biome_and_missing_air_does_not_block_ordinary_spawns() {
        let actors = actor_catalog();
        let leases = HashSet::new();
        for (biome, depth) in [(BiomeKind::Ice, 0), (BiomeKind::Tundra, 12)] {
            let (catalog, compiled, collision) = selection_catalog(1, biome, depth);
            let context = test_context(&catalog, &compiled, &collision, &actors, &leases, 1);
            assert_eq!(select_spawn(&context, IVec2::ZERO, VOID_SLOT, 0, false).unwrap().family, EnemyFamily::Void);
        }
        let (catalog, compiled, collision) = selection_catalog(0, BiomeKind::Ice, 0);
        let context = test_context(&catalog, &compiled, &collision, &actors, &leases, 1);
        let mut runtime = EnemySpawnRuntime::default();
        let slots = prepare_region_for_spawn(&mut runtime, IVec2::ZERO, target_population(8, 0), true, true);
        let (slot, selection) = select_ready_spawn(&context, &runtime, IVec2::ZERO, slots).unwrap();
        assert_ne!(slot, VOID_SLOT);
        assert_eq!(selection.family, EnemyFamily::SpiderSmall);
    }

    #[test]
    fn spider_tier_three_unlocks_at_depth_four_without_tier_two() {
        assert_eq!(spider_tier(SpiderBiome::Ice, 3, 0), Tier::Base);
        assert_eq!(spider_tier(SpiderBiome::Ice, 4, 0), Tier::Three);
        assert_eq!(spider_tier(SpiderBiome::Ice, 4, 14), Tier::Three);
        assert_eq!(spider_tier(SpiderBiome::Ice, 4, 15), Tier::Base);
        assert_eq!(spider_tier(SpiderBiome::Tundra, 20, 0), Tier::Base);
        for roll in 0..100 {
            assert_ne!(spider_tier(SpiderBiome::Ice, 20, roll), Tier::Two);
        }
    }

    #[test]
    fn chunk_activation_uses_chebyshev_distance() {
        let center = IVec2::new(3, 3);
        assert_eq!(chunk_distance(center, IVec2::new(4, 4)), 1);
        assert_eq!(chunk_distance(center, IVec2::new(5, 4)), 2);
        assert_eq!(chunk_distance(center, IVec2::new(0, 3)), 3);
    }
}
