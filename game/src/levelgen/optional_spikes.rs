use super::*;
use crate::game::combat::{KillCount, PlanetImmunityLevel};
use crate::game::gentstate::Dead;
use crate::game::player::Player;
use bevy::sprite_render::{ColorMaterial, MeshMaterial2d};
use std::collections::{HashMap, HashSet, VecDeque};

pub(super) const ACTIVATION_IMMUNITY_LEVEL: u8 = 2;
const EVALUATION_INTERVAL_TICKS: u64 = 16;
const VISUAL_INFLUENCE_CHUNK_RADIUS: i32 = 1;
const GROUP_LINK_CHUNK_RADIUS: i32 = VISUAL_INFLUENCE_CHUNK_RADIUS * 2;

#[derive(Clone)]
struct OptionalSpikeGroup {
    marker_chunks: Vec<IVec2>,
    tile_indices: Vec<usize>,
    prepared: adaptation::PreparedAdaptation,
}

#[derive(Resource)]
pub(crate) struct OptionalSpikeRuntime {
    groups: Vec<OptionalSpikeGroup>,
    last_evaluated_tick: Option<u64>,
}


pub(super) struct OptionalSpikeBakePlan {
    pub chunk_groups: HashMap<IVec2, usize>,
    runtime: OptionalSpikeRuntime,
}

impl OptionalSpikeBakePlan {
    pub fn into_parts(self) -> (HashMap<IVec2, usize>, OptionalSpikeRuntime) {
        (self.chunk_groups, self.runtime)
    }
}

pub(super) fn activated_compiled_level(compiled: &CompiledLevel) -> CompiledLevel {
    let mut active = compiled.clone();
    let len = (active.width * active.height) as usize;
    if active.optional_spike_tiles.len() != len {
        active.optional_spike_tiles.resize(len, false);
    }
    if active.terrain.len() != len || active.spike_tiles.len() != len {
        return active;
    }

    for index in 0..len {
        if !active.optional_spike_tiles[index] {
            continue;
        }
        active.terrain[index] = false;
        active.spike_tiles[index] = true;
    }
    active
}

fn optional_marker_chunks(compiled: &CompiledLevel) -> Vec<IVec2> {
    let mut chunks = HashSet::new();
    for (index, &optional) in compiled.optional_spike_tiles.iter().enumerate() {
        if !optional {
            continue;
        }
        chunks.insert(tile_index_chunk(compiled.width, index));
    }
    let mut chunks = chunks.into_iter().collect::<Vec<_>>();
    chunks.sort_unstable_by_key(|chunk| (chunk.y, chunk.x));
    chunks
}

fn tile_index_chunk(width: u32, index: usize) -> IVec2 {
    let tile_x = index as u32 % width;
    let tile_y = index as u32 / width;
    let chunk_size = mapgen::CHUNK_IMAGE_SIZE;
    IVec2::new((tile_x / chunk_size) as i32, (tile_y / chunk_size) as i32)
}

fn group_marker_chunks(marker_chunks: &[IVec2]) -> Vec<Vec<IVec2>> {
    let marker_set = marker_chunks.iter().copied().collect::<HashSet<_>>();
    let mut visited = HashSet::new();
    let mut groups = Vec::new();

    for &start in marker_chunks {
        if !visited.insert(start) {
            continue;
        }
        let mut queue = VecDeque::from([start]);
        let mut group = Vec::new();
        while let Some(chunk) = queue.pop_front() {
            group.push(chunk);
            enqueue_linked_marker_chunks(
                chunk,
                &marker_set,
                &mut visited,
                &mut queue,
            );
        }
        group.sort_unstable_by_key(|chunk| (chunk.y, chunk.x));
        groups.push(group);
    }
    groups
}

fn enqueue_linked_marker_chunks(
    chunk: IVec2,
    marker_chunks: &HashSet<IVec2>,
    visited: &mut HashSet<IVec2>,
    queue: &mut VecDeque<IVec2>,
) {
    for &candidate in marker_chunks {
        if visited.contains(&candidate)
            || chunk_distance(chunk, candidate) > GROUP_LINK_CHUNK_RADIUS
        {
            continue;
        }
        visited.insert(candidate);
        queue.push_back(candidate);
    }
}

fn assign_optional_tiles_to_groups(
    compiled: &CompiledLevel,
    grouped_chunks: &[Vec<IVec2>],
    groups: &mut [OptionalSpikeGroup],
) {
    let chunk_groups = grouped_chunks
        .iter()
        .enumerate()
        .flat_map(|(group_index, chunks)| {
            chunks.iter().map(move |&chunk| (chunk, group_index))
        })
        .collect::<HashMap<_, _>>();

    for (index, &optional) in compiled.optional_spike_tiles.iter().enumerate() {
        if !optional {
            continue;
        }
        let chunk = tile_index_chunk(compiled.width, index);
        let Some(&group_index) = chunk_groups.get(&chunk) else {
            continue;
        };
        groups[group_index].tile_indices.push(index);
    }
}

fn assign_visual_chunks(
    changed_chunks: &HashSet<IVec2>,
    grouped_chunks: &[Vec<IVec2>],
) -> HashMap<IVec2, usize> {
    let owners: HashMap<_, _> = grouped_chunks.iter().enumerate()
        .flat_map(|(group, chunks)| chunks.iter().map(move |&chunk| (chunk, group))).collect();
    changed_chunks.iter().filter_map(|&chunk| visual_chunk_owner(chunk, &owners).map(|group| (chunk, group))).collect()
}

fn visual_chunk_owner(chunk: IVec2, owners: &HashMap<IVec2, usize>) -> Option<usize> {
    (0..9).filter_map(|index| {
        let delta = IVec2::new(index % 3 - 1, index / 3 - 1);
        owners.get(&(chunk + delta)).map(|&group| (delta.abs().max_element(), group))
    }).min().map(|(_, group)| group)
}

fn chunk_distance(a: IVec2, b: IVec2) -> i32 {
    (a - b).abs().max_element()
}

fn player_chunk(compiled: &CompiledLevel, position: Vec2) -> IVec2 {
    let tile_size = compiled.tile_size().max(f32::EPSILON);
    let tile_x = (position.x / tile_size).floor() as i32;
    let tile_y =
        (compiled.height as f32 - 1.0 - (position.y / tile_size).floor()) as i32;
    let bounded_x = tile_x.clamp(0, compiled.width.saturating_sub(1) as i32);
    let bounded_y = tile_y.clamp(0, compiled.height.saturating_sub(1) as i32);
    let chunk_size = mapgen::CHUNK_IMAGE_SIZE as i32;
    IVec2::new(
        bounded_x.div_euclid(chunk_size),
        bounded_y.div_euclid(chunk_size),
    )
}

fn group_is_outside_player_range(group: &OptionalSpikeGroup, player_chunk: IVec2) -> bool {
    group.marker_chunks.iter().all(|&chunk| {
        chunk_distance(chunk, player_chunk) > PLAYER_ACTIVE_CHUNK_RADIUS
    })
}

#[expect(
    clippy::too_many_arguments,
    reason = "Activation coordinates progression, player range, terrain state, render variants, and spike hazards."
)]
pub(super) fn activate_optional_spikes(
    mut commands: Commands,
    mut compiled: Option<ResMut<CompiledLevel>>,
    mut runtime: Option<ResMut<OptionalSpikeRuntime>>,
    mut collision: ResMut<TileCollisionWorld>,
    ground_ready: Res<pipeline::GenerationBuild>,
    kill_count: Res<KillCount>,
    game_time: Res<GameTime>,
    player_query: Query<&Transform, (With<Player>, Without<Dead>)>,
    mut visual_query: Query<&mut MeshMaterial2d<ColorMaterial>>,
) {
    if !ground_ready.ground_ready()
        || PlanetImmunityLevel::from_kills(**kill_count).get()
            < ACTIVATION_IMMUNITY_LEVEL
    {
        return;
    }
    let (Some(compiled), Some(runtime)) =
        (compiled.as_mut(), runtime.as_mut())
    else {
        return;
    };
    let tick = game_time.tick();
    if runtime.last_evaluated_tick == Some(tick)
        || !tick.is_multiple_of(EVALUATION_INTERVAL_TICKS)
    {
        return;
    }
    runtime.last_evaluated_tick = Some(tick);
    let Ok(player_transform) = player_query.single() else {
        return;
    };
    let player_chunk = player_chunk(compiled, player_transform.translation.xy());
    let activating = runtime
        .groups
        .iter()
        .enumerate()
        .filter(|(_, group)| {
            !group.prepared.is_active() && group_is_outside_player_range(group, player_chunk)
        })
        .map(|(index, _)| index)
        .collect::<HashSet<_>>();
    if activating.is_empty() {
        return;
    }

    // This system runs before collision synchronization and simulation. Commands insert
    // prepared hazard memberships at that same ordered boundary.
    let compiled = compiled.bypass_change_detection();
    let mut activated_tiles = 0;
    for &group_index in &activating {
        activated_tiles += runtime.groups[group_index].prepared.activate(
            &mut commands, compiled, &mut collision, &mut visual_query);
    }
    info!(
        "levelgen: activated {activated_tiles} optional spike tiles across {} groups",
        activating.len()
    );
}


pub(super) fn prepare_local_variants(compiled: &CompiledLevel) -> Option<OptionalSpikeBakePlan> {
    let marker_chunks = optional_marker_chunks(compiled);
    if marker_chunks.is_empty() { return None; }
    let grouped = group_marker_chunks(&marker_chunks);
    let mut groups: Vec<_> = grouped.iter().map(|chunks| OptionalSpikeGroup {
        marker_chunks: chunks.clone(), tile_indices: Vec::new(), prepared: adaptation::PreparedAdaptation::default(),
    }).collect();
    assign_optional_tiles_to_groups(compiled, &grouped, &mut groups);
    let mut influenced = HashSet::new();
    let cols = compiled.width.div_ceil(mapgen::CHUNK_IMAGE_SIZE) as i32;
    let rows = compiled.height.div_ceil(mapgen::CHUNK_IMAGE_SIZE) as i32;
    for chunk in marker_chunks {
        add_influenced_chunks(&mut influenced, chunk, cols, rows);
    }
    info!("levelgen: preparing optional spikes: groups={}, tiles={}, affected_chunks={}",
        groups.len(), groups.iter().map(|group| group.tile_indices.len()).sum::<usize>(), influenced.len());
    Some(OptionalSpikeBakePlan { chunk_groups: assign_visual_chunks(&influenced, &grouped),
        runtime: OptionalSpikeRuntime { groups, last_evaluated_tick: None } })
}

fn add_influenced_chunks(chunks: &mut HashSet<IVec2>, center: IVec2, cols: i32, rows: i32) {
    for index in 0..9 {
        let chunk = center + IVec2::new(index % 3 - 1, index / 3 - 1);
        if chunk.x >= 0 && chunk.y >= 0 && chunk.x < cols && chunk.y < rows { chunks.insert(chunk); }
    }
}

impl OptionalSpikeRuntime {
    pub(super) fn prepare(&mut self, commands: &mut Commands, parent: Entity, level: &CompiledLevel,
        visuals: Vec<(usize, Entity, Handle<ColorMaterial>)>,
    ) {
        let tile_size = level.tile_size();
        for (id, group) in self.groups.iter_mut().enumerate() {
            group.prepared.id = id as u64;
            group.prepared.affected_chunks = affected_chunks(&group.marker_chunks, level);
            group.prepared.states[1].tiles = group.tile_indices.iter().map(|&index| adaptation::TileDelta {
                index, terrain: false, hazard: true,
            }).collect();
            group.prepared.states[1].entities = prepare_hazards(commands, parent, level, tile_size, &group.tile_indices);
        }
        for (group, entity, material) in visuals {
            self.groups[group].prepared.states[1].visuals.push((entity, material));
        }
    }
}

fn affected_chunks(markers: &[IVec2], level: &CompiledLevel) -> Vec<IVec2> {
    let mut affected = HashSet::new();
    for &chunk in markers {
        add_influenced_chunks(&mut affected, chunk, level.width.div_ceil(mapgen::CHUNK_IMAGE_SIZE) as i32,
            level.height.div_ceil(mapgen::CHUNK_IMAGE_SIZE) as i32);
    }
    let mut chunks: Vec<_> = affected.into_iter().collect();
    chunks.sort_by_key(|chunk| (chunk.y, chunk.x));
    chunks
}

fn prepare_hazards(commands: &mut Commands, parent: Entity, level: &CompiledLevel, tile_size: f32, tiles: &[usize]) -> Vec<Entity> {
    tiles.iter().map(|&index| {
        let tile = spike_tile_coord(level, index);
        prepare_spike_hazard(commands, parent, spike_tile_world_center(level, tile_size, tile), tile_size * 0.3, tile, false)
    }).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn activated_level_replaces_optional_terrain_with_spikes() {
        let mut compiled = CompiledLevel {
            width: 2,
            height: 1,
            terrain: vec![true, true],
            spike_tiles: vec![false, false],
            optional_spike_tiles: vec![false, true],
            ..Default::default()
        };
        compiled.bounds = Rect::new(0.0, 0.0, 12.0, 6.0);

        let active = activated_compiled_level(&compiled);
        assert_eq!(active.terrain, vec![true, false]);
        assert_eq!(active.spike_tiles, vec![false, true]);
    }

    #[test]
    fn markers_across_chunk_boundaries_share_local_visual_state() {
        let width = mapgen::CHUNK_IMAGE_SIZE * 10;
        let mut compiled = CompiledLevel { width, height: 1,
            terrain: vec![true; width as usize], spike_tiles: vec![false; width as usize],
            optional_spike_tiles: vec![false; width as usize], ..default() };
        for index in [95, 96, 800] { compiled.optional_spike_tiles[index] = true; }
        let plan = prepare_local_variants(&compiled).unwrap();
        assert_eq!(plan.runtime.groups.len(), 2);
        assert_eq!(plan.chunk_groups[&IVec2::new(0, 0)], plan.chunk_groups[&IVec2::new(2, 0)]);
        assert_ne!(plan.chunk_groups[&IVec2::new(2, 0)], plan.chunk_groups[&IVec2::new(8, 0)]);
        assert!(!plan.chunk_groups.contains_key(&IVec2::new(5, 0)));
    }

    #[test]
    fn activation_range_uses_spawn_director_radius() {
        let group = OptionalSpikeGroup {
            marker_chunks: vec![IVec2::new(3, 0)],
            tile_indices: vec![],
            prepared: adaptation::PreparedAdaptation::default(),
        };
        assert!(!group_is_outside_player_range(&group, IVec2::new(1, 0)));
        assert!(group_is_outside_player_range(&group, IVec2::ZERO));
    }
}
