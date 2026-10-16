//! Cosmetic equipment that follows the player graphics entity.
//!
//! Behaviour:
//! - Spawns one sprite per equipment type and follows `PlayerGfx` with smoothing
//! - Selects a backpack sprite tier from the player's backpack material count
//! - Small hover animation at idle; triangle-wave shake when running
//! - Visibility rules hide weapons during certain actions to prevent visual overlap
use crate::game::player::foreground::{ActionRef, ActionTag};
use bevy::camera::visibility::RenderLayers;
use std::collections::HashMap;
use theseeker_engine::time::{
    GameTickUpdate, GameTickPost,
};

use super::skills::portable_tent::TentInteriorOccupant;
use super::states::{
    BurningDashing, Elevator, Falling, HorizontalInputIntent, Spitting,
};
use super::weapon::{PlayerMeleeWeapon, PlayerRangedWeapon, WeaponInventory, WeaponType};
use super::{
    Attacking, MaterialInventory, Player, PlayerGfx, Running, Whirling,
};
use crate::game::gentstate::Facing;
use crate::prelude::*;

pub struct EquipmentPlugin;

impl Plugin for EquipmentPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<BackpackAnchor>();
        app.add_systems(Startup, load_equipment_assets);
        app.add_systems(
            GameTickUpdate,
            (
                spawn_equipment_on_player_spawn
                    .run_if(any_with_component::<Player>),
                update_equipment_visibility,
                update_backpack_sprite,
                update_equipment_position,
                despawn_equipment_on_player_despawn,
            )
                .chain(),
        );
        app.add_systems(
            GameTickPost,
            apply_equipment_pose.in_set(theseeker_engine::presentation::PresentationSet::Prepare),
        );
    }
}

#[derive(Component)]
pub struct Equipment {
    /// Which player graphics entity this equipment belongs to
    pub player_gfx: Entity,
    /// The type of equipment
    pub equipment_type: EquipmentType,
    /// Cached copy of whether the owner was inside a tent on the previous tick
    pub owner_inside_tent: bool,
    /// Base Y offset for hovering animation
    pub base_offset: Vec2,
    /// Current offset (includes hovering)
    pub current_offset: Vec2,
    /// Phase for hovering animation
    pub hover_phase: f32,
    /// Phase for running shake animation (in ticks)
    pub run_shake_phase: u32,
}

#[derive(Resource, Debug, Clone, Copy)]
pub(crate) struct BackpackAnchor {
    pub pos: Vec2,
    pub side: f32,
    pub valid: bool,
}

impl Default for BackpackAnchor {
    fn default() -> Self {
        Self {
            pos: Vec2::ZERO,
            side: 1.0,
            valid: false,
        }
    }
}

#[derive(Component, Debug, Clone)]
struct EquipmentTickTransform(pub Transform);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EquipmentType {
    Backpack,
    Sword,
    Hammer,
    Bow,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum BackpackTier {
    Small,
    Medium,
    Large,
}

impl BackpackTier {
    fn from_backpack_count(count: u32) -> Self {
        if count < 12 {
            Self::Small
        } else if count < 24 {
            Self::Medium
        } else {
            Self::Large
        }
    }

    fn asset_path(self) -> &'static str {
        match self {
            Self::Small => "items/equipment/backpack1.png",
            Self::Medium => "items/equipment/backpack2.png",
            Self::Large => "items/equipment/backpack3.png",
        }
    }
}

impl EquipmentType {
    /// Z-order for this equipment type
    fn z_order(&self) -> f32 {
        match self {
            EquipmentType::Backpack => 10.0 * 0.000_001, // Furthest back
            EquipmentType::Sword => 11.0 * 0.000_001,
            EquipmentType::Hammer => 12.0 * 0.000_001,
            EquipmentType::Bow => 13.0 * 0.000_001, // Closest to camera (but still behind player at 15.0)
        }
    }

    /// Base offset when idle
    fn base_offset_idle(&self) -> Vec2 {
        Vec2::new(0.0, 0.0)
    }

    /// Base offset when moving left
    fn base_offset_left(&self) -> Vec2 {
        Vec2::new(-3.0, 0.0)
    }

    /// Base offset when moving right
    fn base_offset_right(&self) -> Vec2 {
        Vec2::new(3.0, 0.0)
    }

    /// Hover amplitude for this equipment type
    fn hover_amplitude(&self) -> f32 {
        1.0
    }

    /// Hover frequency multiplier for this equipment type
    fn hover_frequency(&self) -> f32 {
        1.0
    }

    /// Position lerp factor for this equipment type
    /// Higher values => tighter following, lower values => more lag
    fn lerp_factor(&self) -> f32 {
        match self {
            EquipmentType::Backpack => 0.20, // Slightly more lag for weight feel
            EquipmentType::Sword => 0.28,    // Default value
            EquipmentType::Hammer => 0.22,   // Heavier, more lag
            EquipmentType::Bow => 0.32,      // Lighter, follows more closely
        }
    }
}

#[derive(Resource)]
pub struct EquipmentAssetHandles {
    handles: HashMap<EquipmentType, Handle<Image>>,
    backpack_tiers: HashMap<BackpackTier, Handle<Image>>,
}

fn load_equipment_assets(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
) {
    let mut handles = HashMap::new();
    let mut backpack_tiers = HashMap::new();

    for tier in [
        BackpackTier::Small,
        BackpackTier::Medium,
        BackpackTier::Large,
    ] {
        backpack_tiers.insert(tier, asset_server.load(tier.asset_path()));
    }

    handles.insert(
        EquipmentType::Backpack,
        backpack_tiers[&BackpackTier::Small].clone(),
    );
    handles.insert(
        EquipmentType::Sword,
        asset_server.load("items/equipment/sword.png"),
    );
    handles.insert(
        EquipmentType::Hammer,
        asset_server.load("items/equipment/hammer.png"),
    );
    handles.insert(
        EquipmentType::Bow,
        asset_server.load("items/equipment/bow.png"),
    );

    commands.insert_resource(EquipmentAssetHandles {
        handles,
        backpack_tiers,
    });
}

fn spawn_equipment_on_player_spawn(
    player_gfx_query: Query<Entity, Added<PlayerGfx>>,
    equipment_query: Query<&Equipment>,
    equipment_handles: Option<Res<EquipmentAssetHandles>>,
    mut commands: Commands,
) {
    let Some(handles) = equipment_handles else {
        warn!("Equipment handles not loaded yet");
        return;
    };

    for player_gfx_entity in player_gfx_query.iter() {
        // Check if equipment already exists for this player
        let already_has_equipment = equipment_query
            .iter()
            .any(|eq| eq.player_gfx == player_gfx_entity);

        if already_has_equipment {
            continue;
        }

        // Spawn all equipment types
        for equipment_type in [
            EquipmentType::Backpack,
            EquipmentType::Sword,
            EquipmentType::Hammer,
            EquipmentType::Bow,
        ] {
            let handle = handles.handles.get(&equipment_type).unwrap();
            let base_offset = equipment_type.base_offset_idle();
            let base_transform = Transform::from_translation(Vec3::new(
                0.0,
                0.0,
                equipment_type.z_order(),
            ));

            commands.spawn((
                Name::new(format!(
                    "Equipment_{equipment_type:?}"
                )),
                Equipment {
                    player_gfx: player_gfx_entity,
                    equipment_type,
                    owner_inside_tent: false,
                    base_offset,
                    current_offset: base_offset,
                    hover_phase: match equipment_type {
                        EquipmentType::Backpack => 0.0,
                        EquipmentType::Sword => std::f32::consts::PI * 0.5,
                        EquipmentType::Hammer => std::f32::consts::PI,
                        EquipmentType::Bow => std::f32::consts::PI * 1.5,
                    },
                    run_shake_phase: 0,
                },
                Sprite {
                    image: handle.clone(),
                    ..default()
                },
                theseeker_engine::presentation::PresentationSource,
                theseeker_engine::presentation::PresentationAnchor(player_gfx_entity),
                EquipmentTickTransform(base_transform),
                Transform::from_translation(Vec3::new(
                    0.0,
                    0.0,
                    equipment_type.z_order(),
                )),
                GlobalTransform::default(),
                Visibility::default(),
                InheritedVisibility::default(),
                ViewVisibility::default(),
                RenderLayers::layer(2),
                StateDespawnMarker,
            ));
        }
    }
}

fn update_equipment_visibility(
    mut equipment_query: Query<
        (&Equipment, &mut Visibility),
        Without<PlayerGfx>,
    >,
    player_state_query: Query<
        (
            Option<ActionRef<Attacking>>,
            Has<ActionTag<Whirling>>,
            Has<ActionTag<BurningDashing>>,
            Has<ActionTag<Spitting>>,
            Has<ActionTag<Elevator>>,
        ),
        With<Player>,
    >,
    player_gfx_visibility: Query<&Visibility, With<PlayerGfx>>,
    melee_weapon: Res<PlayerMeleeWeapon>,
    weapon_inventory_q: Query<&WeaponInventory, With<Player>>,
) {
    // Read attack-related visibility state once
    let (
        attacking,
        is_whirling,
        is_burning_dashing,
        is_spitting,
        is_elevator,
    ) =
        player_state_query
            .single()
            .unwrap_or((None, false, false, false, false));
    let attack_weapon = attacking.map(|state| state.weapon);
    let is_melee_attacking = attack_weapon.is_some_and(WeaponType::is_melee);
    let is_bow_attacking = attack_weapon == Some(WeaponType::Bow);
    let equipped_ranged = weapon_inventory_q
        .single()
        .ok()
        .and_then(super::weapon::WeaponInventory::equipped_ranged);

    for (equipment, mut visibility) in equipment_query.iter_mut() {
        let player_visible = player_gfx_visibility
            .get(equipment.player_gfx)
            .map(|vis| !matches!(*vis, Visibility::Hidden))
            .unwrap_or(false);

        if !player_visible {
            *visibility = Visibility::Hidden;
            continue;
        }

        // Hide all equipment during states with bespoke player silhouettes.
        if is_burning_dashing || is_spitting || is_elevator {
            *visibility = Visibility::Hidden;
            continue;
        }

        *visibility = equipment_visibility(
            equipment.equipment_type,
            equipped_ranged,
            *melee_weapon,
            is_bow_attacking,
            is_melee_attacking,
            is_whirling,
        );
    }
}

fn equipment_visibility(
    equipment_type: EquipmentType,
    equipped_ranged: Option<PlayerRangedWeapon>,
    melee_weapon: PlayerMeleeWeapon,
    is_bow_attacking: bool,
    is_melee_attacking: bool,
    is_whirling: bool,
) -> Visibility {
    match equipment_type {
        EquipmentType::Bow
            if equipped_ranged != Some(PlayerRangedWeapon::Bow)
                || is_bow_attacking =>
        {
            Visibility::Hidden
        },
        EquipmentType::Hammer
            if melee_weapon != PlayerMeleeWeapon::Hammer
                || is_melee_attacking
                || is_whirling =>
        {
            Visibility::Hidden
        },
        EquipmentType::Sword
            if melee_weapon != PlayerMeleeWeapon::Sword
                || is_melee_attacking
                || is_whirling =>
        {
            Visibility::Hidden
        },
        EquipmentType::Backpack
        | EquipmentType::Bow
        | EquipmentType::Hammer
        | EquipmentType::Sword => Visibility::Visible,
    }
}

fn update_backpack_sprite(
    equipment_handles: Option<Res<EquipmentAssetHandles>>,
    mut equipment_query: Query<(&Equipment, &mut Sprite), Without<PlayerGfx>>,
    player_gfx_query: Query<&PlayerGfx, Without<Equipment>>,
    player_materials_query: Query<&MaterialInventory, With<Player>>,
) {
    let Some(handles) = equipment_handles else {
        return;
    };

    for (equipment, mut sprite) in equipment_query.iter_mut() {
        if equipment.equipment_type != EquipmentType::Backpack {
            continue;
        }

        let Ok(player_gfx) = player_gfx_query.get(equipment.player_gfx) else {
            continue;
        };
        let Ok(materials) = player_materials_query.get(player_gfx.e_gent)
        else {
            continue;
        };

        let tier =
            BackpackTier::from_backpack_count(materials.total_backpack());
        let Some(handle) = handles.backpack_tiers.get(&tier) else {
            continue;
        };

        sprite.image = handle.clone();
    }
}

fn update_equipment_position(
    mut equipment_query: Query<(
        &mut Equipment,
        &mut EquipmentTickTransform,
        &mut Sprite,
    )>,
    player_gfx_query: Query<&PlayerGfx, Without<Equipment>>,
    player_gent_query: Query<&Transform, With<Player>>,
    player_query: Query<
        (
            &Facing,
            Has<Running>,
            &HorizontalInputIntent,
            Option<&Falling>,
            Has<TentInteriorOccupant>,
        ),
        With<Player>,
    >,
    mut backpack_anchor: ResMut<BackpackAnchor>,
) {
    // Hovering speed - radians per tick (at 96Hz, this gives ~2.0 radians/second)
    const HOVER_SPEED_PER_TICK: f32 = 0.02083;
    // Running shake constants
    const RUN_SHAKE_AMPLITUDE: f32 = 2.0; // pixels
    const RUN_SHAKE_CYCLE_TICKS: u32 = 32; // 16 ticks up, 16 ticks down

    backpack_anchor.valid = false;

    for (
        mut equipment,
        mut tick_transform,
        mut sprite,
    ) in
        equipment_query.iter_mut()
    {
        let Ok(player_gfx) = player_gfx_query.get(equipment.player_gfx) else {
            continue;
        };
        let Ok(player_transform) = player_gent_query.get(player_gfx.e_gent)
        else {
            continue;
        };

        // Update hover phase based on ticks (always update to maintain continuity)
        equipment.hover_phase +=
            HOVER_SPEED_PER_TICK * equipment.equipment_type.hover_frequency();

        let animation_offset = equipment_animation_offset(
            &equipment,
            RUN_SHAKE_CYCLE_TICKS,
            RUN_SHAKE_AMPLITUDE,
        );

        // Update current offset with only one animation type
        equipment.current_offset = equipment.base_offset + animation_offset;

        // Calculate target position
        let target_pos =
            player_transform.translation.truncate() + equipment.current_offset;

        // Lerp to target position
        let z = tick_transform.0.translation.z;
        let target_translation = target_pos.extend(z);

        // Update sprite flip and base offset.
        // Read state from the physical player entity
        if let Ok((
            facing,
            is_running,
            horizontal_intent,
            falling,
            is_inside_tent,
        )) =
            player_query.get(player_gfx.e_gent)
        {
            let player_state = EquipmentPlayerFollowState {
                facing,
                is_running,
                horizontal_intent,
                falling,
                is_inside_tent,
            };
            update_equipment_from_player_state(
                &mut equipment,
                &mut tick_transform,
                &mut sprite,
                player_state,
                target_pos,
                target_translation,
                z,
                RUN_SHAKE_CYCLE_TICKS,
                &mut backpack_anchor,
            );
        } else {
            lerp_equipment_to_target(&equipment, &mut tick_transform, target_pos, z);
        }

        if equipment.equipment_type == EquipmentType::Backpack {
            backpack_anchor.pos =
                tick_transform.0.translation.truncate();
            backpack_anchor.valid = true;
        }
    }
}

struct EquipmentPlayerFollowState<'a> {
    facing: &'a Facing,
    is_running: bool,
    horizontal_intent: &'a HorizontalInputIntent,
    falling: Option<&'a Falling>,
    is_inside_tent: bool,
}

fn equipment_animation_offset(
    equipment: &Equipment,
    run_shake_cycle_ticks: u32,
    run_shake_amplitude: f32,
) -> Vec2 {
    if equipment.run_shake_phase > 0 {
        return Vec2::new(
            0.0,
            running_shake_offset(
                equipment.run_shake_phase,
                run_shake_cycle_ticks,
                run_shake_amplitude,
            ),
        );
    }

    Vec2::new(
        0.0,
        equipment.hover_phase.sin()
            * equipment.equipment_type.hover_amplitude(),
    )
}

fn running_shake_offset(
    run_shake_phase: u32,
    run_shake_cycle_ticks: u32,
    run_shake_amplitude: f32,
) -> f32 {
    let half_cycle = run_shake_cycle_ticks / 2;
    if run_shake_phase <= half_cycle {
        return (run_shake_phase as f32
            / (run_shake_cycle_ticks as f32 / 2.0))
            * run_shake_amplitude;
    }

    run_shake_amplitude
        - ((run_shake_phase - half_cycle) as f32
            / (run_shake_cycle_ticks as f32 / 2.0))
            * run_shake_amplitude
}

#[expect(
    clippy::too_many_arguments,
    reason = "The follow update mutates the equipment state, two tick transforms, sprite orientation, and shared backpack anchor together."
)]
fn update_equipment_from_player_state(
    equipment: &mut Equipment,
    tick_transform: &mut EquipmentTickTransform,
    sprite: &mut Sprite,
    player_state: EquipmentPlayerFollowState<'_>,
    target_pos: Vec2,
    target_translation: Vec3,
    z: f32,
    run_shake_cycle_ticks: u32,
    backpack_anchor: &mut BackpackAnchor,
) {
    sync_equipment_owner_space(
        equipment,
        tick_transform,
        target_pos,
        target_translation,
        z,
        player_state.is_inside_tent,
    );

    let wall_side = player_state.falling.and_then(|f| f.wall_slide);
    sprite.flip_x = equipment_flip_x(player_state.facing, wall_side);
    update_equipment_run_shake_phase(
        equipment,
        player_state.is_running,
        run_shake_cycle_ticks,
    );
    update_equipment_base_offset(equipment, player_state.horizontal_intent, wall_side);
    update_backpack_anchor_side(equipment, player_state.facing, backpack_anchor);
}

fn sync_equipment_owner_space(
    equipment: &mut Equipment,
    tick_transform: &mut EquipmentTickTransform,
    target_pos: Vec2,
    target_translation: Vec3,
    z: f32,
    is_inside_tent: bool,
) {
    if equipment.owner_inside_tent != is_inside_tent {
        equipment.owner_inside_tent = is_inside_tent;
        tick_transform.0.translation = target_translation;
        return;
    }

    lerp_equipment_to_target(equipment, tick_transform, target_pos, z);
}

fn lerp_equipment_to_target(
    equipment: &Equipment,
    tick_transform: &mut EquipmentTickTransform,
    target_pos: Vec2,
    z: f32,
) {
    let current_pos = tick_transform.0.translation.truncate();
    let new_pos =
        current_pos.lerp(target_pos, equipment.equipment_type.lerp_factor());
    tick_transform.0.translation = new_pos.extend(z);
}

fn equipment_flip_x(
    facing: &Facing,
    wall_side: Option<crate::game::player::states::WallSide>,
) -> bool {
    match wall_side {
        Some(crate::game::player::states::WallSide::Left) => true,
        Some(crate::game::player::states::WallSide::Right) => false,
        None => facing.direction() < 0.0,
    }
}

fn update_equipment_run_shake_phase(
    equipment: &mut Equipment,
    is_running: bool,
    run_shake_cycle_ticks: u32,
) {
    if is_running {
        equipment.run_shake_phase =
            (equipment.run_shake_phase + 1) % run_shake_cycle_ticks;
    } else {
        equipment.run_shake_phase = 0;
    }
}

fn update_equipment_base_offset(
    equipment: &mut Equipment,
    horizontal_intent: &HorizontalInputIntent,
    wall_side: Option<crate::game::player::states::WallSide>,
) {
    let new_offset = equipment_base_offset(
        equipment.equipment_type,
        horizontal_intent.movement,
        wall_side,
    );
    if equipment.base_offset != new_offset {
        equipment.base_offset = new_offset;
    }
}

fn equipment_base_offset(
    equipment_type: EquipmentType,
    movement_dir: f32,
    wall_side: Option<crate::game::player::states::WallSide>,
) -> Vec2 {
    match wall_side {
        Some(crate::game::player::states::WallSide::Left) => {
            equipment_type.base_offset_left()
        },
        Some(crate::game::player::states::WallSide::Right) => {
            equipment_type.base_offset_right()
        },
        None if movement_dir < 0.0 => equipment_type.base_offset_left(),
        None if movement_dir > 0.0 => equipment_type.base_offset_right(),
        None => equipment_type.base_offset_idle(),
    }
}

fn update_backpack_anchor_side(
    equipment: &Equipment,
    facing: &Facing,
    backpack_anchor: &mut BackpackAnchor,
) {
    if equipment.equipment_type != EquipmentType::Backpack {
        return;
    }

    let side = backpack_anchor_side(equipment, facing, backpack_anchor.side);
    backpack_anchor.side = side;
}

fn backpack_anchor_side(
    equipment: &Equipment,
    facing: &Facing,
    previous_side: f32,
) -> f32 {
    let side = if equipment.base_offset.x.abs() > 0.01 {
        equipment.base_offset.x.signum()
    } else {
        facing.direction().signum()
    };
    if side.abs() < 0.01 {
        previous_side
    } else {
        side
    }
}

fn apply_equipment_pose(mut query: Query<(&mut Transform, &EquipmentTickTransform)>) {
    for (mut transform, current) in &mut query { *transform = current.0; }
}

fn despawn_equipment_on_player_despawn(
    mut commands: Commands,
    mut removed_players: RemovedComponents<PlayerGfx>,
    mut equipment_query: Query<(Entity, &Equipment, &mut Visibility)>,
) {
    for removed_player in removed_players.read() {
        despawn_equipment_for_player(
            removed_player,
            &mut commands,
            &mut equipment_query,
        );
    }
}

fn despawn_equipment_for_player(
    removed_player: Entity,
    commands: &mut Commands,
    equipment_query: &mut Query<(Entity, &Equipment, &mut Visibility)>,
) {
    for (entity, _, mut visibility) in equipment_query
        .iter_mut()
        .filter(|(_, equipment, _)| equipment.player_gfx == removed_player)
    {
        *visibility = Visibility::Hidden;
        commands.entity(entity).despawn();
    }
}
