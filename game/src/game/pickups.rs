use bevy::{ecs::system::Command, prelude::*, utils::hashbrown::HashMap};
use rand::Rng;
use theseeker_engine::{physics::LinearVelocity, time::{GameTickUpdate, GameTime}};

use super::{attack::KillCount, enemy::{dead, Enemy}, gentstate::Dead, player::{Passive, Passives, Player}};

pub struct PickupPlugin;
impl Plugin for PickupPlugin {
    fn build(&self, app: &mut App) {
        app
        .add_systems(Startup, load_pickup_assets)
        .add_systems(GameTickUpdate, (
            spawn_pickups_on_death.after(dead).run_if(resource_exists::<DropTracker>),
            update_orbs_pos,
            update_orbs_vel,
        ));
    }
}

#[derive(Component)]
pub struct PickupDrop {
    pub p_type: PickupType,
//    pickup_timer: Timer
}

impl PickupDrop {
    pub fn new(p_type: PickupType) -> Self {
        Self { 
            p_type, 
            //pickup_timer : Timer::from_seconds(2.0, TimerMode::Once)
        }
    }
}

#[derive(Resource)]
pub struct PickupAssetHandles {
    passive_map: HashMap<Passive, Handle<Image>>,
    seed_map: HashMap<PlanetarySeed, Handle<Image>>,
}


pub fn load_pickup_assets(assets: Res<AssetServer>, mut commands: Commands) {

    let passive_mappings: Vec<(Passive, &str)> = vec![

        (Passive::Bloodstone, "items/passives/Bloodstone.png"),
        (Passive::FlamingHeart, "items/passives/FlamingHeart.png"),
        (Passive::IceDagger, "items/passives/IceDagger.png"),
        (Passive::GlowingShard, "items/passives/GlowingShard.png"),
        (Passive::ObsidianNecklace, "items/passives/ObsidianNecklace.png"),
        (Passive::HeavyBoots, "items/passives/HeavyBoots.png"),
        (Passive::SerpentRing, "items/passives/SerpentRing.png"),
        (Passive::FrenziedAttack, "items/passives/FrenziedAttack.png"),

    ];

    let seed_mappings: Vec<(PlanetarySeed, &str)> = vec![
    
        (PlanetarySeed::CategoryA, "items/seeds/a/PlanetarySeedA"),
        (PlanetarySeed::CategoryB, "items/seeds/b/PlanetarySeedB"),
        (PlanetarySeed::CategoryC, "items/seeds/c/PlanetarySeedC"),
        (PlanetarySeed::CategoryD, "items/seeds/d/PlanetarySeedD"),
        (PlanetarySeed::CategoryE, "items/seeds/e/PlanetarySeedE"),

    ];

    commands.insert_resource(
        PickupAssetHandles{
            passive_map: HashMap::from_iter(
                passive_mappings.iter().map(|(x, y)|
                    (x.clone(), assets.load(*y))).collect::<Vec<_>>()
            ),
            seed_map: HashMap::from_iter(
                seed_mappings.iter().map(|(x, y)|
                    (x.clone(), assets.load(*y))).collect::<Vec<_>>()
            )
        }
    );
}


pub struct SpawnPickupCommand{
    pos: Vec3,
    p_type: PickupType,
}
impl Command for SpawnPickupCommand {
    fn apply(self, world: &mut World) {
        let pos = self.pos;

        let handles = world.get_resource::<PickupAssetHandles>().unwrap();


        match self.p_type.clone() {
            PickupType::None => {
                
            },
            PickupType::PassiveDrop(passive) => {

                let texture_handle = handles.passive_map.get(&passive).unwrap();


                world.spawn((
                    PickupDrop::new(self.p_type),
                    
                    SpriteBundle {
                        //sprite: Sprite { 
                        //    ..default()
                        //},
                        transform: Transform::from_translation(Vec3::new(pos.x, pos.y, 50.0)),
                        texture: texture_handle.clone(),
                        ..default()
                    }
                ));
        

            },
            PickupType::PlanetarySeed => {

                //category A 1/1000 drop chance
                //category C 1/2000 drop chance
                //category B 1/5000 drop chance
                //category D 1/10000 drop chance
                //category E 1/100000 drop chance

            },
        }

        
        
    }
}


#[derive(Clone)]
pub enum PickupType {
    None,
    PassiveDrop(Passive),
    PlanetarySeed,
}

struct DropTable(Vec<(f32, PickupType)>);

#[derive(Resource)]
pub struct DropTableRes {

    pub table : DropTable

}


impl DropTable {

    fn build_table_from_rates(data : Vec<(f32, PickupType)>) -> Self {

        let mut nvec = Vec::new();

        let mut sum_p = 0.0;

        for (rate, p_type) in data {

            sum_p += rate;
            
            nvec.push((sum_p, p_type));
            
            if sum_p >= 1.0 {
                break;
            }
        }

        Self(nvec)
    }

    fn roll_table(&self) -> PickupType {

        let mut rng = rand::thread_rng();
    
        let roll = rng.gen_range(0.0..1.0);
    
        for (rate, drop) in self.0.iter() {
        
            if roll <= *rate {
                return drop.clone();
            }
    
        }
        return PickupType::None
    }
    
}

#[derive(Clone, PartialEq, Eq, Hash)]
pub enum PlanetarySeed {
    None,
    CategoryA,
    CategoryB,
    CategoryC,
    CategoryD,
    CategoryE,
}

impl PlanetarySeed {
    fn default() -> HashMap<Self, Vec<u32>> {

        HashMap::from_iter(vec![
                (Self::CategoryA, vec![]),
                (Self::CategoryB, vec![]),
                (Self::CategoryC, vec![]),
                (Self::CategoryD, vec![]),
                (Self::CategoryE, vec![]),
            ]
        )

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

        let mut passives = world.query::<(&Passives)>();
        let passives = &passives.single(world).locked;

        Self::reset(passives.len())

    }
}

impl DropTracker {
    fn get_passive_progress(&self) -> Option<&u32> {
        println!("{};{:?}", self.progress, self.passive_rolls);

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
    
    //pub fn drop_random(&mut self) -> Option<Passive> {
    //
    //    let mut rng = rand::thread_rng();
    //
    //    if !self.locked.is_empty() {
    //        let i = rng.gen_range(0..self.locked.len());
    //        let passive = self.locked.swap_remove(i);
        //
    //        return Some(passive)
    //    }
    //    None
    //}

    fn roll_seed(&mut self) -> (PlanetarySeed, u32){

        let mut rng = rand::thread_rng();

        let seed_roll = rng.gen_range(0.0..1.0);
        //
        //
        //  category A 1/1000 drop chance
        //  category C 1/2000 drop chance
        //  category B 1/5000 drop chance
        //  category D 1/10000 drop chance
        //  category E 1/100000 drop chance
        //
        //

        return (PlanetarySeed::None, 0);

        //let drop: Option<(PlanetarySeed, u32)> = {
        //    if seed_roll < 0.00001 && self.seeds[Planetary::] {
        //        // drop E
        //    }
        //    if seed_roll < 0.001 {
        //        
        //    }
        //}
    }

    fn seed_cap(&self, seed_type: &PlanetarySeed) {

        //self.seeds[seed_type].len()

    }

}

#[derive(Component)]
pub struct XpOrb{
    init_timer: f32, 
}

fn spawn_pickups_on_death(
    mut kill_count: ResMut<KillCount>,
    mut drop_tracker: ResMut<DropTracker>,
    enemy_q: Query<
        &GlobalTransform,
        (
            With<Enemy>,
            Added<Dead>,
        ),
    >,
    mut p_query: Query<&mut Passives, With<Player>>,
    mut commands: Commands,
) {

    let size = Vec2::splat(2.0);

    //println!("spawn system");
    //ASSUMES ONLY 1 PLAYER
    let Ok(mut passives) = p_query.get_single_mut() else {return};

    //println!("spawn system post let Ok");

    for tr in enemy_q.iter() {

        let translation = tr.translation();
        
        /*
        let mut rng = rand::thread_rng();

        let init_vel = Vec2::new(0.0, 2.0);
        const POS_RADIUS: f32 = 3.0;
        for _ in 0..4 {

            let pos = Vec2::new(
                rng.gen_range(-POS_RADIUS..POS_RADIUS),
                rng.gen_range(-POS_RADIUS..POS_RADIUS),
            ).clamp_length_max(POS_RADIUS);

            let vel = pos * 0.25;

            commands.spawn((
                LinearVelocity(vel + init_vel),
                XpOrb{
                    init_timer: 4.0,
                },
                SpriteBundle {
                    sprite: Sprite {
                        color: Color::WHITE,
                        custom_size: Some(size),
                    ..default()
                },
                transform: Transform::from_translation(translation + pos.extend(0.)),
                ..default()
                }
            ));
        }
         */
        println!("PRE-DROPPING PASSIVE");

        let mut rng = rand::thread_rng();

        let seed_roll = rng.gen_range(0.0..1.0);

        //category A 1/1000 drop chance
        //category C 1/2000 drop chance
        //category B 1/5000 drop chance
        //category D 1/10000 drop chance
        //category E 1/100000 drop chance

        //drop_tracker
        //if seed_roll 

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

const DIST_THRESHOLD: f32 = 0.75;

fn update_orbs_vel(
    mut commands: Commands,
    mut query: Query<(Entity, &GlobalTransform, &mut LinearVelocity, &XpOrb)>,
    mut p_query: Query<&GlobalTransform, With<Player>>,
) {

    let Ok(p) = p_query.get_single() else {return};
    

    let p_pos = p.translation().truncate();

    for (entity, mut tr, mut vel, xp_orb) in query.iter_mut() {

        if xp_orb.init_timer > 0.0 {
            continue;
        }

        let pos = tr.translation().truncate();
        let dist = p_pos.distance(pos);

        let dir = (p_pos - pos).normalize();

        if dist < DIST_THRESHOLD {
            commands.entity(entity).despawn();
        }
        else {
            const SPEEDUP_DIST: f32 = 150.0;
            //let scaled_dist = ((100.0 - dist).powi(2) / 100.).clamp(0.0, 2.);
            let scaled_dist = (2. * (SPEEDUP_DIST - dist.min(SPEEDUP_DIST))/SPEEDUP_DIST).powi(2);
            vel.0 = dir * (1.0 + scaled_dist * 2.0 ) * 25.0;
        }
    }
}

fn update_orbs_pos(
    mut query: Query<(&mut Transform, &LinearVelocity, &mut XpOrb)>,
    time: Res<GameTime>,
) {

    let delta = 1.0 / time.hz as f32;

    for (mut tr, vel, mut xp_orb) in query.iter_mut() {

        tr.translation += vel.0.extend(0.) * delta;

        if xp_orb.init_timer > 0.0 {
            xp_orb.init_timer -= delta;
        }
    }
}