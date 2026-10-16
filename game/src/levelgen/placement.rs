//! Depth and geyser biomes, decoration/entity/ore placement, and entity publication.
//!
//! Rules scan open tiles in row-major order. Around its center, a rule image's green pixels require
//! terrain, red pixels forbid it, and white pixels require a snow decoration. Every roll hashes the
//! tile, the rule's order, and the kind's name, so a seed reproduces its placements.
use std::collections::BTreeMap;

use super::*;

const CHUNK_TILES: i32 = mapgen::CHUNK_IMAGE_SIZE as i32;
const TUNDRA_RADIUS_TILES: f32 = 60.0;
/// The part of the tundra radius that also swaps decorations and entities to the tundra's.
const TUNDRA_INNER_FRACTION: f32 = 0.5;
const ORE_ENTITY_EXCLUSION_RADIUS_PX: f32 = 36.0;
const ORE_VERTICAL_OFFSET_PX: i32 = -1;

#[derive(Clone)]
pub(super) struct DecorationPlacement {
    pub handle: Handle<Image>,
    pub tile_x: u32,
    pub tile_y: u32,
    pub flip_x: bool,
    pub flip_y: bool,
    pub kind: String,
    pub active: bool,
    pub offset_px: IVec2,
    pub rule_key: u32,
}

#[derive(Clone)]
pub(super) struct EntityPlacement {
    pub handle: Handle<Image>,
    pub tile_x: u32,
    pub tile_y: u32,
    pub flip_x: bool,
    pub flip_y: bool,
    pub kind: String,
    pub chunk: IVec2,
    pub offset_px: IVec2,
    pub z_offset: f32,
    pub harvest: Option<harvestable_materials::HarvestPlantDefinition>,
    pub ore: Option<harvestable_materials::HarvestOreDefinition>,
}

/// Everything placed on a level and its depth-based biomes with local geyser patches.
pub(super) struct LevelPlacements {
    pub biomes: LevelBiomeMask,
    /// Tiles outside the inner tundra, where the ice's libraries place.
    pub ice_allowed: Vec<bool>,
    pub decorations: Vec<DecorationPlacement>,
    pub entities: Vec<EntityPlacement>,
}

/// Geysers grow local patches over the depth biomes before each biome's libraries place
/// decorations, entities (ice entities may require snow), and ores. Exterior tops never consume
/// placement budgets, and nothing stays on optional spikes.
pub(super) fn place_level(
    level: &CompiledLevel,
    libraries: &GenerationLibraries,
    images: &Assets<Image>,
    base_biomes: Option<&LevelBiomeMask>,
    chunk_colors: Option<&LevelChunkColorMask>,
    occupancy: Option<&LevelChunkOccupancyMask>,
    rooms: &[secret_rooms::SecretRoomPlacement],
) -> LevelPlacements {
    let mut taken = vec![false; (level.width * level.height) as usize];
    reserve_ore_spawn_tiles(level, &mut taken);
    // A closed room's entrance would show an entity growing inside, so rooms hold only their ore.
    for tile in rooms.iter().flat_map(secret_rooms::SecretRoomPlacement::tiles) {
        taken[(tile.y as u32 * level.width + tile.x as u32) as usize] = true;
    }
    let placeable = placement_area(level, occupancy);
    let geysers = EntityPass { library: &libraries.entities.ice, kinds: |kind| kind == GEYSER_KIND, allowed: Some(&placeable), snow: None };
    let mut entities = generate_entity_placements(level, images, chunk_colors, &geysers, &mut taken);
    let biomes = build_tundra_mask_from_geysers(level, &entities, base_biomes);
    let ice_allowed: Vec<bool> = biomes.tundra_inner.iter().zip(&placeable).map(|(&inner, &allowed)| !inner && allowed).collect();
    let tundra_allowed: Vec<bool> = biomes.tundra_inner.iter().zip(&placeable).map(|(&inner, &allowed)| inner && allowed).collect();
    let mut decorations = generate_decoration_placements(level, &libraries.decorations.ice, images, Some(&ice_allowed));
    let snow = build_decoration_mask_for_kind(&decorations, "snow", level.width, level.height);
    decorations.extend(generate_decoration_placements(level, &libraries.decorations.tundra, images, Some(&tundra_allowed)));
    let passes = [
        EntityPass { library: &libraries.entities.ice, kinds: |kind| kind != GEYSER_KIND, allowed: Some(&ice_allowed), snow: Some(&snow) },
        EntityPass { library: &libraries.entities.tundra, kinds: |_| true, allowed: Some(&tundra_allowed), snow: None },
    ];
    for pass in &passes {
        entities.extend(generate_entity_placements(level, images, chunk_colors, pass, &mut taken));
    }
    entities.extend(generate_ore_placements(level, &libraries.ores.ice, &OreCategory::ICE, Some(&ice_allowed)));
    entities.extend(generate_ore_placements(level, &libraries.ores.tundra, &OreCategory::TUNDRA, Some(&tundra_allowed)));
    decorations.retain(|placement| !tile_is_optional_spike(level, placement.tile_x, placement.tile_y));
    entities.retain(|placement| !tile_is_optional_spike(level, placement.tile_x, placement.tile_y));
    LevelPlacements { biomes, ice_allowed, decorations, entities }
}

/// Exterior sky above an isolated chunk's first rock surface is not a content habitat. This mask
/// runs before rule quotas and spacing, leaving those budgets available for interior ledges.
pub(crate) fn placement_area(level: &CompiledLevel, occupancy: Option<&LevelChunkOccupancyMask>) -> Vec<bool> {
    let mut allowed = vec![true; (level.width * level.height) as usize];
    let Some(occupancy) = occupancy else { return allowed };
    let spawn = player_spawn(level).map(|spawn| spawn.chunk);
    let chunks = UVec2::new(level.width.div_ceil(mapgen::CHUNK_IMAGE_SIZE), level.height.div_ceil(mapgen::CHUNK_IMAGE_SIZE));
    for (cx, cy) in (0..chunks.y).flat_map(|y| (0..chunks.x).map(move |x| (x, y))) {
        let occupied = occupancy.is_occupied(cx, cy);
        let exposed = cy == 0 || !occupancy.is_occupied(cx, cy - 1);
        let origin = UVec2::new(cx, cy) * mapgen::CHUNK_IMAGE_SIZE;
        let end = (origin + UVec2::splat(mapgen::CHUNK_IMAGE_SIZE)).min(UVec2::new(level.width, level.height));
        if occupied && (!exposed || spawn == Some(IVec2::new(cx as i32, cy as i32))) { continue; }
        mask_exterior_columns(level, &mut allowed, origin, end, occupied);
    }
    allowed
}

fn mask_exterior_columns(level: &CompiledLevel, allowed: &mut [bool], origin: UVec2, end: UVec2, occupied: bool) {
    for x in origin.x..end.x {
        let bottom = (origin.y..end.y).find(|&y| occupied && level.terrain[(y * level.width + x) as usize]).unwrap_or(end.y);
        for y in origin.y..bottom { allowed[(y * level.width + x) as usize] = false; }
    }
}

/// Placements per chunk and rule, which a rule's `max` tag caps.
type RuleCounts = HashMap<(IVec2, u32), u32>;

/// A rule image's pixels, as offsets from its center.
#[derive(Default)]
struct RuleMask {
    require: Vec<IVec2>,
    forbid: Vec<IVec2>,
    require_decor: Vec<IVec2>,
}

impl RuleMask {
    fn decode(image: &Image) -> Self {
        let mut mask = Self::default();
        let (width, height) = (image.width() as i32, image.height() as i32);
        let center = IVec2::new(width / 2, height / 2);
        let pixels = image.data.as_deref().unwrap_or_default().as_chunks::<4>().0.iter().take((width * height) as usize);
        for (index, pixel) in pixels.enumerate().filter(|(_, pixel)| pixel[3] >= 128) {
            let offset = IVec2::new(index as i32 % width, index as i32 / width) - center;
            let offsets = match [pixel[0], pixel[1], pixel[2]] {
                compiler::COLOR_RULE_REQUIRE => &mut mask.require,
                compiler::COLOR_RULE_FORBID => &mut mask.forbid,
                [0xFF, 0xFF, 0xFF] => &mut mask.require_decor,
                _ => continue,
            };
            offsets.push(offset);
        }
        mask
    }

    /// Whether the mask matches with its center on `anchor`, mirrored along the `flip` axes.
    fn matches(&self, scan: &RuleScan<'_>, anchor: IVec2, flip: BVec2) -> bool {
        let sign = IVec2::select(flip, IVec2::NEG_ONE, IVec2::ONE);
        let set = |cells: &[bool], offset: &IVec2| {
            tile_index(scan.level, anchor + *offset * sign).is_some_and(|index| cells.get(index) == Some(&true))
        };
        self.require.iter().all(|offset| set(&scan.level.terrain, offset))
            && !self.forbid.iter().any(|offset| set(&scan.level.terrain, offset))
            && self.require_decor.iter().all(|offset| scan.decor.is_some_and(|decor| set(decor, offset)))
    }
}

/// A usable rule of a kind, keyed by its position among them.
struct PreparedRule<'a> {
    image: &'a RuleImage,
    mask: RuleMask,
    key: u32,
}

/// Where one kind's rules are matched.
struct RuleScan<'a> {
    level: &'a CompiledLevel,
    /// Tiles covered by snow decorations.
    decor: Option<&'a [bool]>,
    kind_hash: u64,
    chunk_size: i32,
}

/// A rule that matched at a tile and won its probability roll.
struct RuleHit {
    index: usize,
    tile: IVec2,
    chunk: IVec2,
    flip: BVec2,
    hash: u64,
}

impl RuleHit {
    /// One of the hit's independent rolls.
    fn roll(&self, salt: u64) -> u64 {
        splitmix64(self.hash ^ salt)
    }
}

/// The kind's rules whose images require or forbid terrain, in order.
fn prepare_rules<'a>(kind: &'a PlacementKind, images: &Assets<Image>) -> Vec<PreparedRule<'a>> {
    let masks = kind.rules.iter().filter_map(|image| Some((image, RuleMask::decode(images.get(&image.handle)?))));
    masks.filter(|(_, mask)| !mask.require.is_empty() || !mask.forbid.is_empty())
        .enumerate()
        .map(|(key, (image, mask))| PreparedRule { image, mask, key: key as u32 })
        .collect()
}

impl PreparedRule<'_> {
    /// Tries the rule at a tile, as drawn and then mirrored as its ruleflip tags allow. Its randflip
    /// tags then mirror the artwork at random.
    fn hit(&self, scan: &RuleScan<'_>, index: usize, tile: IVec2) -> Option<RuleHit> {
        let rule = self.image;
        let flip = [BVec2::FALSE, BVec2::new(true, false), BVec2::new(false, true), BVec2::TRUE]
            .into_iter()
            .filter(|flip| (rule.ruleflip_x || !flip.x) && (rule.ruleflip_y || !flip.y))
            .find(|&flip| self.mask.matches(scan, tile, flip))?;
        let hash = tile_hash(tile.x, tile.y, rule.order, scan.kind_hash);
        if !within_percent(hash, rule.prob_percent) {
            return None;
        }
        let artwork = splitmix64(hash ^ 0xB5C0_FBCF);
        let random = BVec2::new(rule.randflip_x && artwork & 2 != 0, rule.randflip_y && artwork & 4 != 0);
        Some(RuleHit { index, tile, chunk: tile / scan.chunk_size, flip: flip | random, hash })
    }

    /// Whether the rule's `max` tag allows another placement in the chunk.
    fn below_max(&self, counts: &RuleCounts, chunk: IVec2) -> bool {
        self.image.max_per_chunk.is_none_or(|max| counts.get(&(chunk, self.key)).copied().unwrap_or(0) < max)
    }
}

/// The index of an in-bounds tile.
fn tile_index(level: &CompiledLevel, tile: IVec2) -> Option<usize> {
    let size = IVec2::new(level.width as i32, level.height as i32);
    (tile.cmpge(IVec2::ZERO).all() && tile.cmplt(size).all()).then(|| (tile.y as u32 * level.width + tile.x as u32) as usize)
}

/// Allowed tiles free of terrain and spikes, in row-major order.
fn open_tiles<'a>(level: &'a CompiledLevel, allowed: Option<&'a [bool]>) -> impl Iterator<Item = (usize, IVec2)> + 'a {
    let width = level.width as usize;
    (0..width * level.height as usize)
        .filter(move |&index| {
            allowed.is_none_or(|allowed| allowed.get(index) == Some(&true))
                && level.spike_tiles.get(index) != Some(&true)
                && level.terrain.get(index) != Some(&true)
        })
        .map(move |index| (index, IVec2::new((index % width) as i32, (index / width) as i32)))
}

fn player_spawn(level: &CompiledLevel) -> Option<&SpawnSpec> {
    level.spawns.iter().find(|spawn| spawn.kind == SpawnKind::Player)
}

fn placement_kind_hash(kind: &str) -> u64 {
    kind.bytes().fold(0x9E37_79B9_7F4A_7C15, |hash, byte| {
        let hash = (hash ^ u64::from(byte)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        hash ^ (hash >> 27)
    })
}

fn has_tags(sprite: &SpriteImage, tags: &[&'static str]) -> bool {
    tags.iter().all(|tag| sprite.tags.contains(tag))
}

/// Picks a sprite with probability proportional to its weight.
fn pick_weighted<'a>(sprites: impl Iterator<Item = &'a SpriteImage> + Clone, roll: u64) -> Option<&'a SpriteImage> {
    let total: u32 = sprites.clone().map(|sprite| sprite.weight).sum();
    let mut left = roll.checked_rem(u64::from(total))? as u32;
    for sprite in sprites {
        if left < sprite.weight {
            return Some(sprite);
        }
        left -= sprite.weight;
    }
    None
}

/// Logs how many placements each kind received.
fn log_placements<'a>(what: &str, kinds: impl Iterator<Item = &'a str>) {
    let mut counts = BTreeMap::<&str, usize>::new();
    for kind in kinds {
        *counts.entry(kind).or_default() += 1;
    }
    let summary: Vec<String> = counts.iter().map(|(kind, count)| format!("{kind}={count}")).collect();
    info!("levelgen: {what} placements -> {}", if summary.is_empty() { "none".to_string() } else { summary.join(", ") });
}

/// Decorations placed so far, and the placements stacked on each tile.
#[derive(Default)]
struct Decorations {
    placements: Vec<DecorationPlacement>,
    stacks: HashMap<usize, Vec<usize>>,
}

impl Decorations {
    /// Whether another decoration may go on the tile. On a taken tile a third of rolls skip it, a
    /// third stack on top, and a third replace the stack.
    fn make_room(&mut self, index: usize, roll: u64, counts: &mut RuleCounts) -> bool {
        let Some(stack) = self.stacks.get_mut(&index).filter(|stack| !stack.is_empty()) else {
            return true;
        };
        match roll % 3 {
            0 => false,
            1 => true,
            _ => {
                stack.drain(..).for_each(|replaced| self.placements[replaced].deactivate(counts));
                true
            },
        }
    }
}

impl DecorationPlacement {
    /// Hides a replaced decoration. Its rule's `max` count is returned under the counts of the kind
    /// being placed, even when the replaced decoration is of another kind.
    fn deactivate(&mut self, counts: &mut RuleCounts) {
        if !self.active {
            return;
        }
        self.active = false;
        let chunk = IVec2::new(self.tile_x as i32, self.tile_y as i32) / CHUNK_TILES;
        if let Some(count) = counts.get_mut(&(chunk, self.rule_key)) {
            *count = count.saturating_sub(1);
        }
    }
}

/// Places each kind in name order, trying every rule at every open tile.
pub(super) fn generate_decoration_placements(
    level: &CompiledLevel,
    library: &PlacementLibrary,
    images: &Assets<Image>,
    allowed: Option<&[bool]>,
) -> Vec<DecorationPlacement> {
    let _timer = diagnostics::StageTimer::new("generate_decoration_placements");
    let mut decorations = Decorations::default();
    for (name, kind) in &library.kinds {
        place_decorations(&mut decorations, level, allowed, (name, kind), &prepare_rules(kind, images));
    }
    log_placements("decoration", decorations.placements.iter().filter(|p| p.active).map(|p| p.kind.as_str()));
    decorations.placements
}

fn place_decorations(
    decorations: &mut Decorations,
    level: &CompiledLevel,
    allowed: Option<&[bool]>,
    (name, kind): (&str, &PlacementKind),
    rules: &[PreparedRule<'_>],
) {
    let scan = &RuleScan { level, decor: None, kind_hash: placement_kind_hash(name), chunk_size: CHUNK_TILES };
    let hits = open_tiles(level, allowed)
        .flat_map(|(index, tile)| rules.iter().filter_map(move |rule| Some((rule, rule.hit(scan, index, tile)?))));
    let mut counts = RuleCounts::new();
    for (rule, hit) in hits {
        if !rule.below_max(&counts, hit.chunk) {
            continue;
        }
        let sprites = kind.sprites.iter().filter(|sprite| has_tags(sprite, &rule.image.tags));
        let Some(sprite) = pick_weighted(sprites, hit.roll(0xA511_F90A)) else { continue };
        if !decorations.make_room(hit.index, hit.roll(0xC13F_A9A9), &mut counts) {
            continue;
        }
        decorations.stacks.entry(hit.index).or_default().push(decorations.placements.len());
        decorations.placements.push(DecorationPlacement {
            handle: sprite.handle.clone(),
            tile_x: hit.tile.x as u32,
            tile_y: hit.tile.y as u32,
            flip_x: hit.flip.x,
            flip_y: hit.flip.y,
            kind: name.to_string(),
            active: true,
            offset_px: sprite.offset_px,
            rule_key: rule.key,
        });
        *counts.entry((hit.chunk, rule.key)).or_default() += 1;
    }
}

/// Tiles holding an active decoration of `kind`.
pub(super) fn build_decoration_mask_for_kind(placements: &[DecorationPlacement], kind: &str, width: u32, height: u32) -> Vec<bool> {
    let mut mask = vec![false; (width * height) as usize];
    for placement in placements.iter().filter(|placement| placement.active && placement.kind == kind) {
        if let Some(tile) = mask.get_mut((placement.tile_y * width + placement.tile_x) as usize) {
            *tile = true;
        }
    }
    mask
}

/// One entity pass over a biome.
pub(super) struct EntityPass<'a> {
    pub library: &'a PlacementLibrary,
    /// The library's kinds this pass places.
    pub kinds: fn(&str) -> bool,
    pub allowed: Option<&'a [bool]>,
    /// Tiles covered by snow decorations.
    pub snow: Option<&'a [bool]>,
}

/// Places each kind in name order. Each open tile outside the player's chunk and six tiles clear of
/// spikes offers its first rule hit, and `colorrule` rules only hit in chunks of their map color.
pub(super) fn generate_entity_placements(
    level: &CompiledLevel,
    images: &Assets<Image>,
    chunk_colors: Option<&LevelChunkColorMask>,
    pass: &EntityPass<'_>,
    taken: &mut [bool],
) -> Vec<EntityPlacement> {
    let _timer = diagnostics::StageTimer::new("generate_entity_placements");
    let chunk_size = chunk_colors.map(|colors| colors.chunk_size as i32).filter(|&size| size > 0).unwrap_or(CHUNK_TILES);
    let spawn_chunk = player_spawn(level).map(|spawn| spawn.chunk);
    let depth_row = spawn_chunk_row(level, chunk_size);
    let forbidden = spike_clearance(level);
    let mut placements = Vec::new();
    for (name, kind) in pass.library.kinds.iter().filter(|(name, _)| (pass.kinds)(name)) {
        let rules = prepare_rules(kind, images);
        let scan = RuleScan { level, decor: pass.snow, kind_hash: placement_kind_hash(name), chunk_size };
        let candidates = open_tiles(level, pass.allowed)
            .filter(|&(index, tile)| !forbidden[index] && !taken[index] && spawn_chunk != Some(tile / chunk_size))
            .filter_map(|(index, tile)| {
                let color = chunk_colors.and_then(|colors| colors.color_for_tile(tile.x as u32, tile.y as u32));
                rules.iter()
                    .filter(|rule| rule.image.map_color.is_none_or(|map| color == Some(map)))
                    .find_map(|rule| Some((rule, rule.hit(&scan, index, tile)?)))
            })
            .collect();
        place_entities(&mut placements, taken, (name, kind), depth_row, candidates);
    }
    log_placements("entity", placements.iter().map(|p| p.kind.as_str()));
    placements
}

/// Chunks take turns top to bottom, trying their candidates in hash order. A chunk shows each sprite
/// at most once, and a rule's `mindist` keeps its entities apart.
fn place_entities(
    placements: &mut Vec<EntityPlacement>,
    taken: &mut [bool],
    (name, kind): (&str, &PlacementKind),
    depth_row: Option<i32>,
    mut candidates: Vec<(&PreparedRule<'_>, RuleHit)>,
) {
    candidates.sort_by_key(|(_, hit)| (hit.chunk.y, hit.chunk.x, hit.roll(0xA511_F90A)));
    let mut shown: HashMap<IVec2, HashSet<&Handle<Image>>> = HashMap::new();
    let mut counts = RuleCounts::new();
    let mut spaced = spatial::PlacementIndex::default();
    for (rule, hit) in candidates {
        let shown = shown.entry(hit.chunk).or_default();
        if shown.len() >= kind.sprites.len() || taken[hit.index] || !rule.below_max(&counts, hit.chunk) {
            continue;
        }
        let fits = |sprite: &&SpriteImage| {
            !shown.contains(&sprite.handle)
                && has_tags(sprite, &rule.image.tags)
                && depth_row.is_none_or(|row| minimum_depth(sprite.harvest).is_none_or(|depth| hit.chunk.y - row >= depth))
        };
        let Some(sprite) = pick_weighted(kind.sprites.iter().filter(fits), hit.roll(0xC13F_A9A9)) else { continue };
        let spacing = rule.image.min_distance_tiles as i32;
        if spacing > 0 && !spaced.clear_square(hit.tile, spacing) {
            continue;
        }
        shown.insert(&sprite.handle);
        taken[hit.index] = true;
        *counts.entry((hit.chunk, rule.key)).or_default() += 1;
        spaced.insert(hit.tile);
        placements.push(EntityPlacement {
            handle: sprite.handle.clone(),
            tile_x: hit.tile.x as u32,
            tile_y: hit.tile.y as u32,
            flip_x: hit.flip.x,
            flip_y: hit.flip.y,
            kind: name.to_string(),
            chunk: hit.chunk,
            offset_px: sprite.offset_px,
            z_offset: 0.0,
            harvest: sprite.harvest,
            ore: None,
        });
    }
}

/// Entities keep six tiles clear of spikes.
fn spike_clearance(level: &CompiledLevel) -> Vec<bool> {
    let (width, height) = (level.width as i32, level.height as i32);
    let mut forbidden = vec![false; (level.width * level.height) as usize];
    for spike in (0..forbidden.len()).filter(|&index| level.spike_tiles.get(index) == Some(&true)) {
        let (x, y) = (spike as i32 % width, spike as i32 / width);
        let columns = (x - 6).max(0) as usize..=(x + 6).min(width - 1) as usize;
        for row in (y - 6).max(0)..=(y + 6).min(height - 1) {
            let start = row as usize * width as usize;
            forbidden[start + columns.start()..=start + columns.end()].fill(true);
        }
    }
    forbidden
}

/// The chunk row of the player spawn, from which berry depth counts.
fn spawn_chunk_row(level: &CompiledLevel, chunk_size: i32) -> Option<i32> {
    let spawn = player_spawn(level)?;
    let tile_size = level.tile_size();
    if level.height == 0 || tile_size <= 0.0 {
        return None;
    }
    let bottom = level.height as f32 - 1.0;
    Some((bottom - (spawn.position.y / tile_size).floor()).clamp(0.0, bottom) as i32 / chunk_size)
}

/// Chunk rows below the spawn that berries need, authored for 192-tile chunks.
fn minimum_depth(harvest: Option<harvestable_materials::HarvestPlantDefinition>) -> Option<i32> {
    let reference_chunks = match harvest?.material {
        CraftMaterial::BlueBerry => 2,
        CraftMaterial::MossBerry => 4,
        _ => return None,
    };
    Some((reference_chunks * 192_u32).div_ceil(mapgen::CHUNK_IMAGE_SIZE) as i32)
}

/// Keeps entities out of reach of each ore marker.
pub(super) fn reserve_ore_spawn_tiles(level: &CompiledLevel, taken: &mut [bool]) {
    let tile_size = level.tile_size();
    if tile_size <= 0.0 {
        return;
    }
    let radius = (ORE_ENTITY_EXCLUSION_RADIUS_PX / tile_size).ceil() as i32;
    let reach: Vec<IVec2> = (-radius..=radius)
        .flat_map(|y| (-radius..=radius).map(move |x| IVec2::new(x, y)))
        .filter(|offset| {
            (offset.as_vec2() * tile_size).length_squared() <= ORE_ENTITY_EXCLUSION_RADIUS_PX * ORE_ENTITY_EXCLUSION_RADIUS_PX
        })
        .collect();
    for &marker in level.ore_spawn_tiles.iter().filter(|&&marker| tile_index(level, marker).is_some()) {
        for index in reach.iter().filter_map(|&offset| tile_index(level, marker + offset)) {
            taken[index] = true;
        }
    }
}

/// A cluster of two to four ores on each marker outside the player's chunk, of a random category
/// that has sprites.
pub(super) fn generate_ore_placements(
    level: &CompiledLevel,
    library: &OreLibrary,
    categories: &[OreCategory],
    allowed: Option<&[bool]>,
) -> Vec<EntityPlacement> {
    let categories: Vec<OreCategory> =
        categories.iter().copied().filter(|&category| !library.sprites_for(category).is_empty()).collect();
    let spawn_chunk = player_spawn(level).map(|spawn| spawn.chunk);
    let mut seen = HashSet::new();
    let mut placements = Vec::new();
    for &tile in &level.ore_spawn_tiles {
        let Some(index) = tile_index(level, tile) else { continue };
        let chunk = tile / CHUNK_TILES;
        if !seen.insert(tile)
            || spawn_chunk == Some(chunk)
            || allowed.is_some_and(|allowed| allowed.get(index) != Some(&true))
            || categories.is_empty()
        {
            continue;
        }
        let seed = splitmix64(((tile.x as i64 as u64) << 32) ^ ((tile.y as i64 as u64) << 16) ^ 0x4DFB_FF0A_EC0D_E123);
        let category = categories[(seed % categories.len() as u64) as usize];
        let count = 2 + (splitmix64(seed ^ 0x517A_61A5) % 3) as usize;
        let cluster = choose_ore_sprites(library.sprites_for(category), count, seed).into_iter().zip(ore_cluster_offsets(seed));
        placements.extend(cluster.enumerate().map(|(slot, (ore, offset))| EntityPlacement {
            handle: ore.handle.clone(),
            tile_x: tile.x as u32,
            tile_y: tile.y as u32,
            flip_x: splitmix64(seed ^ 0xF11F_50A5 ^ (slot as u64).wrapping_mul(17)) & 1 == 1,
            flip_y: false,
            kind: category.dir_name().to_string(),
            chunk,
            offset_px: IVec2::new(offset, ORE_VERTICAL_OFFSET_PX),
            z_offset: slot as f32 * 0.000_000_001,
            harvest: None,
            ore: Some(ore.definition),
        }));
    }
    log_placements("ore", placements.iter().map(|p| p.kind.as_str()));
    placements
}

/// Horizontal offsets of a cluster's ores: the center, either side, then one further out.
fn ore_cluster_offsets(seed: u64) -> [i32; 4] {
    let distance = |salt: u64| 6 + (splitmix64(seed ^ salt) % 13) as i32;
    let (left, right, extra) = (-distance(0xB8C1_5A11), distance(0xC0B4_17E5), distance(0xD6E8_0A93));
    [0, left, right, if splitmix64(seed ^ 0xA2F1_3C77) & 1 == 0 { left - extra } else { right + extra }]
}

/// One ore per slot, each of a different variant in a seeded order while variants last.
fn choose_ore_sprites(sprites: &[OreImage], count: usize, seed: u64) -> Vec<&OreImage> {
    let mut variants: Vec<u32> = sprites.iter().map(|sprite| sprite.definition.variant).collect();
    variants.sort_unstable();
    variants.dedup();
    variants.sort_by_key(|&variant| splitmix64(seed ^ u64::from(variant).wrapping_mul(0x9E37_79B1)));
    (0..count)
        .map(|slot| {
            let salt = slot as u64;
            let Some(&variant) = variants.get(slot) else {
                return &sprites[(splitmix64(seed ^ 0x8F3D_AA91 ^ salt.wrapping_mul(131)) % sprites.len() as u64) as usize];
            };
            let matching: Vec<&OreImage> = sprites.iter().filter(|sprite| sprite.definition.variant == variant).collect();
            matching[(splitmix64(seed ^ u64::from(variant) ^ salt.wrapping_mul(97)) % matching.len() as u64) as usize]
        })
        .collect()
}

/// Grows tundra around each geyser over the base biomes.
pub(super) fn build_tundra_mask_from_geysers(
    level: &CompiledLevel,
    geysers: &[EntityPlacement],
    base: Option<&LevelBiomeMask>,
) -> LevelBiomeMask {
    let (size, len) = (UVec2::new(level.width, level.height), (level.width * level.height) as usize);
    let cells = match base {
        Some(mask) if mask.matches_dims(size.x, size.y) => mask.cells.clone(),
        _ => vec![BiomeKind::Ice; len],
    };
    let tundra_inner = cells.iter().map(|biome| biome.is_tundra()).collect();
    let mut mask = LevelBiomeMask { width: size.x, height: size.y, cells, tundra_inner };
    let reach = TUNDRA_RADIUS_TILES.ceil() as u32;
    let inner = TUNDRA_RADIUS_TILES * TUNDRA_INNER_FRACTION;
    let stamps = geysers.iter().flat_map(|geyser| {
        let center = UVec2::new(geyser.tile_x, geyser.tile_y);
        let region = surface::expand(URect { min: center, max: center + UVec2::ONE }, reach, size);
        let tiles = (region.min.y..region.max.y).flat_map(move |y| (region.min.x..region.max.x).map(move |x| UVec2::new(x, y)));
        tiles.map(move |tile| (tile, (tile.as_vec2() - center.as_vec2()).length_squared()))
    });
    for (tile, distance2) in stamps.filter(|&(_, distance2)| distance2 <= TUNDRA_RADIUS_TILES * TUNDRA_RADIUS_TILES) {
        let index = (tile.y * size.x + tile.x) as usize;
        mask.cells[index] = BiomeKind::Tundra;
        mask.tundra_inner[index] |= distance2 <= inner * inner;
    }
    mask
}

pub(super) struct EntitySpawnAssets<'a> {
    pub images: &'a mut Assets<Image>,
    pub meshes: &'a mut Assets<Mesh>,
    pub foliage_materials: &'a mut Assets<foliage::FoliageMaterial>,
    pub foliage_settings: foliage::FoliageSettings,
}

/// Spawns entities whose sprites loaded, foliage through its instanced renderer when it can.
pub(super) fn spawn_entities(
    commands: &mut Commands,
    parent: Entity,
    level: &CompiledLevel,
    placements: &[EntityPlacement],
    assets: &mut EntitySpawnAssets<'_>,
) -> usize {
    let tile_size = level.tile_size();
    let (foliage, sprites): (Vec<_>, Vec<_>) = placements
        .iter()
        .filter(|placement| assets.images.contains(&placement.handle))
        .partition(|placement| foliage::is_foliage_kind(&placement.kind));
    for placement in &sprites {
        spawn_entity(commands, parent, level, tile_size, placement);
    }
    let foliage_spawned = foliage::spawn_foliage(
        commands,
        parent,
        level,
        &foliage,
        assets.images,
        assets.meshes,
        assets.foliage_materials,
        assets.foliage_settings,
    )
    .unwrap_or_else(|error| {
        warn!("{error}; spawning foliage as regular sprites");
        for placement in &foliage {
            spawn_entity(commands, parent, level, tile_size, placement);
        }
        foliage.len()
    });
    sprites.len() + foliage_spawned
}

fn spawn_entity(commands: &mut Commands, parent: Entity, level: &CompiledLevel, tile_size: f32, placement: &EntityPlacement) {
    let bottom = (level.height as f32 - placement.tile_y as f32 - 1.0) * tile_size;
    let anchor = Vec3::new((placement.tile_x as f32 + 0.5) * tile_size, bottom, ENTITY_Z);
    let mut entity = commands.spawn((
        Sprite { image: placement.handle.clone(), flip_x: placement.flip_x, flip_y: placement.flip_y, ..default() },
        Anchor::BOTTOM_CENTER,
        Transform::from_translation(anchor + placement.offset_px.as_vec2().extend(placement.z_offset)),
        StateDespawnMarker,
        Name::new(format!("Entity:{}@{},{}", placement.kind, placement.tile_x, placement.tile_y)),
        ChildOf(parent),
    ));
    if let Some(definition) = placement.harvest {
        entity.insert(harvestable_materials::HarvestablePlant::from_definition(definition));
    }
    if let Some(definition) = placement.ore {
        entity.insert(harvestable_materials::HarvestableOre::from_definition(definition));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exterior_tops_are_excluded_without_removing_interior_ledges() {
        let mut level = CompiledLevel { width: 96, height: 192, terrain: vec![false; 96 * 192], ..default() };
        level.terrain[4 * 96..5 * 96].fill(true);
        level.terrain[100 * 96..101 * 96].fill(true);
        let occupancy = LevelChunkOccupancyMask { chunks_w: 1, chunks_h: 2, occupied: vec![true, true] };
        let area = placement_area(&level, Some(&occupancy));
        assert!(!area[3 * 96 + 48]);
        assert!(area[99 * 96 + 48]);
        assert!(area[20 * 96 + 48]);
        let isolated = LevelChunkOccupancyMask { occupied: vec![false, true], ..occupancy };
        let area = placement_area(&level, Some(&isolated));
        assert!(!area[20 * 96 + 48]);
        assert!(!area[99 * 96 + 48]);
        assert!(area[120 * 96 + 48]);
    }

    #[test]
    fn depth_tundra_uses_tundra_content_even_without_geysers() {
        let level = CompiledLevel { width: 2, height: 1, ..default() };
        let base = LevelBiomeMask { width: 2, height: 1, cells: vec![BiomeKind::Ice, BiomeKind::Tundra], tundra_inner: vec![] };
        let mask = build_tundra_mask_from_geysers(&level, &[], Some(&base));
        assert_eq!(mask.tundra_inner, [false, true]);
    }
}
