//! Authored spawn markers and deterministic NPC selection.
use super::*;
use crate::game::npc::NpcKind;

pub(super) const VAGRANT_YAK_PAIR_HORIZONTAL_OFFSET: f32 = 24.0;
pub(super) const VAGRANT_PAIR_VERTICAL_OFFSET: f32 = 3.0;
pub(super) const YAK_PAIR_VERTICAL_OFFSET: f32 = 10.0;

pub(super) fn select_random_spawn_in_unreserved_chunk<'a>(
    spawns: &'a [SpawnSpec],
    kind: SpawnKind,
    reserved_chunks: &HashSet<IVec2>,
    rng: &mut rand::rngs::StdRng,
) -> Option<&'a SpawnSpec> {
    let candidates: Vec<&SpawnSpec> = spawns
        .iter()
        .filter(|spawn| {
            spawn.kind == kind && !reserved_chunks.contains(&spawn.chunk)
        })
        .collect();
    if candidates.is_empty() {
        None
    } else {
        let spawn_idx = rng.random_range(0..candidates.len());
        Some(candidates[spawn_idx])
    }
}

#[expect(clippy::too_many_arguments, reason = "Bevy system dependencies are expressed as explicit system parameters.")]
#[expect(
    clippy::too_many_lines,
    reason = "This Bevy spawn system coordinates the compiled level root, terrain, backgrounds, objects, and readiness flags in one flow."
)]
pub(super) fn spawn_compiled_level(
    mut commands: Commands,
    request: Option<Res<pipeline::GenerationRequest>>,
    compiled: Option<Res<CompiledLevel>>,
    root: Option<ResMut<LevelRootEntity>>,
    q_player: Query<
        Entity,
        Or<(
            With<crate::game::player::Player>,
            With<crate::game::player::PlayerBlueprint>,
        )>,
    >,
    npcs: Query<&NpcKind>,
    q_yak_spawns: Query<Entity, With<crate::game::yak::YakBlueprint>>,
    mut world_object_spawns: ParamSet<(
        Query<
            Entity,
            Or<(
                With<crate::game::world_objects::CapsuleBlueprint>,
                With<crate::game::world_objects::Capsule>,
            )>,
        >,
    )>,
    game_over: Option<Res<GameOver>>,
) {
    if game_over.is_some() {
        return;
    }
    let Some(compiled) = compiled else { return; };
    // Everything the level spawns hangs from one root entity.
    let root_e = if let Some(ref r) = root {
        r.0
    } else {
        let e = commands
            .spawn((
                Name::new("PngLevelRoot"),
                Transform::from_xyz(0.0, 0.0, 0.0),
                GlobalTransform::default(),
                // Ensure hierarchy has full visibility stack to avoid B0004 warnings
                Visibility::Visible,
                InheritedVisibility::VISIBLE,
                ViewVisibility::default(),
                StateDespawnMarker,
            ))
            .id();
        commands.insert_resource(LevelRootEntity(e));
        e
    };

    // Spawn player if not present
    if q_player.is_empty() {
        if let Some(spawn) =
            compiled.spawns.iter().find(|s| s.kind == SpawnKind::Player)
        {
            let mut t = Transform::from_translation(spawn.position.extend(0.0));
            // Slight z separation for determinism similar to LDtk layers
            t.translation.z = 10.0 * 0.000_001;
            let e = commands
                .spawn((
                    crate::game::player::PlayerBlueprintBundle::default(),
                    t,
                    StateDespawnMarker,
                    bevy::ecs::hierarchy::ChildOf(root_e),
                ))
                .id();
            commands.insert_resource(crate::camera::SnapCameraNext);
            debug!(
                "Spawned PlayerBlueprint at {:?} (entity {:?})",
                spawn.position, e
            );
        }
    }

    let mut reserved_npc_chunks = HashSet::new();
    let mut npc_rng = <rand::rngs::StdRng as rand::SeedableRng>::seed_from_u64(request.as_ref().map_or(0, |r| r.seed) ^ 0x4e50_4300);

    if !npcs.iter().any(|kind| *kind == NpcKind::Vagrant) && q_yak_spawns.is_empty() {
        if let Some(spawn) = select_random_spawn_in_unreserved_chunk(
            &compiled.spawns,
            SpawnKind::VagrantYakPair,
            &reserved_npc_chunks,
            &mut npc_rng,
        ) {
            reserved_npc_chunks.insert(spawn.chunk);
            let vagrant_position = spawn.position
                + Vec2::new(
                    -VAGRANT_YAK_PAIR_HORIZONTAL_OFFSET,
                    VAGRANT_PAIR_VERTICAL_OFFSET,
                );
            let yak_position = spawn.position
                + Vec2::new(
                    VAGRANT_YAK_PAIR_HORIZONTAL_OFFSET,
                    YAK_PAIR_VERTICAL_OFFSET,
                );

            let mut vagrant_transform =
                Transform::from_translation(vagrant_position.extend(0.0));
            vagrant_transform.translation.z = 10.0 * 0.000_001;
            let vagrant_entity = commands
                .spawn((
                    NpcKind::Vagrant,
                    vagrant_transform,
                    StateDespawnMarker,
                    bevy::ecs::hierarchy::ChildOf(root_e),
                ))
                .id();

            let mut yak_transform =
                Transform::from_translation(yak_position.extend(0.0));
            yak_transform.translation.z = 10.0 * 0.000_001;
            let yak_entity = commands
                .spawn((
                    crate::game::yak::YakBlueprintBundle::default(),
                    yak_transform,
                    StateDespawnMarker,
                    bevy::ecs::hierarchy::ChildOf(root_e),
                ))
                .id();

            debug!(
                "Spawned Vagrant/Yak pair from marker {:?} in chunk {:?} as {:?} / {:?} (entities {:?} / {:?})",
                spawn.position,
                spawn.chunk,
                vagrant_position,
                yak_position,
                vagrant_entity,
                yak_entity
            );
        }
    } else if npcs.iter().any(|kind| *kind == NpcKind::Vagrant) == q_yak_spawns.is_empty() {
        warn!(
            "levelgen: vagrant/yak pair is only partially present; skipping pair spawn"
        );
    }

    if !npcs.iter().any(|kind| *kind == NpcKind::Collector) {
        if let Some(spawn) = select_random_spawn_in_unreserved_chunk(
            &compiled.spawns,
            SpawnKind::Collector,
            &reserved_npc_chunks,
            &mut npc_rng,
        ) {
            reserved_npc_chunks.insert(spawn.chunk);

            let mut collector_transform =
                Transform::from_translation(spawn.position.extend(0.0));
            collector_transform.translation.z = 10.0 * 0.000_001;
            let collector_entity = commands
                .spawn((
                    NpcKind::Collector,
                    collector_transform,
                    StateDespawnMarker,
                    bevy::ecs::hierarchy::ChildOf(root_e),
                ))
                .id();

            debug!(
                "Spawned Collector from marker {:?} in chunk {:?} (entity {:?})",
                spawn.position,
                spawn.chunk,
                collector_entity,
            );
        }
    }

    if !npcs.iter().any(|kind| *kind == NpcKind::Monk) {
        if let Some(spawn) = select_random_spawn_in_unreserved_chunk(
            &compiled.spawns,
            SpawnKind::Monk,
            &reserved_npc_chunks,
            &mut npc_rng,
        ) {
            reserved_npc_chunks.insert(spawn.chunk);

            let mut monk_transform =
                Transform::from_translation(spawn.position.extend(0.0));
            monk_transform.translation.z = 10.0 * 0.000_001;
            let monk_entity = commands
                .spawn((
                    NpcKind::Monk,
                    monk_transform,
                    StateDespawnMarker,
                    bevy::ecs::hierarchy::ChildOf(root_e),
                ))
                .id();

            debug!(
                "Spawned Monk from marker {:?} in chunk {:?} (entity {:?})",
                spawn.position,
                spawn.chunk,
                monk_entity,
            );
        }
    }

    if !npcs.iter().any(|kind| *kind == NpcKind::Exiled) {
        if let Some(spawn) = select_random_spawn_in_unreserved_chunk(
            &compiled.spawns,
            SpawnKind::Exiled,
            &reserved_npc_chunks,
            &mut npc_rng,
        ) {
            reserved_npc_chunks.insert(spawn.chunk);

            let mut exiled_transform =
                Transform::from_translation(spawn.position.extend(0.0));
            exiled_transform.translation.z = 10.0 * 0.000_001;
            let exiled_entity = commands
                .spawn((
                    NpcKind::Exiled,
                    exiled_transform,
                    StateDespawnMarker,
                    bevy::ecs::hierarchy::ChildOf(root_e),
                ))
                .id();

            debug!(
                "Spawned Exiled from marker {:?} in chunk {:?} (entity {:?})",
                spawn.position,
                spawn.chunk,
                exiled_entity,
            );
        }
    }

    if world_object_spawns.p0().is_empty() {
        if let Some(spawn) =
            compiled.spawns.iter().find(|s| s.kind == SpawnKind::Capsule)
        {
            let mut t = Transform::from_translation(spawn.position.extend(0.0));
            t.translation.z = 10.0 * 0.000_001;
            let e = commands
                .spawn((
                    crate::game::world_objects::CapsuleBlueprintBundle::default(),
                    t,
                    StateDespawnMarker,
                    bevy::ecs::hierarchy::ChildOf(root_e),
                ))
                .id();
            debug!(
                "Spawned CapsuleBlueprint at {:?} in chunk {:?} (entity {:?})",
                spawn.position, spawn.chunk, e
            );
        }
    }
}
