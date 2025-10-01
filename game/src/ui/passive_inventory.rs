use bevy::ecs::hierarchy::ChildOf;
use bevy::prelude::*;
use bevy::render::camera::Projection;
use leafwing_input_manager::prelude::*;
use theseeker_engine::animation::SpriteAnimationBundle;

use crate::{
    camera::MainCamera,
    game::player::{Passive, Passives, Player, PlayerAction},
    prelude::*,
};

const WINDOW_WIDTH: f32 = 118.0;
// Height accommodates an extra bottom row of 7 slots
const WINDOW_HEIGHT: f32 = 90.0;
const SLOT_SIZE: f32 = 12.0;
const ICON_SIZE: f32 = 12.0;
const VIEWPORT_MARGIN: f32 = 10.0;

const TOP_ROW_Y: f32 = 26.0;
const TOP_ROW_SLOTS: usize = 4;
const TOP_ROW_START_X: f32 = -24.0;
const TOP_ROW_GAP: f32 = 4.0;

// Inventory grid rows
const BOTTOM_ROWS: usize = 4;
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
                fix_visibility_hierarchy,
            )
                .run_if(in_state(crate::GameState::Playing)),
        );
        app.add_systems(
            GameTickUpdate,
            reposition_passive_inventory_window
                .after(crate::camera::update_camera)
                .run_if(in_state(crate::GameState::Playing)),
        );
    }
}

#[derive(Component)]
pub struct PassiveInventoryWindow;

#[derive(Component)]
pub struct PassiveInventoryUiRoot;

#[derive(Component)]
pub struct PassiveSlot {
    slot_type: SlotType,
    row: usize,
    col: usize,
}

#[derive(Component)]
pub struct PassiveIcon {
    // Used by click/hover handlers
    pub(crate) passive: Passive,
}

#[derive(Component)]
pub struct PassiveHoverPopup {
    #[allow(dead_code)] // Used for marker/type safety; actual passive data retrieved from icon
    passive: Passive,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlotType {
    Equipped,
    Inventory,
}

// Removed: no longer used

fn toggle_inventory_window(
    mut commands: Commands,
    input_query: Query<&ActionState<PlayerAction>, With<Player>>,
    window_query: Query<Entity, With<PassiveInventoryWindow>>,
    ui_root_query: Query<Entity, With<PassiveInventoryUiRoot>>,
    camera_query: Query<(&Transform, &Projection), With<MainCamera>>,
    mut _passives_query: Query<&mut Passives, With<Player>>,
) {
    let Ok(action_state) = input_query.single() else {
        return;
    };

    if !action_state.just_pressed(&PlayerAction::TogglePassiveInventory) {
        return;
    }

    if let Ok(window_entity) = window_query.single() {
        commands.entity(window_entity).despawn();
        if let Ok(ui_root) = ui_root_query.single() {
            commands.entity(ui_root).despawn();
        }
        info!("Passive inventory window closed");
    } else {
        info!("Opening passive inventory window");
        let Ok((camera_transform, projection)) = camera_query.single() else {
            return;
        };

        let window_pos = anchor_top_left(
            camera_transform,
            projection,
            WINDOW_WIDTH,
            WINDOW_HEIGHT,
            VIEWPORT_MARGIN,
        );

        info!("Passive inventory window pos: {:?}", window_pos);

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
                let x =
                    TOP_ROW_START_X + col as f32 * (SLOT_SIZE + TOP_ROW_GAP);
                let y = TOP_ROW_Y;

                parent.spawn((
                    PassiveSlot {
                        slot_type: SlotType::Equipped,
                        row: 0,
                        col,
                    },
                    Transform::from_translation(Vec3::new(x, y, 1.0)),
                    Visibility::Visible,
                    InheritedVisibility::default(),
                    ViewVisibility::default(),
                ));
            }

            for row in 0..BOTTOM_ROWS {
                for col in 0..BOTTOM_ROW_SLOTS {
                    let x = BOTTOM_ROW_START_X
                        + col as f32 * (SLOT_SIZE + BOTTOM_ROW_GAP);
                    let y = BOTTOM_ROW_START_Y
                        - row as f32 * (SLOT_SIZE + BOTTOM_ROW_VERTICAL_GAP);

                    parent.spawn((
                        PassiveSlot {
                            slot_type: SlotType::Inventory,
                            row,
                            col,
                        },
                        Transform::from_translation(Vec3::new(x, y, 1.0)),
                        Visibility::Visible,
                        InheritedVisibility::default(),
                        ViewVisibility::default(),
                    ));
                }
            }
        });

        // Spawn a UI root overlay for popups/labels as a separate UI hierarchy
        commands.spawn((
            PassiveInventoryUiRoot,
            Node {
                position_type: bevy::ui::PositionType::Absolute,
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                ..Default::default()
            },
            Visibility::Visible,
            InheritedVisibility::default(),
            ViewVisibility::default(),
            StateDespawnMarker,
        ));
    }
}

fn anchor_top_left(
    camera_transform: &Transform,
    projection: &Projection,
    window_width: f32,
    window_height: f32,
    margin: f32,
) -> Vec2 {
    let cam_pos = camera_transform.translation.truncate();
    let half_size = match projection {
        Projection::Orthographic(ortho) => ortho.area.half_size(),
        _ => Vec2::new(160.0, 120.0), // sensible fallback
    };
    Vec2::new(
        cam_pos.x - half_size.x + margin + window_width * 0.5,
        cam_pos.y + half_size.y - margin - window_height * 0.5,
    )
}

fn reposition_passive_inventory_window(
    mut window_q: Query<&mut Transform, With<PassiveInventoryWindow>>,
    cam_q: Query<(&Transform, &Projection), (With<MainCamera>, Without<PassiveInventoryWindow>)>,
) {
    let Ok((cam_transform, projection)) = cam_q.single() else {
        return;
    };
    for mut transform in &mut window_q {
        let z = transform.translation.z;
        let pos = anchor_top_left(
            cam_transform,
            projection,
            WINDOW_WIDTH,
            WINDOW_HEIGHT,
            VIEWPORT_MARGIN,
        );
        transform.translation = pos.extend(z);
    }
}

fn update_passive_slots(
    mut commands: Commands,
    passives_query: Query<&Passives, With<Player>>,
    window_query: Query<Entity, With<PassiveInventoryWindow>>,
    slot_query: Query<(Entity, &PassiveSlot, Option<&Children>)>,
    icon_query: Query<Entity, With<PassiveIcon>>,
    asset_server: Res<AssetServer>,
) {
    let Ok(_window_entity) = window_query.single() else {
        return;
    };

    let Ok(passives) = passives_query.single() else {
        return;
    };

    let equipped_passives = &passives.equipped;
    let inventory_passives = &passives.inventory;

    // Rebuild slot icons from current passive inventory snapshot
    for (slot_entity, slot, children) in slot_query.iter() {
        // Remove existing icon children
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
                    let handle = asset_server.load(passive.icon_path());
                    let icon_entity = commands
                        .spawn((
                            PassiveIcon { passive: *passive },
                            Sprite {
                                image: handle,
                                ..default()
                            },
                            Transform::from_scale(Vec3::splat(
                                ICON_SIZE / 12.0,
                            )),
                            GlobalTransform::default(),
                            Visibility::Visible,
                            InheritedVisibility::default(),
                            ViewVisibility::default(),
                        ))
                        .id();
                    commands.entity(slot_entity).add_child(icon_entity);
                }
            },
            SlotType::Inventory => {
                let index = slot.row * BOTTOM_ROW_SLOTS + slot.col;
                if let Some(passive) = inventory_passives.get(index) {
                    let handle = asset_server.load(passive.icon_path());
                    let icon_entity = commands
                        .spawn((
                            PassiveIcon { passive: *passive },
                            Sprite {
                                image: handle,
                                ..default()
                            },
                            Transform::from_scale(Vec3::splat(
                                ICON_SIZE / 12.0,
                            )),
                            GlobalTransform::default(),
                            Visibility::Visible,
                            InheritedVisibility::default(),
                            ViewVisibility::default(),
                        ))
                        .id();
                    commands.entity(slot_entity).add_child(icon_entity);
                }
            },
        }
    }
}

fn handle_slot_clicks(
    mouse_button: Res<ButtonInput<MouseButton>>,
    window_query: Query<&Window>,
    camera_query: Query<(&Camera, &GlobalTransform), With<Camera2d>>,
    slot_query: Query<(
        &PassiveSlot,
        &GlobalTransform,
        Option<&Children>,
    )>,
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

    let Ok(world_position) =
        camera.viewport_to_world_2d(camera_transform, cursor_position)
    else {
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
                },
                SlotType::Inventory => {
                    if let Some(children) = children {
                        for child_entity in children.iter() {
                            if let Ok(icon) = icon_query.get(child_entity) {
                                passives.equip_passive(icon.passive);
                                break;
                            }
                        }
                    }
                },
            }
            break;
        }
    }
}

fn handle_slot_hover(
    mut commands: Commands,
    window_query: Query<&Window>,
    camera_query: Query<(&Camera, &GlobalTransform), With<Camera2d>>,
    slot_query: Query<(
        &PassiveSlot,
        &GlobalTransform,
        Option<&Children>,
    )>,
    icon_query: Query<&PassiveIcon>,
    inventory_window_query: Query<
        &GlobalTransform,
        With<PassiveInventoryWindow>,
    >,
    ui_root_query: Query<Entity, With<PassiveInventoryUiRoot>>,
    mut popup_nodes: Query<&mut Node, With<PassiveHoverPopup>>,
    mut current_popup: Local<Option<Entity>>,
    mut last_passive: Local<Option<Passive>>,
) {
    let Ok(window) = window_query.single() else {
        // Clean up any existing popup if window not available
        if let Some(entity) = *current_popup {
            if let Ok(mut ecmd) = commands.get_entity(entity) {
                ecmd.despawn();
            }
            *current_popup = None;
            *last_passive = None;
        }
        return;
    };

    let Ok((camera, camera_transform)) = camera_query.single() else {
        if let Some(entity) = *current_popup {
            if let Ok(mut ecmd) = commands.get_entity(entity) {
                ecmd.despawn();
            }
            *current_popup = None;
            *last_passive = None;
        }
        return;
    };

    let Some(cursor_position) = window.cursor_position() else {
        if let Some(entity) = *current_popup {
            if let Ok(mut ecmd) = commands.get_entity(entity) {
                ecmd.despawn();
            }
            *current_popup = None;
            *last_passive = None;
        }
        return;
    };

    let Ok(world_position) =
        camera.viewport_to_world_2d(camera_transform, cursor_position)
    else {
        if let Some(entity) = *current_popup {
            if let Ok(mut ecmd) = commands.get_entity(entity) {
                ecmd.despawn();
            }
            *current_popup = None;
            *last_passive = None;
        }
        return;
    };

    let Ok(inventory_transform) = inventory_window_query.single() else {
        if let Some(entity) = *current_popup {
            if let Ok(mut ecmd) = commands.get_entity(entity) {
                ecmd.despawn();
            }
            *current_popup = None;
            *last_passive = None;
        }
        return;
    };

    // Ensure we have a UI root to parent the popup
    let Ok(ui_root) = ui_root_query.single() else {
        if let Some(entity) = *current_popup {
            if let Ok(mut ecmd) = commands.get_entity(entity) {
                ecmd.despawn();
            }
            *current_popup = None;
            *last_passive = None;
        }
        return;
    };

    // Determine if hovering a slot with an icon
    let mut hovering_passive = None;
    for (_slot, slot_transform, children) in slot_query.iter() {
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

    match hovering_passive {
        Some(passive) => {
            // Only rebuild popup if passive changed or popup missing
            let needs_rebuild =
                last_passive.map(|p| p != passive).unwrap_or(true)
                    || current_popup.is_none();
            if needs_rebuild {
                // Despawn existing
                if let Some(entity) = *current_popup {
                    if let Ok(mut ecmd) = commands.get_entity(entity) {
                        ecmd.despawn();
                    }
                    *current_popup = None;
                }
                if let Ok(screen_pos) = camera.world_to_viewport(
                    camera_transform,
                    inventory_transform.translation(),
                ) {
                    let popup_entity = commands
                        .spawn((
                            PassiveHoverPopup { passive },
                            Node {
                                position_type: bevy::ui::PositionType::Absolute,
                                left: Val::Px(screen_pos.x - 295.0),
                                // Place popup below the inventory window using top-origin (relative to parent height)
                                top: Val::Px(
                                    screen_pos.y + WINDOW_HEIGHT / 2.0 + 148.0,
                                ),
                                width: Val::Px(590.0),
                                padding: bevy::ui::UiRect::all(Val::Px(12.0)),
                                border: bevy::ui::UiRect::all(Val::Px(2.0)),
                                ..Default::default()
                            },
                            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.9)),
                            bevy::ui::BorderColor(Color::srgba(
                                0.4, 0.4, 0.4, 1.0,
                            )),
                            bevy::ui::BorderRadius::all(Val::Px(4.0)),
                            Visibility::Visible,
                            InheritedVisibility::default(),
                            ViewVisibility::default(),
                            StateDespawnMarker,
                        ))
                        .with_children(|parent| {
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
                        })
                        .id();
                    // Parent to UI root to avoid non-UI hierarchy warnings and ensure correct UI layout
                    commands.entity(ui_root).add_child(popup_entity);
                    *current_popup = Some(popup_entity);
                    *last_passive = Some(passive);
                }
            } else {
                // Update popup position to remain locked to the window while hovering
                if let Some(entity) = *current_popup {
                    if let Ok(screen_pos) = camera.world_to_viewport(
                        camera_transform,
                        inventory_transform.translation(),
                    ) {
                        if let Ok(mut node) = popup_nodes.get_mut(entity) {
                            node.left = Val::Px(screen_pos.x - 295.0);
                            node.top = Val::Px(
                                screen_pos.y + WINDOW_HEIGHT / 2.0 + 148.0,
                            );
                        }
                    }
                }
            }
        },
        None => {
            if let Some(entity) = *current_popup {
                if let Ok(mut ecmd) = commands.get_entity(entity) {
                    ecmd.despawn();
                }
                *current_popup = None;
                *last_passive = None;
            }
        },
    }
}

/// Ensures that any parent of an entity with InheritedVisibility also has the full visibility trio.
/// This prevents Bevy B0004 warnings in cases where children (UI or sprites) are parented to logic-only entities.
fn fix_visibility_hierarchy(
    mut commands: Commands,
    children_with_inherited: Query<
        (&ChildOf, Entity),
        With<InheritedVisibility>,
    >,
    parent_has_visibility: Query<(), With<Visibility>>,
    parent_has_inherited: Query<(), With<InheritedVisibility>>,
    parent_has_view: Query<(), With<ViewVisibility>>,
) {
    for (child_of, _child) in children_with_inherited.iter() {
        let parent = child_of.parent();

        // Only insert missing components; do not override existing visibility state
        let mut to_insert_visibility = None;
        if parent_has_visibility.get(parent).is_err() {
            to_insert_visibility = Some(Visibility::Visible);
        }

        let mut needs_inherited = parent_has_inherited.get(parent).is_err();
        let mut needs_view = parent_has_view.get(parent).is_err();

        if to_insert_visibility.is_some() || needs_inherited || needs_view {
            let mut ecmd = commands.entity(parent);
            if let Some(vis) = to_insert_visibility.take() {
                ecmd.insert(vis);
            }
            if needs_inherited {
                ecmd.insert(InheritedVisibility::default());
            }
            if needs_view {
                ecmd.insert(ViewVisibility::default());
            }
        }
    }
}
