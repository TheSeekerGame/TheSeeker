use bevy::prelude::*;
use leafwing_input_manager::prelude::*;
use theseeker_engine::animation::SpriteAnimationBundle;

use crate::{
    game::{
        player::{Passive, Passives, Player, PlayerAction},
        pickups::PickupAssetHandles,
    },
    prelude::*,
};

const WINDOW_WIDTH: f32 = 118.0;
const WINDOW_HEIGHT: f32 = 74.0;
const SLOT_SIZE: f32 = 12.0;
const ICON_SIZE: f32 = 12.0;

const TOP_ROW_Y: f32 = 26.0;
const TOP_ROW_SLOTS: usize = 4;
const TOP_ROW_START_X: f32 = -24.0;
const TOP_ROW_GAP: f32 = 4.0;

const BOTTOM_ROWS: usize = 3;
const BOTTOM_ROW_SLOTS: usize = 7;
const BOTTOM_ROW_START_X: f32 = -48.0;
const BOTTOM_ROW_START_Y: f32 = 6.0;
const BOTTOM_ROW_GAP: f32 = 4.0;
const BOTTOM_ROW_VERTICAL_GAP: f32 = 4.0;

pub struct PassiveInventoryPlugin;

impl Plugin for PassiveInventoryPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (
                toggle_inventory_window,
                update_passive_slots,
                handle_slot_clicks,
                handle_slot_hover,
            )
                .run_if(in_state(crate::GameState::Playing)),
        );
    }
}

#[derive(Component)]
pub struct PassiveInventoryWindow;

#[derive(Component)]
pub struct PassiveSlot {
    slot_type: SlotType,
    row: usize,
    col: usize,
}

#[derive(Component)]
pub struct PassiveIcon {
    passive: Passive,
}

#[derive(Component)]
pub struct PassiveHoverPopup {
    passive: Passive,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlotType {
    Equipped,
    Inventory,
}

#[derive(Event)]
pub struct ToggleInventoryEvent;

fn toggle_inventory_window(
    mut commands: Commands,
    input_query: Query<&ActionState<PlayerAction>, With<Player>>,
    window_query: Query<Entity, With<PassiveInventoryWindow>>,
    _pickup_assets: Res<PickupAssetHandles>,
    camera_query: Query<&GlobalTransform, With<Camera2d>>,
    mut passives_query: Query<&mut Passives, With<Player>>,
) {
    let Ok(action_state) = input_query.single() else {
        return;
    };

    if !action_state.just_pressed(&PlayerAction::TogglePassiveInventory) {
        return;
    }

    if let Ok(window_entity) = window_query.single() {
        commands.entity(window_entity).despawn();
        info!("Passive inventory window closed");
    } else {
        info!("Opening passive inventory window");
        let Ok(camera_transform) = camera_query.single() else {
            return;
        };

        let camera_pos = camera_transform.translation().truncate();
        let top_left = camera_pos - Vec2::new(100.0, -100.0);
        let window_pos = top_left + Vec2::new(WINDOW_WIDTH / 2.0 + 10.0, -WINDOW_HEIGHT / 2.0 - 10.0);
        
        info!("Camera pos: {:?}, Window pos: {:?}", camera_pos, window_pos);

        let mut window = commands.spawn((
            PassiveInventoryWindow,
            SpriteAnimationBundle::new_play_key("anim.ui.passivesbg"),
            Sprite {
                texture_atlas: Some(TextureAtlas::default()),
                ..default()
            },
            Transform::from_translation(window_pos.extend(500.0)),
            GlobalTransform::default(),
            Visibility::Visible,
            InheritedVisibility::default(),
            ViewVisibility::default(),
            StateDespawnMarker,
        ));

        window.with_children(|parent| {
            for col in 0..TOP_ROW_SLOTS {
                let x = TOP_ROW_START_X + col as f32 * (SLOT_SIZE + TOP_ROW_GAP);
                let y = TOP_ROW_Y;

                parent.spawn((
                    PassiveSlot {
                        slot_type: SlotType::Equipped,
                        row: 0,
                        col,
                    },
                    Transform::from_translation(Vec3::new(x, y, 1.0)),
                ));
            }

            for row in 0..BOTTOM_ROWS {
                for col in 0..BOTTOM_ROW_SLOTS {
                    let x = BOTTOM_ROW_START_X + col as f32 * (SLOT_SIZE + BOTTOM_ROW_GAP);
                    let y = BOTTOM_ROW_START_Y - row as f32 * (SLOT_SIZE + BOTTOM_ROW_VERTICAL_GAP);

                    parent.spawn((
                        PassiveSlot {
                            slot_type: SlotType::Inventory,
                            row,
                            col,
                        },
                        Transform::from_translation(Vec3::new(x, y, 1.0)),
                    ));
                }
            }
        });
        
        // Force update by marking passives as changed
        if let Ok(mut passives) = passives_query.single_mut() {
            passives.set_changed();
        }
    }
}

fn update_passive_slots(
    mut commands: Commands,
    passives_query: Query<&Passives, With<Player>>,
    window_query: Query<Entity, With<PassiveInventoryWindow>>,
    slot_query: Query<(Entity, &PassiveSlot, Option<&Children>)>,
    icon_query: Query<Entity, With<PassiveIcon>>,
    pickup_assets: Res<PickupAssetHandles>,
) {
    let Ok(window_entity) = window_query.single() else {
        return;
    };

    let Ok(passives) = passives_query.single() else {
        return;
    };

    // Clear all existing icons
    for entity in icon_query.iter() {
        commands.entity(entity).despawn();
    }

    let equipped_passives = &passives.equipped;
    let inventory_passives = &passives.inventory;

    // Update slots with new icons
    for (slot_entity, slot, children) in slot_query.iter() {
        // Remove existing children first
        if let Some(children) = children {
            for child_entity in children.iter() {
                if icon_query.contains(child_entity) {
                    commands.entity(child_entity).despawn();
                }
            }
        }

        match slot.slot_type {
            SlotType::Equipped => {
                if let Some(passive) = equipped_passives.get(slot.col) {
                    if let Some(handle) = pickup_assets.get_passive_handle(passive) {
                        let icon_entity = commands
                            .spawn((
                                PassiveIcon {
                                    passive: *passive,
                                },
                                Sprite {
                                    image: handle.clone(),
                                    ..default()
                                },
                                Transform::from_scale(Vec3::splat(ICON_SIZE / 12.0)),
                            ))
                            .id();
                        commands.entity(slot_entity).add_child(icon_entity);
                    }
                }
            }
            SlotType::Inventory => {
                let index = slot.row * BOTTOM_ROW_SLOTS + slot.col;
                if let Some(passive) = inventory_passives.get(index) {
                    if let Some(handle) = pickup_assets.get_passive_handle(passive) {
                        let icon_entity = commands
                            .spawn((
                                PassiveIcon {
                                    passive: *passive,
                                },
                                Sprite {
                                    image: handle.clone(),
                                    ..default()
                                },
                                Transform::from_scale(Vec3::splat(ICON_SIZE / 12.0)),
                            ))
                            .id();
                        commands.entity(slot_entity).add_child(icon_entity);
                    }
                }
            }
        }
    }
}

fn handle_slot_clicks(
    mouse_button: Res<ButtonInput<MouseButton>>,
    window_query: Query<&Window>,
    camera_query: Query<(&Camera, &GlobalTransform), With<Camera2d>>,
    slot_query: Query<(&PassiveSlot, &GlobalTransform, Option<&Children>)>,
    icon_query: Query<&PassiveIcon>,
    mut passives_query: Query<&mut Passives, With<Player>>,
) {
    if !mouse_button.just_pressed(MouseButton::Left) {
        return;
    }

    let Ok(window) = window_query.single() else {
        return;
    };

    let Ok((camera, camera_transform)) = camera_query.single() else {
        return;
    };

    let Some(cursor_position) = window.cursor_position() else {
        return;
    };

    let Ok(world_position) = camera.viewport_to_world_2d(camera_transform, cursor_position) else {
        return;
    };

    let Ok(mut passives) = passives_query.single_mut() else {
        return;
    };

    for (slot, slot_transform, children) in slot_query.iter() {
        let slot_pos = slot_transform.translation().truncate();
        let half_size = SLOT_SIZE / 2.0;

        if world_position.x >= slot_pos.x - half_size
            && world_position.x <= slot_pos.x + half_size
            && world_position.y >= slot_pos.y - half_size
            && world_position.y <= slot_pos.y + half_size
        {
            match slot.slot_type {
                SlotType::Equipped => {
                    if let Some(children) = children {
                        for child_entity in children.iter() {
                            if let Ok(icon) = icon_query.get(child_entity) {
                                passives.unequip_passive(icon.passive);
                                break;
                            }
                        }
                    }
                }
                SlotType::Inventory => {
                    if let Some(children) = children {
                        for child_entity in children.iter() {
                            if let Ok(icon) = icon_query.get(child_entity) {
                                passives.equip_passive(icon.passive);
                                break;
                            }
                        }
                    }
                }
            }
            break;
        }
    }
}

fn handle_slot_hover(
    mut commands: Commands,
    window_query: Query<&Window>,
    camera_query: Query<(&Camera, &GlobalTransform), With<Camera2d>>,
    slot_query: Query<(&PassiveSlot, &GlobalTransform, Option<&Children>)>,
    icon_query: Query<&PassiveIcon>,
    hover_popup_query: Query<Entity, With<PassiveHoverPopup>>,
    inventory_window_query: Query<&GlobalTransform, With<PassiveInventoryWindow>>,
) {
    let Ok(window) = window_query.single() else {
        // Remove any existing hover popup if no window
        for entity in hover_popup_query.iter() {
            commands.entity(entity).despawn();
        }
        return;
    };

    let Ok((camera, camera_transform)) = camera_query.single() else {
        // Remove any existing hover popup if no camera
        for entity in hover_popup_query.iter() {
            commands.entity(entity).despawn();
        }
        return;
    };

    let Some(cursor_position) = window.cursor_position() else {
        // Remove any existing hover popup if cursor not in window
        for entity in hover_popup_query.iter() {
            commands.entity(entity).despawn();
        }
        return;
    };

    let Ok(world_position) = camera.viewport_to_world_2d(camera_transform, cursor_position) else {
        // Remove any existing hover popup if can't convert position
        for entity in hover_popup_query.iter() {
            commands.entity(entity).despawn();
        }
        return;
    };

    let Ok(inventory_transform) = inventory_window_query.single() else {
        // Remove any existing hover popup if no inventory window
        for entity in hover_popup_query.iter() {
            commands.entity(entity).despawn();
        }
        return;
    };

    // Check if hovering over any slot with a passive
    let mut hovering_passive = None;
    for (slot, slot_transform, children) in slot_query.iter() {
        let slot_pos = slot_transform.translation().truncate();
        let half_size = SLOT_SIZE / 2.0;

        if world_position.x >= slot_pos.x - half_size
            && world_position.x <= slot_pos.x + half_size
            && world_position.y >= slot_pos.y - half_size
            && world_position.y <= slot_pos.y + half_size
        {
            if let Some(children) = children {
                for child_entity in children.iter() {
                    if let Ok(icon) = icon_query.get(child_entity) {
                        hovering_passive = Some(icon.passive);
                        break;
                    }
                }
            }
            break;
        }
    }

    // Update hover popup
    match hovering_passive {
        Some(passive) => {
            // Check if we already have a popup for this passive
            let existing_popup = hover_popup_query.iter().find_map(|entity| {
                hover_popup_query
                    .get(entity)
                    .ok()
                    .filter(|_| commands.get_entity(entity).is_ok())
                    .map(|_| entity)
            });

            if let Some(existing_entity) = existing_popup {
                // Get the component to check if it's the same passive
                if let Ok(mut entity_commands) = commands.get_entity(existing_entity) {
                    // For now, just remove and recreate (simpler than checking passive type)
                    entity_commands.despawn();
                }
            }

            // Create new popup below the inventory window
            // Convert world position to screen position for the inventory window
            if let Ok(screen_pos) = camera.world_to_viewport(camera_transform, inventory_transform.translation()) {
                commands.spawn((
                    PassiveHoverPopup { passive },
                    Node {
                        position_type: bevy::ui::PositionType::Absolute,
                        left: Val::Px(screen_pos.x - 295.0), // Center the popup (590/2 = 295)
                        top: Val::Px(screen_pos.y + WINDOW_HEIGHT / 2.0 + 148.0), // Position below window
                        width: Val::Px(590.0),
                        padding: bevy::ui::UiRect::all(Val::Px(12.0)),
                        border: bevy::ui::UiRect::all(Val::Px(2.0)),
                        ..Default::default()
                    },
                    BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.9)),
                    bevy::ui::BorderColor(Color::srgba(0.4, 0.4, 0.4, 1.0)),
                    bevy::ui::BorderRadius::all(Val::Px(4.0)),
                    StateDespawnMarker,
                )).with_children(|parent| {
                    parent.spawn((
                        Text::new(format!(
                            "{}\n\n{}",
                            passive.name(),
                            passive.description(),
                        )),
                        TextFont::from_font_size(16.0),
                        TextColor(Color::WHITE),
                        bevy::text::TextLayout::new(
                            bevy::text::JustifyText::Center,
                            bevy::text::LineBreak::WordBoundary,
                        ),
                    ));
                });
            }
        }
        None => {
            // Remove any existing hover popup
            for entity in hover_popup_query.iter() {
                commands.entity(entity).despawn();
            }
        }
    }
}
