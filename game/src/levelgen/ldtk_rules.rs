//! LDtk auto-tile rule loader and matcher.
//!
//! Parses LDtk project files to extract auto-tile rules for terrain and background
//! layers, then applies those rules during level generation.

use bevy::prelude::*;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;

use super::hash::tile_hash;

/// Pattern value meanings in LDtk:
/// - 0 = any/ignore
/// - positive N = require IntGrid value N
/// - 1000001 = require any IntGrid value present
/// - -1000001 = require empty (no IntGrid value)
/// - negative -N (where N < 1000000) = forbid IntGrid value N
const LDTK_ANY_VALUE: i32 = 1_000_001;
const LDTK_EMPTY: i32 = -1_000_001;

/// A single auto-tile rule from LDtk.
#[derive(Clone, Debug)]
pub struct LdtkRule {
    pub uid: u32,
    pub active: bool,
    /// Size of the pattern grid (e.g., 3 for 3x3, 5 for 5x5).
    pub size: u8,
    /// Flat array of pattern values, row-major from top-left.
    /// For size=3: [0,1,2,3,4,5,6,7,8] maps to:
    ///   [0][1][2]
    ///   [3][4][5]  (4 is center)
    ///   [6][7][8]
    pub pattern: Vec<i32>,
    /// Tile IDs to use when matched (can have multiple variants for random selection).
    pub tile_ids: Vec<Vec<u32>>,
    /// Whether to try horizontally flipped pattern.
    pub flip_x: bool,
    /// Whether to try vertically flipped pattern.
    pub flip_y: bool,
    /// Probability (0.0 to 1.0).
    pub chance: f32,
    /// Stop processing further rules if this one matches.
    pub break_on_match: bool,
    checks: [Vec<(i32, i32, i32)>; 4],
}

/// A group of related rules.
#[derive(Clone, Debug)]
pub struct LdtkRuleGroup {
    pub name: String,
    pub active: bool,
    pub rules: Vec<LdtkRule>,
}

/// A tileset definition.
#[derive(Clone, Debug)]
pub struct LdtkTileset {
    pub uid: u32,
    pub rel_path: String,
    pub tile_grid_size: u32,
    pub px_width: u32,
}

/// An IntGrid layer with auto-tile rules.
#[derive(Clone, Debug)]
pub struct LdtkAutoLayer {
    pub identifier: String,
    pub tileset_def_uid: Option<u32>,
    pub rule_groups: Vec<LdtkRuleGroup>,
}

/// All LDtk data needed for tile rendering.
#[derive(Resource, Default, Clone)]
pub struct LdtkRuleData {
    pub tilesets: HashMap<u32, LdtkTileset>,
    pub tileset_images: HashMap<u32, Handle<Image>>,
    /// Map layer identifier to layer data.
    pub layers: HashMap<String, LdtkAutoLayer>,
    pub loaded: bool,
}

pub(super) struct TileLayerAssets<'a> {
    pub layer: &'a LdtkAutoLayer,
    pub image: &'a Image,
    pub tile_size: u32,
    pub tiles_per_row: u32,
}

impl LdtkRuleData {
    pub(super) fn resolve_layer<'a>(&'a self, name: &str, images: &'a Assets<Image>, expected: Option<u32>) -> Option<TileLayerAssets<'a>> {
        let layer = self.layers.get(name)?;
        let uid = layer.tileset_def_uid?;
        let tileset = self.tilesets.get(&uid)?;
        let image = images.get(self.tileset_images.get(&uid)?)?;
        if tileset.tile_grid_size == 0 || expected.is_some_and(|size| tileset.tile_grid_size != size) {
            warn!("LDtk {name} tile size {} does not match expected {expected:?}", tileset.tile_grid_size);
            return None;
        }
        Some(TileLayerAssets { layer, image, tile_size: tileset.tile_grid_size,
            tiles_per_row: tileset.px_width / tileset.tile_grid_size })
    }
}

/// Load LDtk project file and extract rule data.
pub fn load_ldtk_rules(
    ldtk_path: &Path,
    asset_server: &AssetServer,
) -> Result<LdtkRuleData, String> {
    let contents = fs::read_to_string(ldtk_path).map_err(|e| {
        format!(
            "Failed to read LDtk file {}: {}",
            ldtk_path.display(),
            e
        )
    })?;

    let json: serde_json::Value = serde_json::from_str(&contents)
        .map_err(|e| format!("Failed to parse LDtk JSON: {e}"))?;

    let mut data = LdtkRuleData::default();

    parse_ldtk_defs(&json, &mut data);

    // Load tileset images
    let ldtk_dir = ldtk_path.parent().unwrap_or(Path::new("."));
    for (uid, ts) in &data.tilesets {
        let img_path = ldtk_dir.join(&ts.rel_path);
        let rel_path = img_path
            .strip_prefix("assets")
            .unwrap_or(&img_path)
            .to_string_lossy()
            .to_string();
        let handle: Handle<Image> = asset_server.load(&rel_path);
        data.tileset_images.insert(*uid, handle);
    }

    data.loaded = true;
    info!(
        "Loaded LDtk rules: {} tilesets, {} layers",
        data.tilesets.len(),
        data.layers.len()
    );

    Ok(data)
}

fn parse_ldtk_defs(json: &serde_json::Value, data: &mut LdtkRuleData) {
    let Some(defs) = json.get("defs") else {
        return;
    };

    if let Some(tilesets) = defs.get("tilesets").and_then(|v| v.as_array()) {
        parse_tilesets(tilesets, data);
    }

    if let Some(layers) = defs.get("layers").and_then(|v| v.as_array()) {
        parse_layers(layers, data);
    }
}

fn parse_tilesets(tilesets: &[serde_json::Value], data: &mut LdtkRuleData) {
    for ts_val in tilesets {
        let Some(ts) = parse_tileset(ts_val) else {
            continue;
        };

        data.tilesets.insert(ts.uid, ts);
    }
}

fn parse_layers(layers: &[serde_json::Value], data: &mut LdtkRuleData) {
    for layer_val in layers {
        let Some(layer) = parse_layer(layer_val) else {
            continue;
        };

        data.layers.insert(layer.identifier.clone(), layer);
    }
}

fn parse_tileset(val: &serde_json::Value) -> Option<LdtkTileset> {
    Some(LdtkTileset {
        uid: val.get("uid")?.as_u64()? as u32,
        rel_path: val.get("relPath")?.as_str()?.to_string(),
        tile_grid_size: val.get("tileGridSize")?.as_u64()? as u32,
        px_width: val.get("pxWid")?.as_u64()? as u32,
    })
}

fn parse_layer(val: &serde_json::Value) -> Option<LdtkAutoLayer> {
    let layer_type = val.get("__type")?.as_str()?;
    // Only parse IntGrid layers (they have auto-tile rules)
    if layer_type != "IntGrid" {
        return None;
    }

    let identifier = val.get("identifier")?.as_str()?.to_string();
    let tileset_def_uid = val
        .get("tilesetDefUid")
        .and_then(serde_json::Value::as_u64)
        .map(|v| v as u32);

    // Parse rule groups
    let mut rule_groups = Vec::new();
    if let Some(groups) = val.get("autoRuleGroups").and_then(|v| v.as_array()) {
        rule_groups.extend(groups.iter().filter_map(parse_rule_group));
    }

    Some(LdtkAutoLayer {
        identifier,
        tileset_def_uid,
        rule_groups,
    })
}

fn parse_rule_group(val: &serde_json::Value) -> Option<LdtkRuleGroup> {
    let name = val
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let active = val.get("active").and_then(serde_json::Value::as_bool).unwrap_or(true);

    let mut rules = Vec::new();
    if let Some(rules_arr) = val.get("rules").and_then(|v| v.as_array()) {
        rules.extend(rules_arr.iter().filter_map(parse_rule));
    }

    Some(LdtkRuleGroup {
        name,
        active,
        rules,
    })
}

fn parse_rule(val: &serde_json::Value) -> Option<LdtkRule> {
    let uid = val.get("uid")?.as_u64()? as u32;
    let active = val.get("active").and_then(serde_json::Value::as_bool).unwrap_or(true);
    let size = val.get("size")?.as_u64()? as u8;

    // Parse pattern
    let pattern: Vec<i32> = val
        .get("pattern")?
        .as_array()?
        .iter()
        .filter_map(|v| v.as_i64().map(|x| x as i32))
        .collect();

    let tile_ids = parse_tile_rect_ids(val);

    let flip_x = val.get("flipX").and_then(serde_json::Value::as_bool).unwrap_or(false);
    let flip_y = val.get("flipY").and_then(serde_json::Value::as_bool).unwrap_or(false);
    let chance =
        val.get("chance").and_then(serde_json::Value::as_f64).unwrap_or(1.0) as f32;
    let break_on_match = val
        .get("breakOnMatch")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(true);

    Some(LdtkRule {
        uid,
        active,
        size,
        checks: compile_pattern(&pattern, size),
        pattern,
        tile_ids,
        flip_x,
        flip_y,
        chance,
        break_on_match,
    })
}

fn parse_tile_rect_ids(val: &serde_json::Value) -> Vec<Vec<u32>> {
    let Some(tile_rects) = val.get("tileRectsIds").and_then(|v| v.as_array())
    else {
        return Vec::new();
    };

    tile_rects
        .iter()
        .filter_map(parse_tile_rect_variant)
        .collect()
}

fn parse_tile_rect_variant(val: &serde_json::Value) -> Option<Vec<u32>> {
    let ids = val.as_array()?;
    let variant_ids: Vec<u32> = ids
        .iter()
        .filter_map(|v| v.as_u64().map(|x| x as u32))
        .collect();
    if variant_ids.is_empty() {
        None
    } else {
        Some(variant_ids)
    }
}

/// Wall category for the Main layer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WallCategory {
    Wall0 = 18,
    Wall1 = 17,
    Wall2 = 19,
    Wall3 = 20,
}

pub const MAIN_SPIKES_INT_GRID_VALUE: i32 = 21;

impl WallCategory {
    pub fn as_int_grid_value(&self) -> i32 {
        *self as i32
    }
}

/// Fallback tile IDs for background wall categories.
#[derive(Clone, Debug, Default)]
pub struct BgFallbackTiles {
    pub near_wall: Option<u32>,
    pub main_wall: Option<u32>,
    pub mid_wall: Option<u32>,
    pub far_wall: Option<u32>,
}

impl BgFallbackTiles {
    /// Get fallback tile for a given IntGrid value.
    pub fn get(&self, int_grid_val: i32) -> Option<u32> {
        match int_grid_val {
            4 => self.near_wall,
            1 => self.main_wall,
            2 => self.mid_wall,
            3 => self.far_wall,
            _ => None,
        }
    }
}

/// Discover fallback tiles for background layers from LDtk catchall rules.
pub fn discover_bg_fallback_tiles(layer: &LdtkAutoLayer) -> BgFallbackTiles {
    let mut fallbacks = BgFallbackTiles::default();

    for group in &layer.rule_groups {
        if !group.active {
            continue;
        }
        for rule in &group.rules {
            apply_bg_fallback_rule(&mut fallbacks, rule);
        }
    }

    fallbacks
}

fn apply_bg_fallback_rule(fallbacks: &mut BgFallbackTiles, rule: &LdtkRule) {
    let Some((center_val, tile_id)) = catchall_rule_tile(rule) else {
        return;
    };

    match center_val {
        4 if fallbacks.near_wall.is_none() => fallbacks.near_wall = Some(tile_id),
        1 if fallbacks.main_wall.is_none() => fallbacks.main_wall = Some(tile_id),
        2 if fallbacks.mid_wall.is_none() => fallbacks.mid_wall = Some(tile_id),
        3 if fallbacks.far_wall.is_none() => fallbacks.far_wall = Some(tile_id),
        _ => {},
    }
}

fn catchall_rule_tile(rule: &LdtkRule) -> Option<(i32, u32)> {
    if !rule.active || rule.size != 1 || rule.pattern.len() != 1 {
        return None;
    }

    Some((rule.pattern[0], first_rule_tile_id(rule)?))
}

fn first_rule_tile_id(rule: &LdtkRule) -> Option<u32> {
    rule.tile_ids.first()?.first().copied()
}


fn distance_from_empty_with_limit(
    mask: &[bool],
    width: u32,
    height: u32,
    max_distance: u16,
) -> Vec<u16> {
    let mut dist: Vec<u16> = mask
        .iter()
        .map(|&occupied| if occupied { max_distance } else { 0 })
        .collect();
    let w = width as i32;
    let h = height as i32;

    for y in 0..h {
        for x in 0..w {
            update_forward_distance_cell(mask, &mut dist, w, x, y);
        }
    }

    for y in (0..h).rev() {
        for x in (0..w).rev() {
            update_backward_distance_cell(mask, &mut dist, w, h, x, y);
        }
    }

    dist
}

fn update_forward_distance_cell(
    mask: &[bool],
    dist: &mut [u16],
    w: i32,
    x: i32,
    y: i32,
) {
    let idx = (y * w + x) as usize;
    if !mask[idx] {
        return;
    }

    let mut d = dist[idx];
    if x > 0 {
        let left_idx = (y * w + x - 1) as usize;
        d = d.min(dist[left_idx].saturating_add(1));
    }
    if y > 0 {
        let top_idx = ((y - 1) * w + x) as usize;
        d = d.min(dist[top_idx].saturating_add(1));
    }
    dist[idx] = d;
}

fn update_backward_distance_cell(
    mask: &[bool],
    dist: &mut [u16],
    w: i32,
    h: i32,
    x: i32,
    y: i32,
) {
    let idx = (y * w + x) as usize;
    if !mask[idx] {
        return;
    }

    let mut d = dist[idx];
    if x < w - 1 {
        let right_idx = (y * w + x + 1) as usize;
        d = d.min(dist[right_idx].saturating_add(1));
    }
    if y < h - 1 {
        let bottom_idx = ((y + 1) * w + x) as usize;
        d = d.min(dist[bottom_idx].saturating_add(1));
    }
    dist[idx] = d;
}

fn compile_pattern(pattern: &[i32], size: u8) -> [Vec<(i32, i32, i32)>; 4] {
    std::array::from_fn(|variant| {
        if size == 0 {
            return Vec::new();
        }
        let size = size as i32;
        let mut checks: Vec<_> = pattern.iter().enumerate()
            .filter(|(_, value)| **value != 0)
            .map(|(index, &value)| {
                let dx = index as i32 % size - size / 2;
                let dy = index as i32 / size - size / 2;
                (dx * (1 - 2 * (variant as i32 & 1)),
                 dy * (1 - 2 * ((variant as i32 >> 1) & 1)), value)
            }).collect();
        checks.sort_by_key(|&(x, y, _)| x.abs() + y.abs());
        checks
    })
}

fn expected_matches_actual(expected: i32, actual: i32) -> bool {
    if expected == LDTK_ANY_VALUE {
        return actual != 0;
    }
    if expected == LDTK_EMPTY {
        return actual == 0;
    }
    if expected < 0 && expected > -1_000_000 {
        return actual != -expected;
    }
    if expected > 0 && expected < 1_000_000 {
        return actual == expected;
    }

    true
}

/// Result of applying rules at a position.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TilePlacement {
    pub tile_id: u32,
    pub flip_x: bool,
    pub flip_y: bool,
}

/// Fallback tile IDs for each wall category when no rule matches.
/// Discovered dynamically from the LDtk rules' catchall patterns.
#[derive(Clone, Debug, Default)]
pub struct WallFallbackTiles {
    pub wall0: Option<u32>,
    pub wall1: Option<u32>,
    pub wall2: Option<u32>,
    pub wall3: Option<u32>,
    pub spikes: Option<u32>,
}

impl WallFallbackTiles {
    /// Get fallback tile for a given IntGrid value.
    pub fn get(&self, int_grid_val: i32) -> Option<u32> {
        match int_grid_val {
            18 => self.wall0,
            17 => self.wall1,
            19 => self.wall2,
            20 => self.wall3,
            MAIN_SPIKES_INT_GRID_VALUE => self.spikes,
            _ => None,
        }
    }
}

/// Discover fallback tiles from LDtk rules by finding catchall rules (size=1).
///
/// Catchall rules have a single-cell pattern [N] where N is the wall type.
/// These are the most permissive rules and serve as fallbacks for interior cells.
pub fn discover_fallback_tiles(layer: &LdtkAutoLayer) -> WallFallbackTiles {
    let mut fallbacks = WallFallbackTiles::default();

    // Look for size=1 catchall rules with patterns like [18], [17], [19], [20]
    for group in &layer.rule_groups {
        if !group.active {
            continue;
        }
        for rule in &group.rules {
            apply_wall_fallback_rule(&mut fallbacks, rule);
        }
    }

    // The spike group does not have to define a size=1 catchall. Use the first
    // spike rule centered on value 21 as a last-resort tile so authored spike
    // cells never disappear if their neighborhood misses every specialized rule.
    if fallbacks.spikes.is_none() {
        fallbacks.spikes = discover_spike_fallback_tile(layer);
    }

    fallbacks
}

fn apply_wall_fallback_rule(fallbacks: &mut WallFallbackTiles, rule: &LdtkRule) {
    let Some((center_val, tile_id)) = catchall_rule_tile(rule) else {
        return;
    };

    match center_val {
        18 if fallbacks.wall0.is_none() => fallbacks.wall0 = Some(tile_id),
        17 if fallbacks.wall1.is_none() => fallbacks.wall1 = Some(tile_id),
        19 if fallbacks.wall2.is_none() => fallbacks.wall2 = Some(tile_id),
        20 if fallbacks.wall3.is_none() => fallbacks.wall3 = Some(tile_id),
        MAIN_SPIKES_INT_GRID_VALUE if fallbacks.spikes.is_none() => {
            fallbacks.spikes = Some(tile_id)
        },
        _ => {},
    }
}

fn discover_spike_fallback_tile(layer: &LdtkAutoLayer) -> Option<u32> {
    for group in &layer.rule_groups {
        if !group.active || !group.name.to_lowercase().contains("spikes") {
            continue;
        }
        if let Some(tile_id) = first_centered_spike_rule_tile(group) {
            return Some(tile_id);
        }
    }

    None
}

fn first_centered_spike_rule_tile(group: &LdtkRuleGroup) -> Option<u32> {
    for rule in &group.rules {
        if centered_spike_rule_matches(rule) {
            return first_rule_tile_id(rule);
        }
    }

    None
}

fn centered_spike_rule_matches(rule: &LdtkRule) -> bool {
    if !rule.active || first_rule_tile_id(rule).is_none() {
        return false;
    }

    let center_idx = (rule.pattern.len().saturating_sub(1)) / 2;
    rule.pattern.get(center_idx).copied() == Some(MAIN_SPIKES_INT_GRID_VALUE)
}

struct ApplyRulesContext<'a> {
    int_grid: &'a [i32],
    width: u32,
    height: u32,
    det_seed: u64,
}

impl ApplyRulesContext<'_> {
    /// The IntGrid value at a cell; cells outside the grid are empty.
    fn value_at(&self, x: i32, y: i32) -> i32 {
        if x < 0 || y < 0 || x >= self.width as i32 || y >= self.height as i32 {
            return 0;
        }
        self.int_grid[(y as u32 * self.width + x as u32) as usize]
    }

    /// Whether `rule`, optionally mirrored, matches around a cell.
    fn matches(&self, rule: &LdtkRule, cx: i32, cy: i32, flip_x: bool, flip_y: bool) -> bool {
        let variant = usize::from(flip_x) | (usize::from(flip_y) << 1);
        rule.checks[variant]
            .iter()
            .all(|&(dx, dy, expected)| expected_matches_actual(expected, self.value_at(cx + dx, cy + dy)))
    }
}

type CenterRules<'a> = HashMap<i32, Vec<Vec<&'a LdtkRule>>>;

fn prepare_center_rules<'a>(
    layer: &'a LdtkAutoLayer, priorities: &[Option<&[&str]>],
    values: &HashSet<i32>,
) -> CenterRules<'a> {
    let stages: Vec<Vec<_>> = priorities.iter().map(|allowed| {
        layer.rule_groups.iter().filter(|group| rule_group_is_allowed(group, *allowed))
            .flat_map(|group| &group.rules)
            .filter(|rule| rule.active && !rule.tile_ids.is_empty()).collect()
    }).collect();
    values.iter().map(|&value| {
        let rules = stages.iter().map(|stage| stage.iter().copied().filter(|rule| {
            let center = (rule.size as usize / 2) * (rule.size as usize + 1);
            expected_matches_actual(rule.pattern.get(center).copied().unwrap_or(0), value)
        }).collect()).collect();
        (value, rules)
    }).collect()
}

/// Prepared matching can be queried only where a biome or fallback is needed.
pub(super) struct RuleProgram<'a> {
    context: ApplyRulesContext<'a>,
    centers: CenterRules<'a>,
    include_empty: bool,
}

impl<'a> RuleProgram<'a> {
    /// Terrain tiles in priority passes:
    /// 1. **spikes**: authored hazard cells, highest priority
    /// 2. **wall0-3**: edge tiles at terrain boundaries
    /// 3. **fog1-3**: interior transitions between wall types
    /// 4. **fog_add**: special edge and island cases, as a fallback
    ///
    /// Native LDtk runs fog_add before fog1-3, but its island patterns can match interior
    /// cells and steal them from the wall-type-specific fog rules.
    pub(super) fn terrain(grid: &'a [i32], size: UVec2, layer: &'a LdtkAutoLayer, seed: u64) -> Self {
        Self::new(grid, size, layer, seed, &[
            Some(&["spikes"]), Some(&["wall0", "wall1", "wall2", "wall3"]),
            Some(&["fog1", "fog2", "fog3"]), Some(&["fog_add"]),
        ], false)
    }

    pub(super) fn background(grid: &'a [i32], size: UVec2, layer: &'a LdtkAutoLayer, seed: u64) -> Self {
        Self::new(grid, size, layer, seed, &[Some(&["fog"]), Some(&["wall"])], false)
    }

    pub(super) fn grass(grid: &'a [i32], size: UVec2, layer: &'a LdtkAutoLayer, seed: u64) -> Self {
        Self::new(grid, size, layer, seed, &[Some(&["grass"])], true)
    }

    fn new(grid: &'a [i32], size: UVec2, layer: &'a LdtkAutoLayer, seed: u64,
        priorities: &[Option<&[&str]>], include_empty: bool,
    ) -> Self {
        let values = grid.iter().copied().collect();
        Self { context: ApplyRulesContext { int_grid: grid, width: size.x, height: size.y, det_seed: seed },
            centers: prepare_center_rules(layer, priorities, &values), include_empty }
    }

    pub(super) fn at(&self, index: usize) -> Option<TilePlacement> {
        let value = *self.context.int_grid.get(index)?;
        if value == 0 && !self.include_empty { return None; }
        let x = (index % self.context.width as usize) as i32;
        let y = (index / self.context.width as usize) as i32;
        evaluate_priority_stages(&self.context, &self.centers[&value], x, y)
    }

    /// Every cell's placement, for comparing against reference implementations.
    #[cfg(test)]
    fn cells(&self) -> Vec<Option<TilePlacement>> {
        (0..self.context.int_grid.len()).map(|index| self.at(index)).collect()
    }
}

fn evaluate_priority_stages(
    ctx: &ApplyRulesContext<'_>, stages: &[Vec<&LdtkRule>], x: i32, y: i32,
) -> Option<TilePlacement> {
    for stage in stages {
        let placement = evaluate_stage(ctx, stage, x, y);
        if placement.is_some() {
            return placement;
        }
    }
    None
}

fn rule_group_is_allowed(
    group: &LdtkRuleGroup,
    allowed_groups: Option<&[&str]>,
) -> bool {
    if !group.active {
        return false;
    }

    let Some(allowed) = allowed_groups else {
        return true;
    };

    let group_name_lower = group.name.to_lowercase();
    allowed
        .iter()
        .any(|allowed| group_name_lower.contains(&allowed.to_lowercase()))
}

fn apply_rule_to_cell(
    ctx: &ApplyRulesContext<'_>,
    rule: &LdtkRule,
    cx: i32,
    cy: i32,
    placement: &mut Option<TilePlacement>,
) -> bool {
    for &(fx, fy) in flip_combinations(rule) {
        let Some(tile) = tile_placement_for_rule(ctx, rule, cx, cy, fx, fy)
        else {
            continue;
        };

        *placement = Some(tile);
        if rule.break_on_match {
            return true;
        }
    }

    false
}

fn flip_combinations(rule: &LdtkRule) -> &'static [(bool, bool)] {
    if rule.flip_x && rule.flip_y {
        &[
            (false, false),
            (true, false),
            (false, true),
            (true, true),
        ]
    } else if rule.flip_x {
        &[(false, false), (true, false)]
    } else if rule.flip_y {
        &[(false, false), (false, true)]
    } else {
        &[(false, false)]
    }
}

fn tile_placement_for_rule(
    ctx: &ApplyRulesContext<'_>,
    rule: &LdtkRule,
    cx: i32,
    cy: i32,
    flip_x: bool,
    flip_y: bool,
) -> Option<TilePlacement> {
    if !ctx.matches(rule, cx, cy, flip_x, flip_y) {
        return None;
    }

    if !rule_chance_passes(ctx, rule, cx, cy) {
        return None;
    }

    Some(TilePlacement {
        tile_id: choose_rule_tile_id(ctx, rule, cx, cy),
        flip_x,
        flip_y,
    })
}

fn rule_chance_passes(
    ctx: &ApplyRulesContext<'_>,
    rule: &LdtkRule,
    cx: i32,
    cy: i32,
) -> bool {
    if rule.chance >= 1.0 {
        return true;
    }

    let hash = tile_hash(cx, cy, rule.uid, ctx.det_seed);
    let prob = (hash & 0xFFFF) as f32 / 65535.0;
    prob < rule.chance
}

fn choose_rule_tile_id(
    ctx: &ApplyRulesContext<'_>,
    rule: &LdtkRule,
    cx: i32,
    cy: i32,
) -> u32 {
    let variant_idx = if rule.tile_ids.len() > 1 {
        let hash = tile_hash(cx, cy, rule.uid.wrapping_add(1000), ctx.det_seed);
        (hash as usize) % rule.tile_ids.len()
    } else {
        0
    };

    let tile_variant = &rule.tile_ids[variant_idx];
    if tile_variant.len() > 1 {
        let hash = tile_hash(cx, cy, rule.uid.wrapping_add(2000), ctx.det_seed);
        tile_variant[(hash as usize) % tile_variant.len()]
    } else {
        tile_variant[0]
    }
}

/// Generate wall category grid from terrain and mosaic masks.
///
/// Maps:
/// - mosaic1 → Wall1 (value 17)
/// - mosaic2 → Wall2 (value 19)
/// - mosaic3 → Wall3 (value 20)
/// - remaining terrain → Wall0 (value 18)
pub fn generate_wall_categories(
    terrain: &[bool],
    mosaic1: &[bool],
    mosaic2: &[bool],
    mosaic3: &[bool],
) -> Vec<i32> {
    let len = terrain.len();
    let mut result = vec![0i32; len];

    for i in 0..len {
        if !terrain[i] { continue; }
        if mosaic3[i] {
            result[i] = WallCategory::Wall3.as_int_grid_value();
        } else if mosaic2[i] {
            result[i] = WallCategory::Wall2.as_int_grid_value();
        } else if mosaic1[i] {
            result[i] = WallCategory::Wall1.as_int_grid_value();
        } else if terrain[i] {
            result[i] = WallCategory::Wall0.as_int_grid_value();
        }
    }

    result
}

/// Generate wall categories based on distance from terrain edges.
///
/// This is used when mosaic masks are empty (no PNG rules loaded).
/// Creates coherent "onion layer" regions based on Manhattan distance from edge:
/// - Distance 0 (edge cells) → Wall0 (value 18)
/// - Distance 1 → Wall1 (value 17)  
/// - Distance 2 → Wall2 (value 19)
/// - Distance 3+ → Wall3 (value 20)
pub fn generate_wall_categories_from_distance(
    terrain: &[bool],
    width: u32,
    height: u32,
) -> Vec<i32> {
    let len = terrain.len();
    let dist = distance_from_empty_with_limit(terrain, width, height, u16::MAX / 2);

    // Convert distances to wall categories
    let mut result = vec![0i32; len];
    for i in 0..len {
        result[i] = wall_category_for_terrain_distance(terrain[i], dist[i]);
    }

    result
}

fn wall_category_for_terrain_distance(is_terrain: bool, dist: u16) -> i32 {
    if !is_terrain {
        return 0;
    }

    match dist {
        0 | 1 => WallCategory::Wall0.as_int_grid_value(),
        2 | 3 => WallCategory::Wall1.as_int_grid_value(),
        4 | 5 => WallCategory::Wall2.as_int_grid_value(),
        _ => WallCategory::Wall3.as_int_grid_value(),
    }
}

/// Check if mosaic masks have any content.
pub fn mosaic_masks_empty(
    mosaic1: &[bool],
    mosaic2: &[bool],
    mosaic3: &[bool],
) -> bool {
    !mosaic1.iter().any(|&b| b)
        && !mosaic2.iter().any(|&b| b)
        && !mosaic3.iter().any(|&b| b)
}

fn evaluate_stage(ctx: &ApplyRulesContext<'_>, rules: &[&LdtkRule], x: i32, y: i32) -> Option<TilePlacement> {
    let mut placement = None;
    for rule in rules {
        if apply_rule_to_cell(ctx, rule, x, y, &mut placement) { break; }
    }
    placement
}

#[cfg(test)]
#[path = "ldtk_rule_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "throughput_tests.rs"]
mod throughput_tests;

#[cfg(test)]
#[path = "biome_rule_tests.rs"]
mod biome_tests;

#[cfg(test)]
pub(super) fn load_test_artwork(images: &mut Assets<Image>) -> LdtkRuleData {
    let directory = Path::new("assets/levels/tiles/ldtk_rules");
    let json = serde_json::from_slice(&fs::read(directory.join("level.01_test.ldtk")).unwrap()).unwrap();
    let mut data = LdtkRuleData::default();
    parse_ldtk_defs(&json, &mut data);
    for (&uid, tileset) in &data.tilesets {
        data.tileset_images.insert(uid, images.add(super::mapgen::load_png(&directory.join(&tileset.rel_path)).unwrap()));
    }
    data
}
