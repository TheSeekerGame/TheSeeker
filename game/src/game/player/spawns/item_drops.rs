//! Item drop system for world loot.
//!
//! Materials appear inside semi-transparent cubes. The cube acts as a physics
//! container with gravity, while the material inside oscillates until it
//! reaches the player's backpack.

use theseeker_engine::physics::{
    geometry::Axis, Collider, CollisionGroups, Group, LinearVelocity, TileCollisionWorld, GROUND,
};
use crate::game::player::equipment::BackpackAnchor;
use crate::game::player::{
    CraftMaterial, MaterialInventory, Player,
};
use crate::prelude::*;
use rand::Rng;
use theseeker_engine::effects::{SpriteScale, SpriteStretch, StretchAnchor};
use theseeker_engine::gent::TransformGfxFromGent;
use theseeker_engine::time::GameTickUpdate;

pub struct ItemDropsPlugin;

#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct ItemDropSet;

/// A dropped material entered the player's backpack.
#[derive(Message, Debug, Clone, Copy)]
pub(crate) struct ItemPickedUp;

impl Plugin for ItemDropsPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<ItemPickedUp>();
        app.add_systems(Startup, load_drop_assets);
        app.add_systems(
            GameTickUpdate,
            (
                update_item_cube_physics,
                attract_and_collect_items,
                update_dropped_item_position,
            )
                .chain()
                .in_set(ItemDropSet)
                .run_if(in_state(AppState::InGame)),
        );
    }
}

// Physics constants
/// Corpse loot probability (50%)
pub(crate) const ENEMY_CORPSE_LOOT_PROBABILITY: f32 = 0.50;
/// Initial upward velocity magnitude (px/tick)
const DROP_LAUNCH_SPEED: f32 = 1.6;
/// Minimum launch angle from vertical (radians) - ~6 degrees
const DROP_ANGLE_MIN: f32 = 0.1;
/// Maximum launch angle from vertical (radians) - ~26 degrees
const DROP_ANGLE_MAX: f32 = 0.45;
/// Bounce coefficient (how much velocity is retained after bouncing)
const BOUNCE_COEFFICIENT: f32 = 0.4;
/// Minimum velocity for bouncing (below this, just stop)
const BOUNCE_THRESHOLD: f32 = 0.5;
/// Maximum number of bounces before settling
const MAX_BOUNCES: u8 = 3;

// Attraction constants
/// Range within which items start attracting towards player (pixels)
const ATTRACTION_RANGE: f32 = 32.0;
/// Maximum attraction speed (px/tick) - additive to existing velocity
const ATTRACTION_MAX_SPEED: f32 = 4.0;
/// Attraction acceleration per tick (how fast attraction builds up)
const ATTRACTION_ACCEL: f32 = 0.4;
/// Ticks to reach full attraction speed (time-based ramp up)
const ATTRACTION_RAMP_TICKS: u32 = 24;
/// Distance at which a drop locks onto the backpack (pixels)
const BACKPACK_PICKUP_DISTANCE: f32 = 6.0;

// Oscillation constants
/// Vertical bob amplitude (pixels)
const OSCILLATION_AMPLITUDE: f32 = 1.5;
/// Bob frequency (radians/tick)
const OSCILLATION_SPEED: f32 = 0.03;
/// Visual offset for item sprites above their physics collider (pixels)
const VISUAL_OFFSET_Y: f32 = 4.0;

/// Gravity acceleration per tick (negative = downward)
const GRAVITY_PER_TICK: f32 = -0.12;
/// Terminal velocity (maximum falling speed)
const TERMINAL_VELOCITY: f32 = -2.5;

#[derive(Debug, Clone, Copy)]
pub(crate) enum DropLaunchMode {
    Random,
}

/// Component for the physics-enabled cube container (no visual - visual is separate).
#[derive(Component)]
pub struct ItemCube {
    /// Whether the cube has landed on the ground
    grounded: bool,
    /// Delay before the drop can attract to or be picked up by the player
    pickup_delay_ticks: u32,
    /// Current attraction velocity (additive to base movement)
    attraction_vel: Vec2,
    /// Whether the squish animation has been applied (true once attraction starts)
    squish_applied: bool,
    /// Once true, attraction is permanent (item cannot escape the pull)
    locked_attraction: bool,
    /// Ticks since attraction started (used for time-based speed ramp)
    attraction_ticks: u32,
    /// Number of bounces performed (limited to prevent infinite bouncing)
    bounce_count: u8,
    /// Entity of the visual representation
    gfx_entity: Entity,
}

/// Marker component for the visual representation of an ItemCube.
#[derive(Component)]
pub struct ItemCubeGfx;

/// Component for the actual collectible item inside the cube.
#[derive(Component)]
pub struct DroppedItem {
    /// The material contained in this drop.
    pub material: CraftMaterial,
    /// Entity of the parent cube
    pub cube_entity: Entity,
    /// Phase for oscillation animation (radians)
    oscillation_phase: f32,
}

/// Resource holding preloaded drop asset handles.
#[derive(Resource)]
pub(crate) struct DropAssetHandles {
    cube: Handle<Image>,
    items: std::collections::HashMap<CraftMaterial, Handle<Image>>,
}

fn load_drop_assets(mut commands: Commands, asset_server: Res<AssetServer>) {
    let mut items = std::collections::HashMap::new();

    for item in CraftMaterial::STORAGE {
        items.insert(
            item,
            asset_server.load(item.icon_path()),
        );
    }

    commands.insert_resource(DropAssetHandles {
        cube: asset_server.load("items/drops/ItemCube.png"),
        items,
    });
}

/// The material an enemy archetype drops, if any.
pub(crate) fn enemy_drop_material(
    arch_id: &str,
) -> Option<CraftMaterial> {
    match arch_id {
        id if id.starts_with("bat_") => Some(CraftMaterial::BatWing),
        "spider_small" | "spider_big" => Some(CraftMaterial::IceShard),
        "spider_small_tundra" | "spider_big_tundra" => Some(CraftMaterial::MossFragment),
        "opi" => Some(CraftMaterial::PlantFiber),
        "squid" => Some(CraftMaterial::Ink),
        "buzzer" => Some(CraftMaterial::AcidGland),
        "void" => Some(CraftMaterial::NullPearl),
        _ => None,
    }
}

/// Tentacles are procedural bodies without an archetype.
pub(crate) const TENTACLE_DROP_MATERIAL: CraftMaterial = CraftMaterial::TentacleTip;

fn build_launch_velocity(
    _origin: Vec2,
    launch: DropLaunchMode,
    rng: &mut impl Rng,
) -> Vec2 {
    let horizontal_dir = match launch {
        DropLaunchMode::Random => {
            if rng.random::<bool>() {
                1.0
            } else {
                -1.0
            }
        },
    };

    let angle_offset = rng.random_range(DROP_ANGLE_MIN..DROP_ANGLE_MAX);
    let angle =
        std::f32::consts::FRAC_PI_2 + (angle_offset * horizontal_dir);
    Vec2::new(angle.cos(), angle.sin()) * DROP_LAUNCH_SPEED
}

fn spawn_material_drop_instance(
    commands: &mut Commands,
    assets: &DropAssetHandles,
    material: CraftMaterial,
    origin: Vec2,
    launch_vel: Vec2,
    pickup_delay_ticks: u32,
    rng: &mut impl Rng,
) {
    let cube_gfx_entity = commands
        .spawn((
            Name::new("ItemCubeGfx"),
            ItemCubeGfx,
            Sprite {
                image: assets.cube.clone(),
                ..default()
            },
            Transform::from_translation(origin.extend(14.9 * 0.000_001)),
            GlobalTransform::default(),
            Visibility::Visible,
            InheritedVisibility::VISIBLE,
            ViewVisibility::default(),
            StateDespawnMarker,
        ))
        .id();

    let cube_entity = commands
        .spawn((
            Name::new("ItemCube"),
            ItemCube {
                grounded: false,
                pickup_delay_ticks,
                attraction_vel: Vec2::ZERO,
                squish_applied: false,
                locked_attraction: false,
                attraction_ticks: 0,
                bounce_count: 0,
                gfx_entity: cube_gfx_entity,
            },
            LinearVelocity(launch_vel),
            Collider::cuboid(4.0, 4.0),
            CollisionGroups::new(Group::NONE, GROUND),
            Transform::from_translation(origin.extend(14.9 * 0.000_001)),
            GlobalTransform::default(),
            StateDespawnMarker,
        ))
        .id();

    commands
        .entity(cube_gfx_entity)
        .insert(TransformGfxFromGent {
            pixel_aligned: true,
            gent: cube_entity,
            offset: Some(Vec3::new(0.0, VISUAL_OFFSET_Y, 0.0)),
        });

    if let Some(item_image) = assets.items.get(&material).cloned() {
        commands.spawn((
            Name::new(format!("DroppedItem_{material:?}")),
            DroppedItem {
                material,
                cube_entity,
                oscillation_phase: rng
                    .random_range(0.0..std::f32::consts::TAU),
            },
            Sprite {
                image: item_image,
                ..default()
            },
            Transform::from_translation(origin.extend(14.95 * 0.000_001)),
            GlobalTransform::default(),
            Visibility::Visible,
            InheritedVisibility::VISIBLE,
            ViewVisibility::default(),
            StateDespawnMarker,
        ));
    }
}

pub(crate) fn spawn_material_drop_burst(
    commands: &mut Commands,
    assets: &DropAssetHandles,
    material: CraftMaterial,
    amount: u32,
    origin: Vec2,
    launch: DropLaunchMode,
    pickup_delay_ticks: u32,
) {
    if amount == 0 {
        return;
    }

    let mut rng = rand::rng();
    for _ in 0..amount {
        let launch_vel = build_launch_velocity(origin, launch, &mut rng);
        spawn_material_drop_instance(
            commands,
            assets,
            material,
            origin,
            launch_vel,
            pickup_delay_ticks,
            &mut rng,
        );
    }
}

fn update_item_cube_physics(
    mut query: Query<(
        &mut ItemCube,
        &mut LinearVelocity,
        &mut Transform,
        &Collider,
    )>,
    collision_world: Option<Res<TileCollisionWorld>>,
) {
    let collision_world = collision_world.as_deref().filter(|w| w.is_ready());
    for (mut cube, mut velocity, mut transform, collider) in query.iter_mut() {
        if cube.grounded
            && move_grounded_item_cube_by_attraction(
                &mut cube,
                &mut velocity,
                &mut transform,
            )
        {
            continue;
        }

        if cube.locked_attraction {
            move_item_cube_by_attraction(&cube, &mut velocity, &mut transform);
            continue;
        }

        velocity.0.y = (velocity.0.y + GRAVITY_PER_TICK).max(TERMINAL_VELOCITY);

        let original_pos = transform.translation.xy();
        let z = transform.translation.z;
        let Some(collision_world) = collision_world else {
            transform.translation = (original_pos + velocity.0).extend(z);
            continue;
        };
        let Some(half_extents) = collider.cuboid_half_extents() else {
            transform.translation = (original_pos + velocity.0).extend(z);
            continue;
        };

        let (resolved_y, blocked_y) = collision_world.resolve_axis(
            Axis::Y,
            original_pos,
            velocity.0.y,
            half_extents,
        );
        if blocked_y {
            resolve_item_cube_y_block(&mut cube, &mut velocity);
        }

        let pos_after_y = Vec2::new(original_pos.x, resolved_y);
        let (resolved_x, blocked_x) = collision_world.resolve_axis(
            Axis::X,
            pos_after_y,
            velocity.0.x,
            half_extents,
        );
        if blocked_x {
            velocity.0.x = 0.0;
        }

        transform.translation = Vec3::new(resolved_x, pos_after_y.y, z);
    }
}

fn move_grounded_item_cube_by_attraction(
    cube: &mut ItemCube,
    velocity: &mut LinearVelocity,
    transform: &mut Transform,
) -> bool {
    if cube.attraction_vel.y > 0.5 {
        cube.grounded = false;
        return false;
    }

    move_item_cube_by_attraction(cube, velocity, transform);
    true
}

fn move_item_cube_by_attraction(
    cube: &ItemCube,
    velocity: &mut LinearVelocity,
    transform: &mut Transform,
) {
    velocity.0 = Vec2::ZERO;
    let z = transform.translation.z;
    transform.translation =
        (transform.translation.xy() + cube.attraction_vel).extend(z);
}

fn resolve_item_cube_y_block(
    cube: &mut ItemCube,
    velocity: &mut LinearVelocity,
) {
    if velocity.0.y >= 0.0 {
        velocity.0.y = velocity.0.y.min(0.0);
        return;
    }

    let impact_speed = (-velocity.0.y).max(0.0);
    let can_bounce =
        impact_speed > BOUNCE_THRESHOLD && cube.bounce_count < MAX_BOUNCES;
    if can_bounce {
        velocity.0.y = impact_speed * BOUNCE_COEFFICIENT;
        velocity.0.x *= 0.8;
        cube.bounce_count += 1;
        cube.grounded = false;
        return;
    }

    cube.grounded = true;
    velocity.0.y = 0.0;
}

fn update_dropped_item_position(
    mut item_query: Query<
        (&mut DroppedItem, &mut Transform),
        Without<ItemCube>,
    >,
    cube_query: Query<&Transform, With<ItemCube>>,
) {
    for (mut item, mut transform) in item_query.iter_mut() {
        if let Ok(cube_transform) = cube_query.get(item.cube_entity) {
            let cube_pos = cube_transform.translation;

            item.oscillation_phase =
                wrapped_item_oscillation_phase(item.oscillation_phase);

            let oscillation_y =
                item.oscillation_phase.sin() * OSCILLATION_AMPLITUDE;

            transform.translation.x = cube_pos.x;
            transform.translation.y =
                cube_pos.y + oscillation_y + VISUAL_OFFSET_Y;
        }
    }
}

fn wrapped_item_oscillation_phase(phase: f32) -> f32 {
    let phase = phase + OSCILLATION_SPEED;
    if phase <= std::f32::consts::TAU {
        return phase;
    }

    phase - std::f32::consts::TAU
}

/// Applies attraction towards the backpack within range and stores collected
/// drops in backpack inventory.
/// Attraction is additive to pre-existing velocity (doesn't replace physics velocity).
fn attract_and_collect_items(
    mut commands: Commands,
    mut player_q: Query<
        (&GlobalTransform, &mut MaterialInventory),
        With<Player>,
    >,
    backpack_anchor: Res<BackpackAnchor>,
    mut cube_query: Query<(
        Entity,
        &GlobalTransform,
        &mut ItemCube,
        &mut LinearVelocity,
    )>,
    item_query: Query<(Entity, &DroppedItem)>,
    mut pickups: MessageWriter<ItemPickedUp>,
) {
    let Ok((player_transform, mut material_inventory)) = player_q.single_mut()
    else {
        return;
    };
    let player_pos = player_transform.translation().truncate();
    let target_pos = if backpack_anchor.valid {
        backpack_anchor.pos
    } else {
        player_pos
    };

    for (cube_entity, cube_transform, mut cube, mut velocity) in
        cube_query.iter_mut()
    {
        if cube.pickup_delay_ticks > 0 {
            cube.pickup_delay_ticks -= 1;
            continue;
        }

        let cube_pos = cube_transform.translation().truncate();
        let delta_to_player = player_pos - cube_pos;
        let dist_to_player = delta_to_player.length();
        let delta_to_target = target_pos - cube_pos;
        let dist_to_target = delta_to_target.length();

        if dist_to_target < BACKPACK_PICKUP_DISTANCE {
            collect_item_cube(
                &mut commands,
                &mut material_inventory,
                &mut pickups,
                &item_query,
                cube_entity,
                cube.gfx_entity,
            );
            continue;
        }

        let should_attract = cube.locked_attraction
            || (cube.grounded && dist_to_player <= ATTRACTION_RANGE);

        if !should_attract {
            continue;
        }

        if !cube.locked_attraction {
            cube.locked_attraction = true;
        }

        if !cube.squish_applied {
            cube.squish_applied = true;

            apply_item_cube_squish(
                &mut commands,
                &item_query,
                cube_entity,
                cube.gfx_entity,
            );
        }

        cube.attraction_ticks += 1;
        let time_factor = (cube.attraction_ticks as f32
            / ATTRACTION_RAMP_TICKS as f32)
            .min(1.0);
        let target_speed = ATTRACTION_MAX_SPEED * time_factor;

        let dir = delta_to_target.normalize_or_zero();
        let target_attraction = dir * target_speed;

        cube.attraction_vel = cube
            .attraction_vel
            .lerp(target_attraction, ATTRACTION_ACCEL);

        velocity.0 += cube.attraction_vel;
    }
}

fn collect_item_cube(
    commands: &mut Commands,
    material_inventory: &mut MaterialInventory,
    pickups: &mut MessageWriter<ItemPickedUp>,
    item_query: &Query<(Entity, &DroppedItem)>,
    cube_entity: Entity,
    cube_gfx_entity: Entity,
) {
    commands.entity(cube_gfx_entity).despawn();
    commands.entity(cube_entity).despawn();
    let Some((item_entity, dropped_item)) =
        item_for_cube(cube_entity, item_query)
    else {
        return;
    };

    material_inventory.add_to_backpack(dropped_item.material, 1);
    pickups.write(ItemPickedUp);
    commands.entity(item_entity).despawn();
}

fn item_for_cube<'a>(
    cube_entity: Entity,
    item_query: &'a Query<(Entity, &DroppedItem)>,
) -> Option<(Entity, &'a DroppedItem)> {
    item_query
        .iter()
        .find(|(_item_entity, dropped_item)| {
            dropped_item.cube_entity == cube_entity
        })
}

fn apply_item_cube_squish(
    commands: &mut Commands,
    item_query: &Query<(Entity, &DroppedItem)>,
    cube_entity: Entity,
    cube_gfx_entity: Entity,
) {
    commands
        .entity(cube_gfx_entity)
        .insert(item_pickup_stretch_bundle());
    let Some((item_entity, _dropped_item)) =
        item_for_cube(cube_entity, item_query)
    else {
        return;
    };

    commands
        .entity(item_entity)
        .insert(item_pickup_stretch_bundle());
}

fn item_pickup_stretch_bundle() -> (SpriteScale, SpriteStretch) {
    (
        SpriteScale::new(),
        SpriteStretch {
            current_tick: 0,
            squeeze_ticks: 2,
            hold_ticks: 0,
            release_ticks: 8,
            max_squeeze_x: 0.7,
            max_stretch_y: 1.3,
            anchor: StretchAnchor::Center,
            sprite_height: 8.0,
        },
    )
}

#[cfg(test)]
mod tests {
    use bevy::ecs::system::RunSystemOnce;

    use super::*;

    #[test]
    fn a_collected_drop_fills_the_backpack_and_reports_the_pickup() {
        let mut world = World::new();
        world.init_resource::<BackpackAnchor>();
        world.init_resource::<Messages<ItemPickedUp>>();
        let player = world
            .spawn((Player, GlobalTransform::default(), MaterialInventory::empty()))
            .id();
        let gfx_entity = world.spawn_empty().id();
        let cube = ItemCube {
            grounded: true,
            pickup_delay_ticks: 0,
            attraction_vel: Vec2::ZERO,
            squish_applied: false,
            locked_attraction: false,
            attraction_ticks: 0,
            bounce_count: 0,
            gfx_entity,
        };
        let cube_entity = world
            .spawn((cube, GlobalTransform::default(), LinearVelocity(Vec2::ZERO)))
            .id();
        world.spawn(DroppedItem {
            material: CraftMaterial::BatWing,
            cube_entity,
            oscillation_phase: 0.0,
        });

        world.run_system_once(attract_and_collect_items).unwrap();

        let materials = world.get::<MaterialInventory>(player).unwrap();
        assert_eq!(materials.backpack_count(CraftMaterial::BatWing), 1);
        assert_eq!(world.resource::<Messages<ItemPickedUp>>().len(), 1);
        assert!(world.get_entity(cube_entity).is_err());
    }
}
