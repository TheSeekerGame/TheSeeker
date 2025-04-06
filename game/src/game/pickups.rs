use bevy::{
    prelude::*, text::LineBreak, transform::TransformSystem, ui::UiSystem,
    utils::hashbrown::HashMap,
};
use bevy_hanabi::{
    Attribute, ColorOverLifetimeModifier, EffectAsset, ExprWriter, Gradient,
    ParticleEffect, ParticleEffectBundle, SetAttributeModifier,
    SetPositionSphereModifier, SetVelocitySphereModifier, ShapeDimension, SizeOverLifetimeModifier, Spawner,
};
use rand::Rng;
use strum::IntoEnumIterator;
use theseeker_engine::time::{GameTickUpdate, GameTime};

use crate::{
    camera::{MainCamera, CameraShake},
    prelude::StateDespawnMarker, ui::popup::PopupUi,
};

use super::{
    attack::KillCount,
    enemy::{dead, Enemy, Tier},
    gentstate::Dead,
    player::{Passive, Passives, Player, PlayerConfig},
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
                hover_animation_system.run_if(any_with_component::<HoverAnimation>),
                pickup_glow_system,
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

// Component to mark entities that should glow when player is near
#[derive(Component)]
pub struct GlowEffect;

#[derive(Resource)]
pub struct PickupParticleEffectHandle(pub Handle<EffectAsset>);

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

pub fn load_pickup_assets(
    assets: Res<AssetServer>,
    mut commands: Commands,
    mut effects: ResMut<Assets<EffectAsset>>,
) {
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

    // Create the pickup particle effect
    let mut color_gradient = Gradient::new();
    // Increase brightness AGAIN for MORE bloom!
    color_gradient.add_key(0.0, Vec4::new(10.0, 10.0, 8.0, 1.0)); // MAX BRIGHTNESS Yellowish start
    color_gradient.add_key(0.7, Vec4::new(10.0, 10.0, 6.0, 1.0)); // MAX BRIGHTNESS Yellowish mid
    color_gradient.add_key(1.0, Vec4::new(1.0, 1.0, 0.4, 0.0)); // Fade out (no bloom needed)

    let mut size_gradient = Gradient::new();
    // Set specific particle sizes as requested
    size_gradient.add_key(0.0, Vec3::splat(1.0)); 
    size_gradient.add_key(0.5, Vec3::splat(1.25));
    size_gradient.add_key(1.0, Vec3::splat(0.5));

    let writer = ExprWriter::new();

    let age = writer.lit(0.).expr();
    let init_age = SetAttributeModifier::new(Attribute::AGE, age);

    let lifetime = writer.lit(1.5).expr(); // Particle lifetime
    let init_lifetime = SetAttributeModifier::new(Attribute::LIFETIME, lifetime);

    let init_pos = SetPositionSphereModifier {
        center: writer.lit(Vec3::ZERO).expr(),
        radius: writer.lit(5.).expr(), // Spawn within a small radius
        dimension: ShapeDimension::Volume,
    };

    // Set constant upward velocity
    let init_vel = SetAttributeModifier::new(Attribute::VELOCITY, writer.lit(Vec3::Y * 15.0).expr());

    let effect = effects.add(
        EffectAsset::new(32, Spawner::rate(2.5.into()), writer.finish())
            .with_name("pickup_particles")
            .init(init_pos)
            // Use the new constant velocity modifier
            .init(init_vel) 
            .init(init_age)
            .init(init_lifetime)
            .render(SizeOverLifetimeModifier { gradient: size_gradient, screen_space_size: false })
            .render(ColorOverLifetimeModifier { gradient: color_gradient }),
    );

    commands.insert_resource(PickupParticleEffectHandle(effect));
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
        // Get the particle effect handle
        let particle_effect_handle = world.get_resource::<PickupParticleEffectHandle>().unwrap();
        // Clone the handle *before* spawning
        let effect_handle_clone = particle_effect_handle.0.clone();

        match self.p_type.clone() {
            PickupType::PassiveDrop(passive) => {
                let texture_handle = handles.passive_map.get(&passive).unwrap();
                let transform = Transform::from_xyz(pos.x, pos.y, 50.0);

                // Spawn the main pickup entity
                let mut entity_commands = world.spawn((
                    Name::new("PickupDrop"),
                    PickupDrop::new(self.p_type.clone()), // Clone p_type here
                    SpriteBundle {
                        transform,
                        sprite: texture_handle.clone().into(),
                        ..default()
                    },
                    GlowEffect, // Add the GlowEffect marker
                    HoverAnimation {
                        base_y: pos.y,
                        base_position: pos.xy(),
                        amplitude: 2.0,
                        frequency: 0.375,
                        time: rand::thread_rng().gen_range(0.0..std::f32::consts::TAU),
                    },
                    StateDespawnMarker,
                ));

                // Add the particle effect as a child
                entity_commands.with_children(|parent| {
                    // Spawn particles behind the item
                    parent.spawn((
                        Name::new("PickupParticlesBack"),
                        ParticleEffectBundle {
                            effect: ParticleEffect::new(effect_handle_clone.clone()), // Clone handle again for the second emitter
                            // Make particles relative to the parent item's transform, further behind
                            transform: Transform::from_xyz(0.0, 0.0, 1.0), 
                            ..default()
                        },
                    ));
                    // Spawn particles in front of the item
                    parent.spawn((
                        Name::new("PickupParticlesFront"),
                        ParticleEffectBundle {
                            effect: ParticleEffect::new(effect_handle_clone),
                            // Make particles relative to the parent item's transform, further in front
                            transform: Transform::from_xyz(0.0, 0.0, -1.0), 
                            ..default()
                        },
                    ));
                });

                let entity = entity_commands.id(); // Get the ID after spawning children

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
                        sprite: texture_handle.clone().into(),
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
        const SPAN: u32 = 5;

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
    player_config: Res<PlayerConfig>,
) {
    //ASSUMES ONLY 1 PLAYER
    let Ok(mut passives) = p_query.get_single_mut() else {
        return;
    };

    let mut enemy_killed = false;
    for (tr, tier) in enemy_q.iter() {
        enemy_killed = true;
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
                commands.queue(SpawnPickupCommand {
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
                    commands.queue(SpawnPickupCommand {
                        pos: translation,
                        p_type: PickupType::PassiveDrop(passive),
                    });
                }
            }
        }
    }

    // Trigger screen shake if any enemy died this frame
    if enemy_killed {
        // Insert the CameraShake resource instead of sending an event
        commands.insert_resource(CameraShake::new(
            player_config.on_kill_screenshake_strength,
            player_config.on_kill_screenshake_duration_secs,
            player_config.on_kill_screenshake_frequency,
        ));
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
    pickup_query: Query<(Entity, &HoverAnimation), With<PickupDrop>>,
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

    let pickup_in_range = pickup_query.iter().find(|(_, hover)| {
        let dist =
            p_pos.distance_squared(hover.base_position);
        dist <= PICKUP_RANGE_SQUARED
    });

    match pickup_in_range {
        Some((pickup_entity, hover)) => {
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
                    Vec3::new(hover.base_position.x, hover.base_position.y, 50.0),
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

#[derive(Component)]
pub struct HoverAnimation {
    pub base_y: f32,
    pub base_position: Vec2,
    pub amplitude: f32,
    pub frequency: f32,
    pub time: f32,
}

fn hover_animation_system(
    time: Res<GameTime>,
    mut query: Query<(&mut Transform, &mut HoverAnimation), With<PickupDrop>>,
) {
    let delta = 1.0 / time.hz as f32;
    for (mut transform, mut hover) in query.iter_mut() {
        hover.time += delta;
        let offset = hover.amplitude * (hover.time * hover.frequency * std::f32::consts::TAU).sin();
        transform.translation.y = hover.base_y + offset;
    }
}

// System to make pickups glow based on player proximity
fn pickup_glow_system(
    player_query: Query<&Transform, (With<Player>, Without<PickupDrop>)>, 
    mut pickup_query: Query<(&HoverAnimation, &mut Sprite), (With<PickupDrop>, With<GlowEffect>)>, 
) {
    // Define distances and colors for interpolation
    const MAX_GLOW_DISTANCE: f32 = 20.0;
    const MAX_GLOW_DISTANCE_SQUARED: f32 = MAX_GLOW_DISTANCE * MAX_GLOW_DISTANCE; // 400.0
    // Inner distance uses the existing pickup range for max brightness
    const INNER_GLOW_DISTANCE_SQUARED: f32 = PICKUP_RANGE_SQUARED; // 100.0

    const BASE_COLOR: Color = Color::WHITE;
    // Current bright color for max glow
    const MAX_GLOW_COLOR: Color = Color::rgb(4.0, 4.0, 3.0);

    if let Ok(player_transform) = player_query.get_single() {
        let player_pos = player_transform.translation.xy();

        for (hover, mut sprite) in pickup_query.iter_mut() {
            let pickup_base_pos = hover.base_position; 
            let distance_squared = player_pos.distance_squared(pickup_base_pos);

            let target_color = if distance_squared <= INNER_GLOW_DISTANCE_SQUARED {
                // Within inner radius: Max brightness
                MAX_GLOW_COLOR
            } else if distance_squared >= MAX_GLOW_DISTANCE_SQUARED {
                // Outside outer radius: Base color
                BASE_COLOR
            } else {
                // Between inner and outer radius: Interpolate factor t
                let t = 1.0 - (distance_squared - INNER_GLOW_DISTANCE_SQUARED) / (MAX_GLOW_DISTANCE_SQUARED - INNER_GLOW_DISTANCE_SQUARED);
                let t = t.clamp(0.0, 1.0);

                // Interpolate using the factor t and Color::rgba
                // We need the actual component values of the const colors here.
                // Let's define them explicitly.
                const BASE_R: f32 = 1.0;
                const BASE_G: f32 = 1.0;
                const BASE_B: f32 = 1.0;
                const BASE_A: f32 = 1.0;
                const MAX_R: f32 = 4.0;
                const MAX_G: f32 = 4.0;
                const MAX_B: f32 = 3.0;
                const MAX_A: f32 = 1.0;

                let r = BASE_R * (1.0 - t) + MAX_R * t;
                let g = BASE_G * (1.0 - t) + MAX_G * t;
                let b = BASE_B * (1.0 - t) + MAX_B * t;
                let a = BASE_A * (1.0 - t) + MAX_A * t;
                
                Color::rgba(r, g, b, a)
            };

            // Only update if the color needs to change
            if sprite.color != target_color {
                 sprite.color = target_color;
            }
        }
    } else {
        // If player doesn't exist, ensure all items are at base color
        for (_, mut sprite) in pickup_query.iter_mut() { 
             if sprite.color != BASE_COLOR {
                sprite.color = BASE_COLOR;
            }
        }
    }
}
