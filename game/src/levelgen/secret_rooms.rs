//! Secret rooms: rule images carved into the terrain, hidden behind covers until their
//! breakable wall falls.
//!
//! A rule PNG is centered on the tile it is placed at. Its black, white, `#5b666f` and
//! `#4dfbff` pixels are the room, all carved out: white is the breakable wall, `#5b666f`
//! also gets manual near background and `#4dfbff` marks ore. Green pixels require terrain;
//! red pixels are the entrance, which must be open cave without spikes.

use super::*;

/// Directory containing secret room rule PNGs.
pub(super) const SECRET_ROOMS_DIR: &str = "assets/levels/secret_rooms";

/// What a room tile is before the room opens and it becomes air.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum RoomTile {
    Interior,
    BreakableWall,
    NearBackground,
    OreMarker,
}

#[derive(Clone, Debug)]
pub(super) struct SecretRoomRule {
    pub name: String,
    /// The room's tiles by offset from the rule's center, in image scan order.
    pub room: Vec<(IVec2, RoomTile)>,
    pub require_terrain: Vec<IVec2>,
    /// The entrance.
    pub forbid_terrain: Vec<IVec2>,
    /// Whether the room may also be placed mirrored.
    pub ruleflip_x: bool,
}

/// The rules, loaded once and sorted by name so placement is deterministic.
#[derive(Resource, Default)]
pub(super) struct SecretRoomLibrary {
    rules: Vec<SecretRoomRule>,
    loaded: bool,
}

pub(super) fn load_secret_rooms(mut library: ResMut<SecretRoomLibrary>) {
    if std::mem::replace(&mut library.loaded, true) {
        return;
    }
    let Ok(entries) = fs::read_dir(SECRET_ROOMS_DIR)
        .inspect_err(|err| warn!("Failed to read secret rooms directory {SECRET_ROOMS_DIR}: {err}"))
    else {
        return;
    };
    let pngs = entries.flatten().map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext.eq_ignore_ascii_case("png")));
    let mut rules: Vec<_> = pngs.filter_map(|path| load_rule(&path)
        .inspect_err(|err| warn!("Failed to load secret room rule {}: {err}", path.display())).ok()).collect();
    rules.sort_by(|a, b| a.name.cmp(&b.name));
    if !rules.is_empty() {
        info!("Loaded {} secret room rules from {SECRET_ROOMS_DIR}", rules.len());
    }
    library.rules = rules;
}

pub(super) fn load_rule(path: &Path) -> Result<SecretRoomRule, String> {
    let name = path.file_stem().and_then(|stem| stem.to_str()).unwrap_or_default();
    let lower = name.to_ascii_lowercase();
    parse_rule(&mapgen::load_png(path)?, name, lower.contains("ruleflip-x") || lower.contains("ruleflip_x"))
}

fn parse_rule(image: &Image, name: &str, ruleflip_x: bool) -> Result<SecretRoomRule, String> {
    let data = image.data.as_deref().ok_or("image has no pixel data")?;
    let width = image.width() as i32;
    let center = UVec2::new(image.width() / 2, image.height() / 2).as_ivec2();
    let mut rule = SecretRoomRule { name: name.to_owned(), room: Vec::new(), require_terrain: Vec::new(),
        forbid_terrain: Vec::new(), ruleflip_x };
    let pixels = data.as_chunks::<4>().0.iter().take((image.width() * image.height()) as usize);
    for (index, pixel) in pixels.enumerate().filter(|(_, pixel)| pixel[3] >= 128) {
        let offset = IVec2::new(index as i32 % width, index as i32 / width) - center;
        match [pixel[0], pixel[1], pixel[2]] {
            [0, 0, 0] => rule.room.push((offset, RoomTile::Interior)),
            [0xFF, 0xFF, 0xFF] => rule.room.push((offset, RoomTile::BreakableWall)),
            compiler::COLOR_NEAR_BG_MANUAL => rule.room.push((offset, RoomTile::NearBackground)),
            compiler::COLOR_ORE_MARKER => rule.room.push((offset, RoomTile::OreMarker)),
            compiler::COLOR_RULE_REQUIRE => rule.require_terrain.push(offset),
            compiler::COLOR_RULE_FORBID => rule.forbid_terrain.push(offset),
            _ => {},
        }
    }
    if rule.room.is_empty() {
        return Err("no room pixels: black, white, #5b666f or #4dfbff".into());
    }
    Ok(rule)
}

/// A carved room.
#[derive(Clone, Debug)]
pub(super) struct SecretRoomPlacement {
    pub name: String,
    /// The room's level tiles, which lie inside the level: rules match only on terrain.
    pub room: Vec<(IVec2, RoomTile)>,
}

impl SecretRoomPlacement {
    pub fn tiles(&self) -> impl Iterator<Item = IVec2> + Clone + '_ {
        self.room.iter().map(|&(tile, _)| tile)
    }

    /// The room's tiles of one kind, in rule scan order.
    pub fn tiles_of(&self, kind: RoomTile) -> impl Iterator<Item = IVec2> + '_ {
        self.room.iter().filter(move |&&(_, tile_kind)| tile_kind == kind).map(|&(tile, _)| tile)
    }

    pub fn bounds(&self) -> URect {
        tile_rect(self.tiles()).expect("rules have room tiles")
    }
}

/// The smallest rectangle holding `tiles`.
pub(super) fn tile_rect(tiles: impl Iterator<Item = IVec2>) -> Option<URect> {
    tiles.map(|tile| IRect::from_corners(tile, tile + 1)).reduce(|a, b| a.union(b)).map(|rect| rect.as_urect())
}

/// How far, in tiles, carving a room reshades the ground around it (the reach of edge
/// shading, rounded up) plus a tile of margin. A room's cover hides this border too.
pub(super) fn cover_border_tiles() -> u32 {
    let reach_px = GROUND_DARKEN_START_PX as f32 + GROUND_DARKEN_RANGE_PX;
    (reach_px / GROUND_TILE_ART_SIZE_PX as f32).ceil() as u32 + 1
}

pub(super) struct CarvingResult {
    pub terrain: Vec<bool>,
    pub placements: Vec<SecretRoomPlacement>,
}

/// Places each rule at most once and carves its room out of the terrain.
///
/// Rooms stay two cover borders away from each other, from optional spikes and from
/// `crater`, the tiles the intro impact changes. So a room's carving never reaches another
/// room's cover, and no cover ever hides art that changes during play.
pub(super) fn carve_secret_rooms(level: &CompiledLevel, library: &SecretRoomLibrary, crater: &[UVec2]) -> CarvingResult {
    let _timer = diagnostics::StageTimer::new("secret_room_carving");
    let size = UVec2::new(level.width, level.height);
    let index = |tile: UVec2| (tile.y * size.x + tile.x) as usize;
    let gap = 2 * cover_border_tiles();
    let mut changing = level.optional_spike_tiles.clone();
    changing.resize(level.terrain.len(), false);
    for &tile in crater {
        changing[index(tile)] = true;
    }
    let mut reserved = dilate(&changing, size, gap);
    let mut terrain = level.terrain.clone();
    let mut placements = Vec::new();
    for rule in &library.rules {
        let Some((anchor, flip)) = best_placement(level, &reserved, rule) else { continue; };
        let room: Vec<_> = rule.room.iter().map(|&(offset, kind)| (anchor + mirrored(offset, flip), kind)).collect();
        let mut footprint = vec![false; terrain.len()];
        for tile in room.iter().map(|&(tile, _)| index(tile.as_uvec2())) {
            terrain[tile] = false;
            footprint[tile] = true;
        }
        reserved.iter_mut().zip(dilate(&footprint, size, gap)).for_each(|(reserved, near)| *reserved |= near);
        info!("Secret room '{}' carved at {anchor}, flipped={flip}", rule.name);
        placements.push(SecretRoomPlacement { name: rule.name.clone(), room });
    }
    CarvingResult { terrain, placements }
}

/// The fitting anchor and orientation with the lowest hash, the first one on ties.
fn best_placement(level: &CompiledLevel, reserved: &[bool], rule: &SecretRoomRule) -> Option<(IVec2, bool)> {
    let flips: &[bool] = if rule.ruleflip_x { &[false, true] } else { &[false] };
    let anchors = (0..level.height as i32).flat_map(|y| (0..level.width as i32).map(move |x| IVec2::new(x, y)));
    let mut best: Option<(u64, IVec2, bool)> = None;
    for (anchor, flip) in anchors.flat_map(|anchor| flips.iter().map(move |&flip| (anchor, flip))) {
        let hash = anchor_hash(anchor, flip);
        if best.is_none_or(|(lowest, ..)| hash < lowest) && fits(level, reserved, rule, anchor, flip) {
            best = Some((hash, anchor, flip));
        }
    }
    best.map(|(_, anchor, flip)| (anchor, flip))
}

/// Rooms go where the anchor hash is lowest; a mirrored room ranks just after the unmirrored one.
fn anchor_hash(anchor: IVec2, flip: bool) -> u64 {
    let position = ((anchor.x as i64 as u64) << 32) ^ ((anchor.y as i64 as u64) << 16);
    super::hash::mix64(position ^ 0xCAFE_BABE_DEAD_BEEF).wrapping_add(u64::from(flip))
}

/// Whether the room fits: an open entrance in the authored cave, the required tiles solid,
/// and the room itself unreserved terrain.
fn fits(level: &CompiledLevel, reserved: &[bool], rule: &SecretRoomRule, anchor: IVec2, flip: bool) -> bool {
    let at = |offset: IVec2| {
        let tile = anchor + mirrored(offset, flip);
        let inside = tile.cmpge(IVec2::ZERO).all() && tile.cmplt(UVec2::new(level.width, level.height).as_ivec2()).all();
        inside.then(|| (tile.y as u32 * level.width + tile.x as u32) as usize)
    };
    let open = |offset: &IVec2| at(*offset).is_some_and(|index| !level.terrain[index] && level.spike_tiles.get(index) != Some(&true));
    let solid = |offset: &IVec2| at(*offset).is_some_and(|index| level.terrain[index]);
    let free = |(offset, _): &(IVec2, RoomTile)| at(*offset).is_some_and(|index| level.terrain[index] && !reserved[index]);
    rule.forbid_terrain.iter().all(open) && rule.require_terrain.iter().all(solid) && rule.room.iter().all(free)
}

fn mirrored(offset: IVec2, flip: bool) -> IVec2 {
    if flip { IVec2::new(-offset.x, offset.y) } else { offset }
}

/// Marks every tile within `radius` tiles of a marked one along both axes.
pub(super) fn dilate(mask: &[bool], size: UVec2, radius: u32) -> Vec<bool> {
    let (width, height, radius) = (size.x as usize, size.y as usize, radius as usize);
    let rows = dilate_lines(mask, (width, 1), (height, width), radius);
    dilate_lines(&rows, (height, width), (width, 1), radius)
}

/// Dilates each line of `mask` along itself: `len` cells `step` apart, in `lines` lines
/// whose starts lie `stride` apart.
fn dilate_lines(mask: &[bool], (len, step): (usize, usize), (lines, stride): (usize, usize), radius: usize) -> Vec<bool> {
    let mut out = vec![false; mask.len()];
    let mut marked = vec![0; len + 1];
    for line in 0..lines {
        let cell = |i: usize| line * stride + i * step;
        for i in 0..len {
            marked[i + 1] = marked[i] + usize::from(mask[cell(i)]);
        }
        for i in 0..len {
            out[cell(i)] = marked[(i + radius + 1).min(len)] > marked[i.saturating_sub(radius)];
        }
    }
    out
}

#[cfg(test)]
#[path = "secret_room_tests.rs"]
mod tests;
