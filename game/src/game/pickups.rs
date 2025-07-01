use bevy::{
    prelude::*, text::LineBreak, transform::TransformSystem, ui::UiSystem,
};
use rand::Rng;
use strum::IntoEnumIterator;
use theseeker_engine::time::GameTickUpdate;
use std::collections::HashMap;

use crate::{
    camera::MainCamera, prelude::StateDespawnMarker, ui::popup::PopupUi,
};

use super::{
    attack::KillCount,
    enemy::{dead, Enemy},
    gentstate::Dead,
    player::{Passive, Passives, Player},
};

pub const PICKUP_RANGE_SQUARED: f32 = 100.0;

pub struct PickupPlugin;
impl Plugin for PickupPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, load_pickup_assets).add_systems(
            GameTickUpdate,
            (
                spawn_pickups_on_death
                    .after(dead)
                    .run_if(resource_exists::<DropTracker>),
                display_passives_description
                    .after(UiSystem::Layout)
                    .before(TransformSystem::TransformPropagate)
                    .run_if(any_with_component::<PickupDrop>),
            ),
        );
    }
}

/// Marker component for the Pickup interaction hint
#[derive(Component)]
pub struct PickupHint;

#[derive(Component)]
pub struct PassiveDescriptionNode;

/// Intended to be used in the passive description UI node to reference the dropped passive.
#[derive(Component)]
pub struct PassiveEntity(Entity);

impl PassiveEntity {
    pub fn get(&self) -> Entity {
        self.0
    }
}

#[derive(Component)]
pub struct PickupDrop {
    pub p_type: PickupType,
}

impl PickupDrop {
    pub fn new(p_type: PickupType) -> Self {
        Self { p_type }
    }
}

#[derive(Resource)]
pub struct PickupAssetHandles {
    passive_map: HashMap<Passive, Handle<Image>>,
}

impl PickupAssetHandles {
    pub fn get_passive_handle(
        &self,
        passive: &Passive,
    ) -> Option<&Handle<Image>> {
        self.passive_map.get(passive)
    }
}

pub fn load_pickup_assets(assets: Res<AssetServer>, mut commands: Commands) {
    let passive_mappings: Vec<(Passive, &str)> = vec![
        (
            Passive::Bloodstone,
            "items/passives/Bloodstone.png",
        ),
        (
            Passive::FlamingHeart,
            "items/passives/FlamingHeart.png",
        ),
        (
            Passive::IceDagger,
            "items/passives/IceDagger.png",
        ),
        (
            Passive::GlowingShard,
            "items/passives/GlowingShard.png",
        ),
        (
            Passive::ObsidianNecklace,
            "items/passives/ObsidianNecklace.png",
        ),
        (
            Passive::HeavyBoots,
            "items/passives/HeavyBoots.png",
        ),
        (
            Passive::SerpentRing,
            "items/passives/SerpentRing.png",
        ),
        (
            Passive::FrenziedAttack,
            "items/passives/FrenziedAttack.png",
        ),
        (
            Passive::PackKiller,
            "items/passives/PackKiller.png",
        ),
        (
            Passive::DeadlyFeather,
            "items/passives/DeadlyFeather.png",
        ),
        (
            Passive::Sharpshooter,
            "items/passives/Sharpshooter.png",
        ),
        (
            Passive::ProtectiveSpirit,
            "items/passives/ProtectiveSpirit.png",
        ),
        (
            Passive::RabbitsFoot,
            "items/passives/RabbitsFoot.png",
        ),
        (
            Passive::CriticalRegeneration,
            "items/passives/CriticalRegeneration.png",
        ),
        (
            Passive::VitalityOverclock,
            "items/passives/VitalityOverclock.png",
        ),
    ];

    commands.insert_resource(PickupAssetHandles {
        passive_map: HashMap::from_iter(
            passive_mappings
                .iter()
                .map(|(x, y)| (x.clone(), assets.load(*y)))
                .collect::<Vec<_>>(),
        ),
    });
}

pub struct SpawnPickupCommand {
    pos: Vec3,
    p_type: PickupType,
}

impl Command for SpawnPickupCommand {
    fn apply(self, world: &mut World) {
        let pos = self.pos;
        let handles = world.get_resource::<PickupAssetHandles>().unwrap();

        match self.p_type.clone() {
            PickupType::PassiveDrop(passive) => {
                let texture_handle = handles.passive_map.get(&passive).unwrap();
                let transform = Transform::from_translation(Vec3::new(pos.x, pos.y, 0.0));

                let entity = world
                    .spawn((
                        Name::new("PickupDrop"),
                        PickupDrop::new(self.p_type),
                        Sprite {
                            image: texture_handle.clone(),
                            ..Default::default()
                        },
                        transform,
                        GlobalTransform::default(),
                        Visibility::Visible,
                        InheritedVisibility::VISIBLE,
                        ViewVisibility::default(),
                        StateDespawnMarker,
                    ))
                    .id();

                world.spawn((
                    Name::new("PassiveDescription"),
                    PassiveDescriptionNode,
                    PassiveEntity(entity),
                    Text::new(format!(
                        "{}\n{}",
                        passive.name(),
                        passive.description(),
                    )),
                    TextFont::from_font_size(24.0),
                    TextLayout::new(
                        JustifyText::Center,
                        LineBreak::WordBoundary,
                    ),
                    Node {
                        max_width: Val::Percent(33.0),
                        ..Default::default()
                    },
                    GlobalTransform::from_translation(Vec3::new(
                        pos.x, pos.y, 50.0,
                    )),
                    BackgroundColor::from(Color::BLACK.with_alpha(0.75)),
                    Visibility::Hidden,
                    StateDespawnMarker,
                ));
            },
        }
    }
}

#[derive(Clone)]
pub enum PickupType {
    PassiveDrop(Passive),
}

#[derive(Resource)]
pub struct DropTracker {
    pub progress: usize,
    pub passive_rolls: Vec<u32>,
}

impl Default for DropTracker {
    fn default() -> Self {
        DropTracker::new(Passive::iter().count())
    }
}

impl DropTracker {
    fn get_passive_progress(&self) -> Option<&u32> {
        self.passive_rolls.get(self.progress)
    }

    fn new(passive_count: usize) -> Self {
        const SPAN: u32 = 5;

        let mut rng = rand::thread_rng();

        let mut rolls = Vec::new();

        for i in 0..passive_count {
            rolls.push(SPAN * i as u32 + rng.gen_range(1..SPAN));
        }

        Self {
            progress: 0,
            passive_rolls: rolls,
        }
    }
}

fn spawn_pickups_on_death(
    mut kill_count: ResMut<KillCount>,
    mut drop_tracker: ResMut<DropTracker>,
    enemy_pos_q: Query<&GlobalTransform, (With<Enemy>, Added<Dead>)>,
    mut p_query: Query<&mut Passives, With<Player>>,
    mut commands: Commands,
) {
    //ASSUMES ONLY 1 PLAYER
    let Ok(mut passives) = p_query.get_single_mut() else {
        return;
    };

    for tr in enemy_pos_q.iter() {
        let translation = tr.translation();

        if let Some(milestone) = drop_tracker.get_passive_progress() {
            if kill_count.0 >= *milestone {
                drop_tracker.progress += 1;

                if let Some(passive) = passives.drop_random() {
                    commands.queue(SpawnPickupCommand {
                        pos: translation,
                        p_type: PickupType::PassiveDrop(passive),
                    });
                }
            }
        }
    }
}

fn display_passives_description(
    mut commands: Commands,
    mut passive_descriptions: Query<
        (
            &PassiveEntity,
            &ComputedNode,
            &mut GlobalTransform,
            &mut Visibility,
        ),
        (
            With<PassiveDescriptionNode>,
            Without<MainCamera>,
        ),
    >,
    player_query: Query<&Transform, With<Player>>,
    pickup_query: Query<(Entity, &Transform), With<PickupDrop>>,
    camera_query: Query<(&Camera, &GlobalTransform), With<MainCamera>>,
    pickup_hint: Query<Entity, With<PickupHint>>,
) {
    let Ok(p_transform) = player_query.get_single() else {
        return;
    };
    let Ok((camera, camera_transform)) = camera_query.get_single() else {
        return;
    };
    let p_pos = p_transform.translation.truncate();

    let pickup_in_range = pickup_query.iter().find(|(_, pickup_transform)| {
        let dist =
            p_pos.distance_squared(pickup_transform.translation.truncate());
        dist <= PICKUP_RANGE_SQUARED
    });

    match pickup_in_range {
        Some((pickup_entity, pickup_transform)) => {
            if pickup_hint.is_empty() {
                commands.popup().insert(PickupHint).with_children(|popup| {
                    popup.row().with_children(|row| {
                        row.text("Press ");
                        row.control_icon("F");
                        row.text(" to pick up");
                    });
                });
            }

            if let Some((
                _,
                description_node,
                mut description_transform,
                mut description_visibility,
            )) = passive_descriptions.iter_mut().find(
                |(passive_entity, _, _, _)| {
                    pickup_entity == passive_entity.get()
                },
            ) {
                *description_visibility = Visibility::Visible;
                if let Ok(pickup_viewport_pos) = camera.world_to_viewport(
                    camera_transform,
                    pickup_transform.translation,
                ) {
                    const ADDITIONAL_Y_OFFSET: f32 = 24.0;

                    let y_offset =
                        description_node.size().y + ADDITIONAL_Y_OFFSET;

                    *description_transform = GlobalTransform::from_translation(Vec3::new(
                        pickup_viewport_pos.x,
                        pickup_viewport_pos.y - y_offset,
                        0.0,
                    ));
                }
            }
        },
        None => {
            for entity in &pickup_hint {
                commands.entity(entity).despawn_recursive();
            }

            for (_, _, _, mut visibility) in &mut passive_descriptions {
                *visibility = Visibility::Hidden;
            }
        },
    }
}
