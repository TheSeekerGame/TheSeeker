use bevy::{ecs::system::Command, prelude::*, utils::hashbrown::HashMap};
use rand::Rng;
use theseeker_engine::time::GameTickUpdate;

use super::{
    attack::KillCount,
    enemy::{dead, Enemy, Tier},
    gentstate::Dead,
    player::{Passive, Passives, Player},
};

pub struct PickupPlugin;
impl Plugin for PickupPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, load_pickup_assets).add_systems(
            GameTickUpdate,
            spawn_pickups_on_death
                .after(dead)
                .run_if(resource_exists::<DropTracker>),
        );
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
            PickupType::Seed(categ, id) => {
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
    Seed(PlanetarySeed, u32),
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
    fn default() -> HashMap<Self, Vec<u32>> {
        HashMap::from_iter(vec![
            (
                Self::CategoryA,
                vec![1, 4, 5, 8, 11, 12],
            ),
            (Self::CategoryB, vec![1, 3, 4, 7, 8, 12]),
            (
                Self::CategoryC,
                vec![1, 2, 3, 4, 5, 8, 9, 11],
            ),
            (Self::CategoryD, vec![1, 2, 3, 6, 7, 8]),
            (
                Self::CategoryE,
                vec![3, 6, 7, 8, 11, 12],
            ),
        ])
    }
}

#[derive(Resource)]
pub struct DropTracker {
    pub progress: usize,
    pub passive_rolls: Vec<u32>,
    pub seeds: HashMap<PlanetarySeed, Vec<u32>>,
}

impl FromWorld for DropTracker {
    fn from_world(world: &mut World) -> Self {
        let mut passives = world.query::<&Passives>();
        let passives = &passives.single(world).locked;

        Self::reset(passives.len())
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

    fn reset(passive_count: usize) -> Self {
        const SPAN: u32 = 50;

        let mut rng = rand::thread_rng();

        let mut rolls = Vec::new();

        for i in 0..passive_count {
            rolls.push(SPAN * i as u32 + rng.gen_range(1..SPAN));
        }

        println!("DROP ROLLS: {:?}", rolls);

        Self {
            progress: 0,
            passive_rolls: rolls,
            seeds: PlanetarySeed::default(),
        }
    }

    pub fn drop_random_seed(
        &mut self,
        seed_type: &PlanetarySeed,
    ) -> Option<u32> {
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
                    Some(PlanetarySeed::CategoryA)
                } else if seed_roll < 0.0015 {
                    Some(PlanetarySeed::CategoryC)
                } else {
                    None
                }
            },
            Tier::Two => {
                if seed_roll < 0.001 {
                    Some(PlanetarySeed::CategoryA)
                } else if seed_roll < 0.0015 {
                    Some(PlanetarySeed::CategoryC)
                } else if seed_roll < 0.0017 {
                    Some(PlanetarySeed::CategoryB)
                } else if seed_roll < 0.00171 {
                    Some(PlanetarySeed::CategoryD)
                } else {
                    None
                }
            },
            Tier::Three => {
                if seed_roll < 0.001 {
                    Some(PlanetarySeed::CategoryA)
                } else if seed_roll < 0.0015 {
                    Some(PlanetarySeed::CategoryC)
                } else if seed_roll < 0.0017 {
                    Some(PlanetarySeed::CategoryB)
                } else if seed_roll < 0.00171 {
                    Some(PlanetarySeed::CategoryD)
                } else if seed_roll < 0.001711 {
                    Some(PlanetarySeed::CategoryE)
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
