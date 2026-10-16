use theseeker_engine::effects::scale_recovery::ScaleRecovery;
use crate::game::player::foreground::ActionMut;
use crate::game::combat::effects::DamageFlash;
use crate::game::enemy::{
    corpse_loot_distance_squared, CorpseLoot, Decay, OpiTombstone,
};
use crate::game::player::spawns::item_drops::{
    spawn_material_drop_burst, DropAssetHandles, DropLaunchMode,
};
use crate::game::player::spawns::tether_node::TetherNode;
use crate::game::player::states::{transition_action, Gathering};
use crate::game::player::{
    CraftMaterial, Grounded, Player, PlayerAction, PlayerStateSet,
};
use crate::prelude::*;
use leafwing_input_manager::action_state::ActionState;
use std::cmp::Ordering;
use theseeker_engine::physics::LinearVelocity;
use theseeker_engine::time::GameTickUpdate;

pub const HARVEST_HITS_REQUIRED: u32 = 3;
const HARVEST_RANGE: f32 = 22.0;
const HARVEST_DROP_PICKUP_DELAY_TICKS: u32 = 48;
const HARVEST_DROP_SPAWN_Y_OFFSET: f32 = 10.0;
const HARVEST_STRETCH_TICKS: u32 = 8;
const HARVEST_MAX_SQUEEZE_X: f32 = 0.7;
const HARVEST_MAX_STRETCH_Y: f32 = 1.3;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct HarvestPlantDefinition {
    pub material: CraftMaterial,
    pub drop_count: u32,
}

#[derive(Component, Debug, Clone, Copy)]
pub struct HarvestableResource {
    pub material: CraftMaterial,
    pub drop_count: u32,
    pub hits_remaining: u32,
}

#[derive(Component, Debug, Clone, Copy)]
pub struct HarvestablePlant;

#[derive(Component, Debug, Clone, Copy)]
pub struct HarvestableOre;

#[derive(Bundle)]
pub(crate) struct HarvestablePlantBundle {
    plant: HarvestablePlant,
    harvestable: HarvestableResource,
}

#[derive(Bundle)]
pub(crate) struct HarvestableOreBundle {
    ore: HarvestableOre,
    harvestable: HarvestableResource,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct HarvestOreDefinition {
    pub category: u8,
    pub variant: u32,
    pub material: CraftMaterial,
    pub drop_count: u32,
}

#[derive(Component, Debug, Clone, Copy)]
struct HarvestStretch(ScaleRecovery);

impl HarvestableResource {
    fn new(material: CraftMaterial, drop_count: u32) -> Self {
        Self {
            material,
            drop_count,
            hits_remaining: HARVEST_HITS_REQUIRED,
        }
    }
}

impl HarvestablePlant {
    pub(crate) fn from_definition(
        definition: HarvestPlantDefinition,
    ) -> HarvestablePlantBundle {
        HarvestablePlantBundle {
            plant: HarvestablePlant,
            harvestable: HarvestableResource::new(
                definition.material,
                definition.drop_count,
            ),
        }
    }
}

impl HarvestableOre {
    pub(crate) fn from_definition(
        definition: HarvestOreDefinition,
    ) -> HarvestableOreBundle {
        HarvestableOreBundle {
            ore: HarvestableOre,
            harvestable: HarvestableResource::new(
                definition.material,
                definition.drop_count,
            ),
        }
    }
}

pub struct HarvestableMaterialsPlugin;

impl Plugin for HarvestableMaterialsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            GameTickUpdate,
            animate_harvest_stretch
                .run_if(in_state(AppState::InGame)),
        );
        app.add_systems(
            GameTickUpdate,
            harvest_nearby_resources
                .after(PlayerStateSet::Behavior)
                .run_if(in_state(AppState::InGame)),
        );
    }
}

fn parse_count_suffix(name: &str, needle: &str) -> Option<u32> {
    let (_, tail) = name.split_once(needle)?;
    let digits: String = tail
        .chars()
        .take_while(char::is_ascii_digit)
        .collect();
    if digits.is_empty() {
        None
    } else {
        digits.parse::<u32>().ok()
    }
}

pub(crate) fn parse_harvest_drop_from_sprite_name(
    sprite_name: &str,
) -> Option<HarvestPlantDefinition> {
    let name = sprite_name.to_ascii_lowercase();

    if let Some(count) = parse_count_suffix(&name, "redberry") {
        return Some(HarvestPlantDefinition {
            material: CraftMaterial::RedBerry,
            drop_count: count.max(1),
        });
    }
    if let Some(count) = parse_count_suffix(&name, "blueberry") {
        return Some(HarvestPlantDefinition {
            material: CraftMaterial::BlueBerry,
            drop_count: count.max(1),
        });
    }
    if let Some(count) = parse_count_suffix(&name, "mossberry") {
        return Some(HarvestPlantDefinition {
            material: CraftMaterial::MossBerry,
            drop_count: count.max(1),
        });
    }

    None
}

pub(crate) fn parse_harvest_ore_from_sprite_name(
    sprite_name: &str,
) -> Option<HarvestOreDefinition> {
    let name = sprite_name.to_ascii_lowercase();
    let tail = name.strip_prefix("ore")?;
    let category = tail.chars().next()?.to_digit(10)? as u8;
    let material = match category {
        1 => CraftMaterial::Ore1,
        2 => CraftMaterial::Ore2,
        3 => CraftMaterial::Ore3,
        _ => return None,
    };
    let tail = &tail[1..];
    let (variant_part, count_part) = tail.split_once('c')?;

    let variant_digits: String = variant_part
        .chars()
        .take_while(char::is_ascii_digit)
        .collect();
    let variant = variant_digits.parse::<u32>().ok()?;

    let count_digits: String = count_part
        .chars()
        .take_while(char::is_ascii_digit)
        .collect();
    let drop_count = count_digits.parse::<u32>().ok()?.max(1);

    Some(HarvestOreDefinition {
        category,
        variant,
        material,
        drop_count,
    })
}

fn apply_harvest_stretch(
    entity: Entity, transform: &mut Transform, existing: Option<&HarvestStretch>, commands: &mut Commands,
) {
    let recovery = ScaleRecovery::start(transform, existing.map(|stretch| stretch.0),
        Vec2::new(HARVEST_MAX_SQUEEZE_X, HARVEST_MAX_STRETCH_Y));
    commands.entity(entity).insert(HarvestStretch(recovery));
}

fn animate_harvest_stretch(
    mut commands: Commands, mut query: Query<(Entity, &mut Transform, &mut HarvestStretch)>,
) {
    for (entity, mut transform, mut stretch) in &mut query {
        if stretch.0.tick(&mut transform, HARVEST_STRETCH_TICKS,
            Vec2::new(HARVEST_MAX_SQUEEZE_X, HARVEST_MAX_STRETCH_Y)) {
            commands.entity(entity).remove::<HarvestStretch>();
        }
    }
}

fn compare_harvest_candidates(
    a_distance: f32,
    a_is_tether: bool,
    b_distance: f32,
    b_is_tether: bool,
) -> Ordering {
    a_is_tether.cmp(&b_is_tether).then_with(|| {
        a_distance
            .partial_cmp(&b_distance)
            .unwrap_or(Ordering::Equal)
    })
}

fn harvest_nearby_resources(
    mut commands: Commands,
    mut player_q: Query<
        (
            Entity,
            &ActionState<PlayerAction>,
            &GlobalTransform,
            Has<Grounded>,
            Option<ActionMut<Gathering>>,
            &mut LinearVelocity,
        ),
        With<Player>,
    >,
    harvest_candidates: Query<
        (Entity, &GlobalTransform, Has<TetherNode>),
        With<HarvestableResource>,
    >,
    corpse_candidates: Query<
        &CorpseLoot,
        Or<(With<Decay>, With<OpiTombstone>)>,
    >,
    visual_positions: Query<&GlobalTransform>,
    mut harvest_q: Query<
        (
            &GlobalTransform,
            &mut Transform,
            &mut HarvestableResource,
            Option<&HarvestStretch>,
        ),
    >,
    drop_assets: Option<Res<DropAssetHandles>>,
) {
    let Ok((
        player_entity,
        actions,
        player_transform,
        is_grounded,
        maybe_gathering,
        mut velocity,
    )) = player_q.single_mut()
    else {
        return;
    };
    if !actions.just_pressed(&PlayerAction::Interact) {
        return;
    }
    if !is_grounded {
        return;
    }

    let player_pos = player_transform.translation().truncate();
    let Some((target_entity, target_is_tether)) = harvest_candidates
        .iter()
        .filter_map(|(entity, transform, is_tether)| {
            let dist_sq = transform
                .translation()
                .truncate()
                .distance_squared(player_pos);
            if dist_sq <= HARVEST_RANGE * HARVEST_RANGE {
                Some((entity, dist_sq, is_tether))
            } else {
                None
            }
        })
        .min_by(|(_, a_distance, a_is_tether), (_, b_distance, b_is_tether)| {
            compare_harvest_candidates(
                *a_distance,
                *a_is_tether,
                *b_distance,
                *b_is_tether,
            )
        })
        .map(|(entity, _, is_tether)| (entity, is_tether))
    else {
        return;
    };

    if target_is_tether
        && corpse_candidates.iter().any(|corpse_loot| {
            corpse_loot_distance_squared(
                corpse_loot,
                player_pos,
                &visual_positions,
            )
            .is_some()
        })
    {
        return;
    }

    let Ok((transform, mut local_transform, mut harvestable, stretch)) =
        harvest_q.get_mut(target_entity)
    else {
        return;
    };

    commands.entity(target_entity).insert(DamageFlash {
        current_ticks: 0,
        max_ticks: 8,
    });
    apply_harvest_stretch(
        target_entity,
        &mut local_transform,
        stretch,
        &mut commands,
    );

    velocity.0 = Vec2::ZERO;
    if let Some(mut gathering) = maybe_gathering {
        gathering.tick = 0;
    } else {
        transition_action(&mut commands, player_entity, Gathering::default());
    }

    harvestable.hits_remaining =
        harvestable.hits_remaining.saturating_sub(1);
    if harvestable.hits_remaining > 0 {
        return;
    }

    if let Some(drop_assets) = drop_assets.as_deref() {
        spawn_material_drop_burst(
            &mut commands,
            drop_assets,
            harvestable.material,
            harvestable.drop_count,
            transform.translation().truncate()
                + Vec2::Y * HARVEST_DROP_SPAWN_Y_OFFSET,
            DropLaunchMode::Random,
            HARVEST_DROP_PICKUP_DELAY_TICKS,
        );
    }

    commands.entity(target_entity).despawn();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collectible_resource_outranks_a_closer_tether() {
        assert_eq!(
            compare_harvest_candidates(20.0, false, 1.0, true),
            Ordering::Less,
        );
    }

    #[test]
    fn same_priority_resources_prefer_nearest() {
        assert_eq!(
            compare_harvest_candidates(4.0, false, 9.0, false),
            Ordering::Less,
        );
    }
}
