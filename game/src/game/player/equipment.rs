use bevy::prelude::*;
use std::collections::HashMap;
use theseeker_engine::time::{GameTickUpdate, GameTime};

use crate::prelude::*;
use super::{Player, PlayerGfx, Running, Attacking, Whirling};
use super::player_weapon::{PlayerCombatStyle, PlayerMeleeWeapon};
use super::player_action::PlayerAction;
use crate::game::gentstate::Facing;
use leafwing_input_manager::prelude::*;

pub struct EquipmentPlugin;

impl Plugin for EquipmentPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, load_equipment_assets);
        app.add_systems(
            GameTickUpdate,
            (
                spawn_equipment_on_player_spawn
                    .run_if(any_with_component::<Player>),
                update_equipment_visibility,
                update_equipment_position,
                despawn_equipment_on_player_despawn,
            )
                .chain(),
        );
    }
}

#[derive(Component)]
pub struct Equipment {
    /// Which player graphics entity this equipment belongs to
    pub player_gfx: Entity,
    /// The type of equipment
    pub equipment_type: EquipmentType,
    /// Base Y offset for hovering animation
    pub base_offset: Vec2,
    /// Current offset (includes hovering)
    pub current_offset: Vec2,
    /// Phase for hovering animation
    pub hover_phase: f32,
    /// Phase for running shake animation (in ticks)
    pub run_shake_phase: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EquipmentType {
    Backpack,
    Sword,
    Hammer,
    Bow,
}

impl EquipmentType {
    /// Returns the z-order for this equipment type
    fn z_order(&self) -> f32 {
        match self {
            EquipmentType::Backpack => 10.0 * 0.000001, // Furthest back
            EquipmentType::Sword => 11.0 * 0.000001,
            EquipmentType::Hammer => 12.0 * 0.000001,
            EquipmentType::Bow => 13.0 * 0.000001,     // Closest to camera (but still behind player at 15.0)
        }
    }

    /// Returns the base offset for this equipment type when idle
    fn base_offset_idle(&self) -> Vec2 {
        match self {
            EquipmentType::Backpack => Vec2::new(0.0, 0.0),
            EquipmentType::Sword => Vec2::new(0.0, 0.0),
            EquipmentType::Hammer => Vec2::new(0.0, 0.0),
            EquipmentType::Bow => Vec2::new(0.0, 0.0),
        }
    }
    
    /// Returns the base offset for this equipment type when moving left
    fn base_offset_left(&self) -> Vec2 {
        match self {
            EquipmentType::Backpack => Vec2::new(-3.0, 0.0),
            EquipmentType::Sword => Vec2::new(-3.0, 0.0),
            EquipmentType::Hammer => Vec2::new(-3.0, 0.0),
            EquipmentType::Bow => Vec2::new(-3.0, 0.0),
        }
    }
    
    /// Returns the base offset for this equipment type when moving right
    fn base_offset_right(&self) -> Vec2 {
        match self {
            EquipmentType::Backpack => Vec2::new(3.0, 0.0),
            EquipmentType::Sword => Vec2::new(3.0, 0.0),
            EquipmentType::Hammer => Vec2::new(3.0, 0.0),
            EquipmentType::Bow => Vec2::new(3.0, 0.0),
        }
    }

    /// Returns the hover amplitude for this equipment type
    fn hover_amplitude(&self) -> f32 {
        match self {
            EquipmentType::Backpack => 1.0,
            EquipmentType::Sword => 1.0,
            EquipmentType::Hammer => 1.0,
            EquipmentType::Bow => 1.0,
        }
    }

    /// Returns the hover frequency multiplier for this equipment type
    fn hover_frequency(&self) -> f32 {
        match self {
            EquipmentType::Backpack => 1.0,
            EquipmentType::Sword => 1.0,
            EquipmentType::Hammer => 1.0,
            EquipmentType::Bow => 1.0,
        }
    }
    
    /// Returns the position lerp factor for this equipment type
    /// Higher values = tighter following, lower values = more lag
    fn lerp_factor(&self) -> f32 {
        match self {
            EquipmentType::Backpack => 0.20,  // Slightly more lag for weight feel
            EquipmentType::Sword => 0.28,     // Default value
            EquipmentType::Hammer => 0.22,    // Heavier, more lag
            EquipmentType::Bow => 0.32,       // Lighter, follows more closely
        }
    }
}

#[derive(Resource)]
pub struct EquipmentAssetHandles {
    handles: HashMap<EquipmentType, Handle<Image>>,
}

fn load_equipment_assets(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
) {
    let mut handles = HashMap::new();
    
    handles.insert(
        EquipmentType::Backpack,
        asset_server.load("items/equipment/backpack.png"),
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

    commands.insert_resource(EquipmentAssetHandles { handles });
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
            
            commands.spawn((
                Name::new(format!("Equipment_{:?}", equipment_type)),
                Equipment {
                    player_gfx: player_gfx_entity,
                    equipment_type,
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
                Transform::from_translation(Vec3::new(0.0, 0.0, equipment_type.z_order())),
                GlobalTransform::default(),
                Visibility::default(),
                InheritedVisibility::default(),
                ViewVisibility::default(),
                StateDespawnMarker,
            ));
        }
    }
}

fn update_equipment_visibility(
    mut equipment_query: Query<(&Equipment, &mut Visibility)>,
    player_query: Query<(Has<Attacking>, Has<Whirling>), With<Player>>,
    combat_style: Res<PlayerCombatStyle>,
    melee_weapon: Res<PlayerMeleeWeapon>,
) {

    
    // Get player attack states
    let (is_attacking, is_whirling) = player_query
        .get_single()
        .unwrap_or((false, false));
    
    for (equipment, mut visibility) in equipment_query.iter_mut() {
        *visibility = match equipment.equipment_type {
            EquipmentType::Backpack => Visibility::Visible,
            EquipmentType::Bow => {
                // Hide bow when ranged attacking (basic attack)
                if *combat_style == PlayerCombatStyle::Ranged && is_attacking {
                    Visibility::Hidden
                } else {
                    Visibility::Visible
                }
            },
            EquipmentType::Hammer => {
                // Hide hammer when: melee attacking OR whirling (any combat style)
                if *melee_weapon == PlayerMeleeWeapon::Hammer {
                    if (*combat_style == PlayerCombatStyle::Melee && is_attacking) || is_whirling {
                        Visibility::Hidden
                    } else {
                        Visibility::Visible
                    }
                } else {
                    Visibility::Hidden
                }
            },
            EquipmentType::Sword => {
                // Hide sword when: melee attacking OR whirling (any combat style)
                if *melee_weapon == PlayerMeleeWeapon::Sword {
                    if (*combat_style == PlayerCombatStyle::Melee && is_attacking) || is_whirling {
                        Visibility::Hidden
                    } else {
                        Visibility::Visible
                    }
                } else {
                    Visibility::Hidden
                }
            },
        };
    }
}

fn update_equipment_position(
    mut equipment_query: Query<(&mut Equipment, &mut Transform, &mut Sprite)>,
    player_gfx_query: Query<(&GlobalTransform, &PlayerGfx), Without<Equipment>>,
    player_query: Query<(&Facing, Has<Running>, &ActionState<PlayerAction>), With<Player>>,
    gametime: Res<GameTime>,
) {
    // Hovering speed - radians per tick (at 96Hz, this gives ~2.0 radians/second)
    const HOVER_SPEED_PER_TICK: f32 = 0.02083;
    // Running shake constants
    const RUN_SHAKE_AMPLITUDE: f32 = 1.0;  // pixels
    const RUN_SHAKE_CYCLE_TICKS: u32 = 64; // 16 ticks up, 16 ticks down

    for (mut equipment, mut transform, mut sprite) in equipment_query.iter_mut() {
        let Ok((player_transform, player_gfx)) = player_gfx_query.get(equipment.player_gfx) else {
            continue;
        };

        // Update hover phase based on ticks (always update to maintain continuity)
        equipment.hover_phase += HOVER_SPEED_PER_TICK * equipment.equipment_type.hover_frequency();
        
        // Calculate the offset - either running shake OR regular hover, not both
        let animation_offset = if equipment.run_shake_phase > 0 {
            // Running shake offset
            // Create a triangle wave: 0 to 32 ticks = up, 32 to 64 ticks = down
            let shake_y = if equipment.run_shake_phase <= RUN_SHAKE_CYCLE_TICKS / 2 {
                // Up phase (0 to 32 ticks)
                (equipment.run_shake_phase as f32 / (RUN_SHAKE_CYCLE_TICKS as f32 / 2.0)) * RUN_SHAKE_AMPLITUDE
            } else {
                // Down phase (32 to 64 ticks)
                RUN_SHAKE_AMPLITUDE - ((equipment.run_shake_phase - RUN_SHAKE_CYCLE_TICKS / 2) as f32 / (RUN_SHAKE_CYCLE_TICKS as f32 / 2.0)) * RUN_SHAKE_AMPLITUDE
            };
            Vec2::new(0.0, shake_y)
        } else {
            // Regular hover offset
            Vec2::new(
                0.0,
                equipment.hover_phase.sin() * equipment.equipment_type.hover_amplitude(),
            )
        };
        
        // Update current offset with only one animation type
        equipment.current_offset = equipment.base_offset + animation_offset;
        
        // Calculate target position
        let target_pos = player_transform.translation().truncate() + equipment.current_offset;
        
        // Lerp to target position
        let current_pos = transform.translation.truncate();
        let new_pos = current_pos.lerp(target_pos, equipment.equipment_type.lerp_factor());
        
        transform.translation.x = new_pos.x;
        transform.translation.y = new_pos.y;
        // Preserve z-order
        
        // Update sprite flip based on player facing direction
        // Get the facing direction from the player physical entity
        if let Ok((facing, is_running, action_state)) = player_query.get(player_gfx.e_gent) {
            sprite.flip_x = facing.direction() < 0.0;
            
            // Update running shake
            if is_running {
                equipment.run_shake_phase = (equipment.run_shake_phase + 1) % RUN_SHAKE_CYCLE_TICKS;
            } else {
                equipment.run_shake_phase = 0;
            }
            
            // Update base offset based on movement direction
            let movement_dir = action_state.clamped_value(&PlayerAction::Move);
            let new_offset = if movement_dir < 0.0 {
                equipment.equipment_type.base_offset_left()
            } else if movement_dir > 0.0 {
                equipment.equipment_type.base_offset_right()
            } else {
                equipment.equipment_type.base_offset_idle()
            };
            
            // Only update if offset changed
            if equipment.base_offset != new_offset {
                equipment.base_offset = new_offset;
            }
        }
    }
}

fn despawn_equipment_on_player_despawn(
    mut commands: Commands,
    mut removed_players: RemovedComponents<PlayerGfx>,
    equipment_query: Query<(Entity, &Equipment)>,
) {
    for removed_player in removed_players.read() {
        for (entity, equipment) in equipment_query.iter() {
            if equipment.player_gfx == removed_player {
                commands.entity(entity).despawn();
            }
        }
    }
} 