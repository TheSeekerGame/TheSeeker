use bevy::{
    ecs::system::Command, prelude::*, text::BreakLineOn,
    transform::TransformSystem, ui::UiSystem, utils::hashbrown::HashMap,
};
use rand::Rng;
use strum::IntoEnumIterator;
use theseeker_engine::time::GameTickUpdate;

use crate::{
    camera::MainCamera, prelude::StateDespawnMarker, ui::popup::PopupUi,
};

use super::{
    attack::KillCount,
    enemy::{dead, Enemy, Tier},
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
    seed_map: HashMap<PlanetarySeed, String>,
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
            Passive::Sniper,
            "items/passives/Sniper.png",
        ),
    ];

    let seed_mappings: Vec<(PlanetarySeed, &str)> = vec![
        (
            PlanetarySeed::CategoryA,
            "items/seeds/a/PlanetarySeedA",
        ),
        (
            PlanetarySeed::CategoryB,
            "items/seeds/b/PlanetarySeedB",
        ),
        (
            PlanetarySeed::CategoryC,
            "items/seeds/c/PlanetarySeedC",
        ),
        (
            PlanetarySeed::CategoryD,
            "items/seeds/d/PlanetarySeedD",
        ),
        (
            PlanetarySeed::CategoryE,
            "items/seeds/e/PlanetarySeedE",
        ),
    ];

    commands.insert_resource(PickupAssetHandles {
        passive_map: HashMap::from_iter(
            passive_mappings
                .iter()
                .map(|(x, y)| (x.clone(), assets.load(*y)))
                .collect::<Vec<_>>(),
        ),
        seed_map: HashMap::from_iter(
            seed_mappings
                .iter()
                .map(|(x, y)| (x.clone(), String::from(*y)))
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
        let asset_server = world.get_resource::<AssetServer>().unwrap();

        match self.p_type.clone() {
            PickupType::PassiveDrop(passive) => {
                let texture_handle = handles.passive_map.get(&passive).unwrap();
                let transform = Transform::from_xyz(pos.x, pos.y, 50.0);

                let entity = world
                    .spawn((
                        Name::new("PickupDrop"),
                        PickupDrop::new(self.p_type),
                        SpriteBundle {
                            transform,
                            texture: texture_handle.clone(),
                            ..default()
                        },
                        StateDespawnMarker,
                    ))
                    .id();

                world.spawn((
                    Name::new("PassiveDescription"),
                    PassiveDescriptionNode,
                    PassiveEntity(entity),
                    TextBundle {
                        text: Text {
                            sections: vec![
                                TextSection::new(
                                    passive.name(),
                                    TextStyle {
                                        font_size: 24.0,
                                        ..Default::default()
                                    },
                                ),
                                TextSection::from("\n"),
                                TextSection::new(
                                    passive.description(),
                                    TextStyle {
                                        font_size: 24.0,
                                        ..Default::default()
                                    },
                                ),
                            ],
                            justify: JustifyText::Center,
                            linebreak_behavior: BreakLineOn::WordBoundary,
                        },
                        style: Style {
                            max_width: Val::Percent(33.0),
                            ..Default::default()
                        },
                        global_transform: GlobalTransform::from_translation(
                            Vec3::new(pos.x, pos.y, 50.0),
                        ),
                        background_color: BackgroundColor::from(
                            Color::BLACK.with_a(0.75),
                        ),
                        visibility: Visibility::Hidden,
                        ..Default::default()
                    },
                    StateDespawnMarker,
                ));
            },
            PickupType::Seed(categ, (id, _)) => {
                let path = &handles.seed_map[&categ];

                let texture_handle =
                    asset_server.load(format!("{path}{id}.png"));

                world.spawn((
                    PickupDrop::new(self.p_type),
                    SpriteBundle {
                        transform: Transform::from_translation(Vec3::new(
                            pos.x, pos.y, 50.0,
                        )),
                        texture: texture_handle.clone(),
                        ..default()
                    },
                ));
            },
        }
    }
}

#[derive(Clone)]
pub enum PickupType {
    PassiveDrop(Passive),
    Seed(PlanetarySeed, (u32, String)),
}

#[derive(Clone, PartialEq, Eq, Hash)]
pub enum PlanetarySeed {
    CategoryA,
    CategoryB,
    CategoryC,
    CategoryD,
    CategoryE,
}

impl PlanetarySeed {
    const PLANETARY_SEED_A1: &str = "PLANETARY_SEED_A1";
    const PLANETARY_SEED_A4: &str = "PLANETARY_SEED_A4";
    const PLANETARY_SEED_A5: &str = "PLANETARY_SEED_A5";
    const PLANETARY_SEED_A8: &str = "PLANETARY_SEED_A8";
    const PLANETARY_SEED_A11: &str = "PLANETARY_SEED_A11";
    const PLANETARY_SEED_A12: &str = "PLANETARY_SEED_A12";

    const PLANETARY_SEED_B1: &str = "PLANETARY_SEED_B1";
    const PLANETARY_SEED_B3: &str = "PLANETARY_SEED_B3";
    const PLANETARY_SEED_B4: &str = "PLANETARY_SEED_B4";
    const PLANETARY_SEED_B7: &str = "PLANETARY_SEED_B7";
    const PLANETARY_SEED_B8: &str = "PLANETARY_SEED_B8";
    const PLANETARY_SEED_B12: &str = "PLANETARY_SEED_B12";

    const PLANETARY_SEED_C1: &str = "PLANETARY_SEED_C1";
    const PLANETARY_SEED_C2: &str = "PLANETARY_SEED_C2";
    const PLANETARY_SEED_C3: &str = "PLANETARY_SEED_C3";
    const PLANETARY_SEED_C4: &str = "PLANETARY_SEED_C4";
    const PLANETARY_SEED_C5: &str = "PLANETARY_SEED_C5";
    const PLANETARY_SEED_C8: &str = "PLANETARY_SEED_C8";
    const PLANETARY_SEED_C9: &str = "PLANETARY_SEED_C9";
    const PLANETARY_SEED_C11: &str = "PLANETARY_SEED_C11";

    const PLANETARY_SEED_D1: &str = "PLANETARY_SEED_D1";
    const PLANETARY_SEED_D2: &str = "PLANETARY_SEED_D2";
    const PLANETARY_SEED_D3: &str = "PLANETARY_SEED_D3";
    const PLANETARY_SEED_D6: &str = "PLANETARY_SEED_D6";
    const PLANETARY_SEED_D7: &str = "PLANETARY_SEED_D7";
    const PLANETARY_SEED_D8: &str = "PLANETARY_SEED_D8";

    const PLANETARY_SEED_E3: &str = "PLANETARY_SEED_E3";
    const PLANETARY_SEED_E6: &str = "PLANETARY_SEED_E6";
    const PLANETARY_SEED_E7: &str = "PLANETARY_SEED_E7";
    const PLANETARY_SEED_E8: &str = "PLANETARY_SEED_E8";
    const PLANETARY_SEED_E11: &str = "PLANETARY_SEED_E11";
    const PLANETARY_SEED_E12: &str = "PLANETARY_SEED_E12";

    fn seed_map() -> HashMap<Self, Vec<(u32, String)>> {
        HashMap::from_iter(vec![
            (
                Self::CategoryA,
                vec![
                    (1, Self::PLANETARY_SEED_A1.to_string()),
                    (4, Self::PLANETARY_SEED_A4.to_string()),
                    (5, Self::PLANETARY_SEED_A5.to_string()),
                    (8, Self::PLANETARY_SEED_A8.to_string()),
                    (11, Self::PLANETARY_SEED_A11.to_string()),
                    (12, Self::PLANETARY_SEED_A12.to_string()),
                ],
            ),
            (
                Self::CategoryB,
                vec![
                    (1, Self::PLANETARY_SEED_B1.to_string()),
                    (3, Self::PLANETARY_SEED_B3.to_string()),
                    (4, Self::PLANETARY_SEED_B4.to_string()),
                    (7, Self::PLANETARY_SEED_B7.to_string()),
                    (8, Self::PLANETARY_SEED_B8.to_string()),
                    (12, Self::PLANETARY_SEED_B12.to_string()),
                ],
            ),
            (
                Self::CategoryC,
                vec![
                    (1, Self::PLANETARY_SEED_C1.to_string()),
                    (2, Self::PLANETARY_SEED_C2.to_string()),
                    (3, Self::PLANETARY_SEED_C3.to_string()),
                    (4, Self::PLANETARY_SEED_C4.to_string()),
                    (5, Self::PLANETARY_SEED_C5.to_string()),
                    (8, Self::PLANETARY_SEED_C8.to_string()),
                    (9, Self::PLANETARY_SEED_C9.to_string()),
                    (11, Self::PLANETARY_SEED_C11.to_string()),
                ],
            ),
            (
                Self::CategoryD,
                vec![
                    (1, Self::PLANETARY_SEED_D1.to_string()),
                    (2, Self::PLANETARY_SEED_D2.to_string()),
                    (3, Self::PLANETARY_SEED_D3.to_string()),
                    (6, Self::PLANETARY_SEED_D6.to_string()),
                    (7, Self::PLANETARY_SEED_D7.to_string()),
                    (8, Self::PLANETARY_SEED_D8.to_string()),
                ],
            ),
            (
                Self::CategoryE,
                vec![
                    (3, Self::PLANETARY_SEED_E3.to_string()),
                    (6, Self::PLANETARY_SEED_E6.to_string()),
                    (7, Self::PLANETARY_SEED_E7.to_string()),
                    (8, Self::PLANETARY_SEED_E8.to_string()),
                    (11, Self::PLANETARY_SEED_E11.to_string()),
                    (12, Self::PLANETARY_SEED_E12.to_string()),
                ],
            ),
        ])
    }
}

#[derive(Resource)]
pub struct DropTracker {
    pub progress: usize,
    pub passive_rolls: Vec<u32>,
    pub seeds: HashMap<PlanetarySeed, Vec<(u32, String)>>,
}

impl Default for DropTracker {
    fn default() -> Self {
        DropTracker::new(Passive::iter().count())
    }
}

impl DropTracker {
    fn get_passive_progress(&self) -> Option<&u32> {
        println!(
            "{};{:?}",
            self.progress, self.passive_rolls
        );

        self.passive_rolls.get(self.progress)
    }

    fn new(passive_count: usize) -> Self {
        const SPAN: u32 = 10;

        let mut rng = rand::thread_rng();

        let mut rolls = Vec::new();

        for i in 0..passive_count {
            rolls.push(SPAN * i as u32 + rng.gen_range(1..SPAN));
        }

        println!("DROP ROLLS: {:?}", rolls);

        Self {
            progress: 0,
            passive_rolls: rolls,
            seeds: PlanetarySeed::seed_map(),
        }
    }

    pub fn drop_random_seed(
        &mut self,
        seed_type: &PlanetarySeed,
    ) -> Option<(u32, String)> {
        let mut rng = rand::thread_rng();

        if !self.seeds[seed_type].is_empty() {
            let i = rng.gen_range(0..self.seeds[seed_type].len());
            let seed = self.seeds.get_mut(seed_type).unwrap().swap_remove(i);

            return Some(seed);
        }
        None
    }
}

fn spawn_pickups_on_death(
    mut kill_count: ResMut<KillCount>,
    mut drop_tracker: ResMut<DropTracker>,
    enemy_q: Query<(&GlobalTransform, &Tier), (With<Enemy>, Added<Dead>)>,
    mut p_query: Query<&mut Passives, With<Player>>,
    mut commands: Commands,
) {
    //ASSUMES ONLY 1 PLAYER
    let Ok(mut passives) = p_query.get_single_mut() else {
        return;
    };

    for (tr, tier) in enemy_q.iter() {
        let translation = tr.translation();

        println!("PRE-DROPPING PASSIVE");

        let mut rng = rand::thread_rng();

        let seed_roll = rng.gen_range(0.0..1.0);

        println!("seed roll: {}", seed_roll);

        let seed_category: Option<PlanetarySeed> = match tier {
            Tier::Base => {
                if seed_roll < 0.001 {
                    Some(PlanetarySeed::CategoryC)
                } else if seed_roll < 0.005 {
                    Some(PlanetarySeed::CategoryA)
                } else {
                    None
                }
            },
            Tier::Two => {
                if seed_roll < 0.0007 {
                    Some(PlanetarySeed::CategoryD)
                } else if seed_roll < 0.003 {
                    Some(PlanetarySeed::CategoryB)
                } else if seed_roll < 0.006 {
                    Some(PlanetarySeed::CategoryC)
                } else if seed_roll < 0.012 {
                    Some(PlanetarySeed::CategoryA)
                } else {
                    None
                }
            },
            Tier::Three => {
                if seed_roll < 0.0005 {
                    Some(PlanetarySeed::CategoryE)
                } else if seed_roll < 0.001 {
                    Some(PlanetarySeed::CategoryD)
                } else if seed_roll < 0.005 {
                    Some(PlanetarySeed::CategoryB)
                } else if seed_roll < 0.01 {
                    Some(PlanetarySeed::CategoryC)
                } else if seed_roll < 0.1 {
                    Some(PlanetarySeed::CategoryA)
                } else {
                    None
                }
            },
        };

        if let Some(seed_category) = seed_category {
            let seed_id = drop_tracker.drop_random_seed(&seed_category);

            if let Some(seed_id) = seed_id {
                commands.add(SpawnPickupCommand {
                    pos: translation,
                    p_type: PickupType::Seed(seed_category, seed_id),
                });
            }
        }

        // category A 1/1000 drop chance
        // all tiers
        // category C 1/2000 drop chance
        // all tiers
        // category B 1/5000 drop chance
        // Tiers 2 and 3
        // category D 1/10000 drop chance
        // Tiers 2 and 3
        // category E 1/100000 drop chance
        // Tier 3 only

        if let Some(milestone) = drop_tracker.get_passive_progress() {
            if kill_count.0 >= *milestone {
                drop_tracker.progress += 1;

                if let Some(passive) = passives.drop_random() {
                    println!("DROPPING PASSIVE");
                    commands.add(SpawnPickupCommand {
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
            &Node,
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
                if let Some(pickup_viewport_pos) = camera.world_to_viewport(
                    camera_transform,
                    pickup_transform.translation,
                ) {
                    const ADDITIONAL_Y_OFFSET: f32 = 24.0;

                    let y_offset =
                        description_node.size().y + ADDITIONAL_Y_OFFSET;

                    *description_transform = GlobalTransform::from_xyz(
                        pickup_viewport_pos.x,
                        pickup_viewport_pos.y - y_offset,
                        description_transform.translation().z,
                    );
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
