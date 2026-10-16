//! Shared hold/release lifetime for placement tools; each tool owns its placement rules.
use bevy::{ecs::{query::QueryData, system::SystemParam}, prelude::*};
use bevy::camera::visibility::RenderLayers;
use leafwing_input_manager::action_state::ActionState;
use theseeker_engine::{animation::SpriteAnimationBundle, physics::{Collider, TileCollisionWorld}};
use crate::{StateDespawnMarker, game::{gentstate::Facing, world_objects::Capsule}};
use super::super::{Player, PlayerAction, SkillInventory, MaterialInventory, CraftMaterial,
    sensors::GroundSensor, foreground::ActionTag, states::Elevator, passives::PassiveEvent,
    spawns::tether_node::{TetherNode, spawn_tether_node, tether_connector_in_range, TETHER_VISUAL_Z}};
use super::{portable_tent::{self, TentInteriorAssets, TentInteriorOccupant}, tether_node,
    registry::SLOT_INPUTS, types::SkillId};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum PlacementTool { Tent, Tether }

#[derive(Component)]
pub(crate) struct PlacementPreview { owner: Entity, tool: PlacementTool }

#[derive(Clone, Copy)]
pub(super) struct Placement { pub center: Vec2, pub valid: bool }

#[derive(QueryData)]
#[query_data(mutable)]
pub(crate) struct PlacementUser {
    entity: Entity,
    input: &'static ActionState<PlayerAction>,
    transform: &'static Transform,
    facing: &'static Facing,
    collider: Option<&'static Collider>,
    ground: Option<&'static GroundSensor>,
    skills: &'static SkillInventory,
    materials: &'static mut MaterialInventory,
    occupant: Option<&'static TentInteriorOccupant>,
    elevator: Has<ActionTag<Elevator>>,
    recovering: Has<crate::game::spikes::SpikeRespawnPending>,
}

#[derive(SystemParam)]
pub(crate) struct PreviewWorld<'w, 's> {
    commands: Commands<'w, 's>,
    previews: Query<'w, 's, (Entity, &'static PlacementPreview), Without<Player>>,
    visuals: Query<'w, 's, (&'static mut Sprite, &'static mut Transform, &'static mut RenderLayers), (With<PlacementPreview>, Without<Player>)>,
    tiles: Option<Res<'w, TileCollisionWorld>>,
    interiors: Option<Res<'w, TentInteriorAssets>>,
    tethers: Query<'w, 's, (&'static Transform, &'static TetherNode), (Without<Player>, Without<PlacementPreview>)>,
    capsules: Query<'w, 's, &'static Transform, (With<Capsule>, Without<Player>, Without<PlacementPreview>)>,
    connection: ResMut<'w, tether_node::TetherConnectionStatus>,
    events: MessageWriter<'w, PassiveEvent>,
}

pub(crate) fn update_placement_previews(
    mut players: Query<PlacementUser, (With<Player>, Without<PlacementPreview>)>,
    mut world: PreviewWorld,
) {
    world.connection.available = false;
    let Ok(mut player) = players.single_mut() else { return; };
    // Matches the tool order in the gameplay schedule.
    for tool in [PlacementTool::Tent, PlacementTool::Tether] {
        update_tool(tool, &mut player, &mut world);
    }
}

fn update_tool(tool: PlacementTool, player: &mut PlacementUserItem, world: &mut PreviewWorld) {
    if tool == PlacementTool::Tether && (player.collider.is_none() || player.ground.is_none()) {
        return;
    }
    let mut existing = world.previews.iter().filter(|(_, preview)| preview.owner == player.entity && preview.tool == tool);
    let active = existing.next().map(|(entity, _)| entity);
    for (duplicate, _) in existing {
        world.commands.entity(duplicate).despawn();
    }
    let slot = SLOT_INPUTS.iter().find(|slot| player.skills.get_skill_at_slot(slot.index) == Some(tool.skill()));
    let eligible = !player.recovering && !(tool == PlacementTool::Tent && player.elevator)
        && player.materials.backpack_count(tool.material()) > 0;
    let Some(slot) = slot.filter(|_| eligible) else {
        despawn_preview(&mut world.commands, active);
        return;
    };
    let placement = placement(tool, player, world);
    if player.input.just_released(&slot.action) {
        despawn_preview(&mut world.commands, active);
        commit_placement(tool, placement, player, world);
        return;
    }
    if !player.input.pressed(&slot.action) {
        despawn_preview(&mut world.commands, active);
        return;
    }
    sync_preview(tool, placement, active, player, world);
}

fn placement(tool: PlacementTool, player: &PlacementUserItem, world: &mut PreviewWorld) -> Placement {
    match tool {
        PlacementTool::Tent => portable_tent::preview_placement(player.transform, player.facing,
            player.occupant, world.tiles.as_deref(), world.interiors.as_deref()),
        PlacementTool::Tether => {
            let placement = tether_node::compute_tether_placement(player.transform, player.facing,
                player.collider.expect("tether player has collider"), player.ground.expect("tether player has sensor"), world.tiles.as_deref());
            world.connection.available = tether_connector_in_range(placement.center,
                world.tethers.iter().filter(|(_, tether)| tether.is_network_active())
                    .map(|(transform, _)| transform.translation.truncate())
                    .chain(world.capsules.iter().map(|transform| transform.translation.truncate())));
            placement
        }
    }
}

fn commit_placement(tool: PlacementTool, placement: Placement, player: &mut PlacementUserItem, world: &mut PreviewWorld) {
    if !placement.valid || !player.materials.remove_from_backpack(tool.material(), 1) {
        return;
    }
    match tool {
        PlacementTool::Tent => portable_tent::spawn_portable_tent(&mut world.commands, placement.center),
        PlacementTool::Tether => { spawn_tether_node(&mut world.commands, placement.center); }
    }
    world.events.write(PassiveEvent::SkillUsed { owner: player.entity, skill: tool.skill() });
}

fn sync_preview(tool: PlacementTool, placement: Placement, active: Option<Entity>, player: &PlacementUserItem, world: &mut PreviewWorld) {
    let color = if placement.valid { Color::srgba(0.6, 1.0, 0.6, 0.55) } else { Color::srgba(1.0, 0.45, 0.45, 0.55) };
    let position = placement.center.extend(tool.z());
    let layers = tool.layers(player.occupant.is_some());
    if let Some(entity) = active {
        let Ok((mut sprite, mut transform, mut render_layers)) = world.visuals.get_mut(entity) else { return; };
        sprite.color = color;
        transform.translation = position;
        *render_layers = layers;
        return;
    }
    world.commands.spawn((
        PlacementPreview { owner: player.entity, tool },
        SpriteAnimationBundle::new_play_key(tool.animation()),
        Sprite { color, texture_atlas: Some(TextureAtlas::default()), ..default() },
        Transform::from_translation(position), Visibility::Visible, InheritedVisibility::VISIBLE,
        layers, StateDespawnMarker, Name::new(tool.name()),
    ));
}

fn despawn_preview(commands: &mut Commands, entity: Option<Entity>) {
    if let Some(entity) = entity { commands.entity(entity).despawn(); }
}

pub(crate) fn cleanup_orphaned_previews(
    mut commands: Commands,
    previews: Query<(Entity, &PlacementPreview)>,
    players: Query<(), With<Player>>,
) {
    for (entity, preview) in &previews {
        if players.get(preview.owner).is_err() { commands.entity(entity).despawn(); }
    }
}

impl PlacementTool {
    fn skill(self) -> SkillId { match self { Self::Tent => SkillId::PortableTent, Self::Tether => SkillId::TetherNode } }
    fn material(self) -> CraftMaterial { match self { Self::Tent => CraftMaterial::Tent, Self::Tether => CraftMaterial::Tether } }
    fn z(self) -> f32 { match self { Self::Tent => portable_tent::TENT_Z, Self::Tether => TETHER_VISUAL_Z } }
    fn animation(self) -> &'static str { match self { Self::Tent => "anim.objects.TentPreview", Self::Tether => "anim.objects.TetherPreview" } }
    fn name(self) -> &'static str { match self { Self::Tent => "PortableTentPreview", Self::Tether => "TetherNodePreview" } }
    fn layers(self, inside: bool) -> RenderLayers {
        match self {
            Self::Tent if inside => RenderLayers::layer(2),
            Self::Tent => RenderLayers::layer(0),
            Self::Tether => crate::graphics::pixel_perfect::pixel_perfect_layers(),
        }
    }
}

#[cfg(test)]
#[path = "placement_preview_tests.rs"]
mod tests;
