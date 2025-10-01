use bevy::ecs::hierarchy::ChildOf;
use bevy::prelude::*;
use bevy::render::camera::Projection;
use leafwing_input_manager::prelude::*;
use theseeker_engine::animation::SpriteAnimationBundle;

use crate::{
    camera::MainCamera,
    game::player::{Passive, Passives, Player, PlayerAction, SkillInventory},
    prelude::*,
};

const WINDOW_WIDTH: f32 = 118.0;
// Height accommodates an extra top row of 7 slots
const WINDOW_HEIGHT: f32 = 90.0;
const SLOT_SIZE: f32 = 12.0;
const ICON_SIZE: f32 = 12.0;
const VIEWPORT_MARGIN: f32 = 10.0;

// Bottom row for equipped skills (mirrors passive inventory placement)
const BOTTOM_ROW_Y: f32 = -26.0;
#[allow(dead_code)] // Kept for reference; actual value computed dynamically at runtime
const BOTTOM_ROW_SLOTS: usize = 4;
#[allow(dead_code)] // Kept for reference; actual value computed dynamically based on Battery Pack
const BOTTOM_ROW_START_X: f32 = -24.0;
const BOTTOM_ROW_GAP: f32 = 4.0;

// Top rows for inventory skills (4 rows)
const TOP_ROWS: usize = 4;
const TOP_ROW_SLOTS: usize = 7;
const TOP_ROW_START_X: f32 = -48.0;
const TOP_ROW_START_Y: f32 = -6.0;
const TOP_ROW_GAP: f32 = 4.0;
const TOP_ROW_VERTICAL_GAP: f32 = 4.0;

pub struct SkillInventoryPlugin;

impl Plugin for SkillInventoryPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (
                refresh_inventory_on_battery_pack_change,
                toggle_inventory_window,
                update_skill_slots,
                handle_slot_clicks,
                handle_slot_hover,
                fix_visibility_hierarchy,
            )
                .chain()
                .run_if(in_state(crate::GameState::Playing)),
        );
        app.add_systems(
            GameTickUpdate,
            reposition_skill_inventory_window
                .after(crate::camera::update_camera)
                .run_if(in_state(crate::GameState::Playing)),
        );
    }
}

#[derive(Component)]
pub struct SkillInventoryWindow;

#[derive(Component)]
pub struct SkillInventoryUiRoot;

#[derive(Component)]
pub struct SkillSlot {
    slot_type: SlotType,
    row: usize,
    col: usize,
}

/// Automatically refresh (close and reopen) the skill inventory window when Battery Pack is equipped/unequipped
fn refresh_inventory_on_battery_pack_change(
    mut commands: Commands,
    passives_query: Query<&Passives, With<Player>>,
    window_query: Query<Entity, With<SkillInventoryWindow>>,
    ui_root_query: Query<Entity, With<SkillInventoryUiRoot>>,
    camera_query: Query<(&Transform, &Projection), With<MainCamera>>,
    mut prev_has_battery: Local<Option<bool>>,
) {
    let Ok(passives) = passives_query.single() else {
        return;
    };
    let has_battery = passives.contains(&Passive::BatteryPack);

    // Initialize baseline without triggering refresh
    if prev_has_battery.is_none() {
        *prev_has_battery = Some(has_battery);
        return;
    }

    // Only act if window is currently open and the flag changed
    if *prev_has_battery != Some(has_battery) {
        if let Ok(window_entity) = window_query.single() {
            // Close current window and UI root
            commands.entity(window_entity).despawn();
            if let Ok(ui_root) = ui_root_query.single() {
                commands.entity(ui_root).despawn();
            }

            // Reopen with the correct preset immediately
            let Ok((camera_transform, projection)) = camera_query.single() else {
                *prev_has_battery = Some(has_battery);
                return;
            };

            let window_pos = anchor_top_right(
                camera_transform,
                projection,
                WINDOW_WIDTH,
                WINDOW_HEIGHT,
                VIEWPORT_MARGIN,
            );

            let bg_key = if has_battery {
                "anim.ui.skillsbg5slots"
            } else {
                "anim.ui.skillsbg"
            };

            let mut window = commands.spawn((
                SkillInventoryWindow,
                SpriteAnimationBundle::new_play_key(bg_key),
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
                let equipped_slots = if has_battery { 5 } else { 4 };
                let start_x = -((equipped_slots as f32 - 1.0)
                    * (SLOT_SIZE + BOTTOM_ROW_GAP)
                    * 0.5);
                for col in 0..equipped_slots {
                    let x = start_x + col as f32 * (SLOT_SIZE + BOTTOM_ROW_GAP);
                    let y = BOTTOM_ROW_Y;

                    parent.spawn((
                        SkillSlot {
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

                for row in 0..TOP_ROWS {
                    for col in 0..TOP_ROW_SLOTS {
                        let x = TOP_ROW_START_X
                            + col as f32 * (SLOT_SIZE + TOP_ROW_GAP);
                        let y = TOP_ROW_START_Y
                            + row as f32 * (SLOT_SIZE + TOP_ROW_VERTICAL_GAP);

                        parent.spawn((
                            SkillSlot {
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

            commands.spawn((
                SkillInventoryUiRoot,
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
        *prev_has_battery = Some(has_battery);
    }
}

#[derive(Component)]
pub struct SkillIcon {
    pub(crate) skill: crate::game::player::skills::types::SkillId,
}

#[derive(Component)]
pub struct SkillHoverPopup {
    #[allow(dead_code)] // Used for marker/type safety; actual skill data retrieved from icon
    skill: crate::game::player::skills::types::SkillId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlotType {
    Equipped,
    Inventory,
}

fn toggle_inventory_window(
    mut commands: Commands,
    input_query: Query<&ActionState<PlayerAction>, With<Player>>,
    window_query: Query<Entity, With<SkillInventoryWindow>>,
    ui_root_query: Query<Entity, With<SkillInventoryUiRoot>>,
    camera_query: Query<(&Transform, &Projection), With<MainCamera>>,
    passives_query: Query<&Passives, With<Player>>,
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
        info!("Skill inventory window closed");
    } else {
        info!("Opening skill inventory window");
        let Ok((camera_transform, projection)) = camera_query.single() else {
            return;
        };

        let window_pos = anchor_top_right(
            camera_transform,
            projection,
            WINDOW_WIDTH,
            WINDOW_HEIGHT,
            VIEWPORT_MARGIN,
        );

        info!("Skill inventory window pos: {:?}", window_pos);

        let has_battery = passives_query
            .single()
            .map(|p| p.contains(&Passive::BatteryPack))
            .unwrap_or(false);
        let bg_key = if has_battery {
            "anim.ui.skillsbg5slots"
        } else {
            "anim.ui.skillsbg"
        };

        let mut window = commands.spawn((
            SkillInventoryWindow,
            SpriteAnimationBundle::new_play_key(bg_key),
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
            // Bottom row for equipped skills (dynamic 4/5)
            let equipped_slots = if has_battery { 5 } else { 4 };
            let start_x = -((equipped_slots as f32 - 1.0)
                * (SLOT_SIZE + BOTTOM_ROW_GAP)
                * 0.5);
            for col in 0..equipped_slots {
                let x = start_x + col as f32 * (SLOT_SIZE + BOTTOM_ROW_GAP);
                let y = BOTTOM_ROW_Y;

                parent.spawn((
                    SkillSlot {
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

            // Top rows for inventory skills
            for row in 0..TOP_ROWS {
                for col in 0..TOP_ROW_SLOTS {
                    let x = TOP_ROW_START_X
                        + col as f32 * (SLOT_SIZE + TOP_ROW_GAP);
                    let y = TOP_ROW_START_Y
                        + row as f32 * (SLOT_SIZE + TOP_ROW_VERTICAL_GAP);

                    parent.spawn((
                        SkillSlot {
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
            SkillInventoryUiRoot,
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

fn anchor_top_right(
    camera_transform: &Transform,
    projection: &Projection,
    window_width: f32,
    window_height: f32,
    margin: f32,
) -> Vec2 {
    let cam_pos = camera_transform.translation.truncate();
    let half_size = match projection {
        Projection::Orthographic(ortho) => ortho.area.half_size(),
        _ => Vec2::new(160.0, 120.0),
    };
    Vec2::new(
        cam_pos.x + half_size.x - margin - window_width * 0.5,
        cam_pos.y + half_size.y - margin - window_height * 0.5,
    )
}

fn reposition_skill_inventory_window(
    mut window_q: Query<&mut Transform, With<SkillInventoryWindow>>,
    cam_q: Query<(&Transform, &Projection), (With<MainCamera>, Without<SkillInventoryWindow>)>,
) {
    let Ok((cam_transform, projection)) = cam_q.single() else {
        return;
    };
    for mut transform in &mut window_q {
        let z = transform.translation.z;
        let pos = anchor_top_right(
            cam_transform,
            projection,
            WINDOW_WIDTH,
            WINDOW_HEIGHT,
            VIEWPORT_MARGIN,
        );
        transform.translation = pos.extend(z);
    }
}

fn update_skill_slots(
    mut commands: Commands,
    skills_query: Query<&SkillInventory, With<Player>>,
    window_query: Query<Entity, With<SkillInventoryWindow>>,
    slot_query: Query<(Entity, &SkillSlot, Option<&Children>)>,
    icon_query: Query<Entity, With<SkillIcon>>,
    game_ui_assets: Option<Res<crate::assets::GameUiAssets>>,
) {
    let Ok(_window_entity) = window_query.single() else {
        return;
    };

    let Ok(skills) = skills_query.single() else {
        return;
    };

    // Gracefully handle missing assets during loading
    let Some(game_ui_assets) = game_ui_assets else {
        return;
    };

    let equipped_skills = &skills.equipped;
    let inventory_skills = &skills.inventory;

    // Rebuild slot icons from the current SkillInventory snapshot
    for (slot_entity, slot, children) in slot_query.iter() {
        // Remove existing icon children first
        if let Some(children) = children {
            for child_entity in children.iter() {
                if icon_query.contains(child_entity) {
                    commands.entity(child_entity).despawn();
                }
            }
        }

        match slot.slot_type {
            SlotType::Equipped => {
                if let Some(skill) = equipped_skills.get(slot.col) {
                    if let Some(handle) =
                        get_skill_icon_handle(skill, &game_ui_assets)
                    {
                        let icon_entity = commands
                            .spawn((
                                SkillIcon { skill: *skill },
                                Sprite {
                                    image: handle.clone(),
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
                }
            },
            SlotType::Inventory => {
                let index = slot.row * TOP_ROW_SLOTS + slot.col;
                if let Some(skill) = inventory_skills.get(index) {
                    if let Some(handle) =
                        get_skill_icon_handle(skill, &game_ui_assets)
                    {
                        let icon_entity = commands
                            .spawn((
                                SkillIcon { skill: *skill },
                                Sprite {
                                    image: handle.clone(),
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
                }
            },
        }
    }
}

fn get_skill_icon_handle(
    skill: &crate::game::player::skills::types::SkillId,
    game_ui_assets: &crate::assets::GameUiAssets,
) -> Option<Handle<Image>> {
    use crate::game::player::skills::types::SkillId;

    match skill {
        SkillId::Attack => Some(game_ui_assets.attack_skill_icon.clone()),
        SkillId::Dash => Some(game_ui_assets.dash_skill_icon.clone()),
        SkillId::DashStrike => {
            Some(game_ui_assets.dash_strike_skill_icon.clone())
        },
        SkillId::Whirl => Some(game_ui_assets.whirl_skill_icon.clone()),
        SkillId::Stealth => Some(game_ui_assets.stealth_skill_icon.clone()),
        SkillId::BurningDash => {
            Some(game_ui_assets.burning_dash_skill_icon.clone())
        },
        SkillId::FlickerStrike => {
            Some(game_ui_assets.flicker_strike_skill_icon.clone())
        },
        SkillId::AmplifiedBell => {
            Some(game_ui_assets.amplified_bell_skill_icon.clone())
        },
        SkillId::Spinner => Some(game_ui_assets.spinner_skill_icon.clone()),
        SkillId::ExplosiveMine => Some(game_ui_assets.mine_skill_icon.clone()),
        SkillId::IceNova => Some(game_ui_assets.ice_nova_skill_icon.clone()),
    }
}

fn handle_slot_clicks(
    mouse_button: Res<ButtonInput<MouseButton>>,
    window_query: Query<&Window>,
    camera_query: Query<(&Camera, &GlobalTransform), With<Camera2d>>,
    slot_query: Query<(
        &SkillSlot,
        &GlobalTransform,
        Option<&Children>,
    )>,
    icon_query: Query<&SkillIcon>,
    mut skills_query: Query<&mut SkillInventory, With<Player>>,
    passives_query: Query<&Passives, With<Player>>,
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

    let Ok(mut skills) = skills_query.single_mut() else {
        return;
    };
    let max_slots = passives_query
        .single()
        .map(|p| SkillInventory::max_slots_for(p))
        .unwrap_or(4);

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
                                skills.unequip_skill(icon.skill);
                                break;
                            }
                        }
                    }
                },
                SlotType::Inventory => {
                    if let Some(children) = children {
                        for child_entity in children.iter() {
                            if let Ok(icon) = icon_query.get(child_entity) {
                                // Respect dynamic capacity (4 vs 5)
                                skills.equip_skill_with_capacity(
                                    icon.skill, max_slots,
                                );
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
        &SkillSlot,
        &GlobalTransform,
        Option<&Children>,
    )>,
    icon_query: Query<&SkillIcon>,
    inventory_window_query: Query<&GlobalTransform, With<SkillInventoryWindow>>,
    ui_root_query: Query<Entity, With<SkillInventoryUiRoot>>,
    mut popup_nodes: Query<&mut Node, With<SkillHoverPopup>>,
    mut current_popup: Local<Option<Entity>>,
    mut last_skill: Local<Option<crate::game::player::skills::types::SkillId>>,
) {
    let Ok(window) = window_query.single() else {
        // Clean up any existing popup if window not available
        if let Some(entity) = *current_popup {
            if let Ok(mut ecmd) = commands.get_entity(entity) {
                ecmd.despawn();
            }
            *current_popup = None;
            *last_skill = None;
        }
        return;
    };

    let Ok((camera, camera_transform)) = camera_query.single() else {
        if let Some(entity) = *current_popup {
            if let Ok(mut ecmd) = commands.get_entity(entity) {
                ecmd.despawn();
            }
            *current_popup = None;
            *last_skill = None;
        }
        return;
    };

    let Some(cursor_position) = window.cursor_position() else {
        if let Some(entity) = *current_popup {
            if let Ok(mut ecmd) = commands.get_entity(entity) {
                ecmd.despawn();
            }
            *current_popup = None;
            *last_skill = None;
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
            *last_skill = None;
        }
        return;
    };

    let Ok(inventory_transform) = inventory_window_query.single() else {
        if let Some(entity) = *current_popup {
            if let Ok(mut ecmd) = commands.get_entity(entity) {
                ecmd.despawn();
            }
            *current_popup = None;
            *last_skill = None;
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
            *last_skill = None;
        }
        return;
    };

    // Determine if hovering a slot with an icon
    let mut hovering_skill = None;
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
                        hovering_skill = Some(icon.skill);
                        break;
                    }
                }
            }
            break;
        }
    }

    match hovering_skill {
        Some(skill) => {
            // Only rebuild popup if skill changed or popup missing
            let needs_rebuild = last_skill.map(|s| s != skill).unwrap_or(true)
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
                            SkillHoverPopup { skill },
                            Node {
                                position_type: bevy::ui::PositionType::Absolute,
                                left: Val::Px(screen_pos.x - 295.0),
                                // Place popup above the inventory window
                                top: Val::Px(
                                    screen_pos.y - WINDOW_HEIGHT / 2.0 + 222.0,
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
                                    skill_name(&skill),
                                    skill_description(&skill),
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
                    *last_skill = Some(skill);
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
                                screen_pos.y - WINDOW_HEIGHT / 2.0 + 222.0,
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
                *last_skill = None;
            }
        },
    }
}

fn skill_name(skill: &crate::game::player::skills::types::SkillId) -> &str {
    use crate::game::player::skills::types::SkillId;

    match skill {
        SkillId::Attack => "Basic Attack",
        SkillId::Dash => "Dash",
        SkillId::DashStrike => "Dash Strike",
        SkillId::Whirl => "Whirling Strike",
        SkillId::Stealth => "Stealth",
        SkillId::BurningDash => "Burning Dash",
        SkillId::FlickerStrike => "Flicker Strike",
        SkillId::AmplifiedBell => "Amplified Bell",
        SkillId::Spinner => "Spinner",
        SkillId::ExplosiveMine => "Explosive Mine",
        SkillId::IceNova => "Ice Nova",
    }
}

fn skill_description(
    skill: &crate::game::player::skills::types::SkillId,
) -> &str {
    use crate::game::player::skills::types::SkillId;

    // Placeholder descriptions as requested
    match skill {
        SkillId::Attack => "A basic weapon attack. Adapts to your currently equipped weapon.",
        SkillId::Dash => "Quickly dash in a direction, avoiding enemy attacks.",
        SkillId::DashStrike => "Dive diagonally down by default or strike upward when held, phasing through enemies and unleashing an impact blast on contact.",
        SkillId::Whirl => "Spin with your melee weapon, dealing damage to all nearby enemies.",
        SkillId::Stealth => "Become temporarily invisible to enemies. Attacking breaks stealth.",
        SkillId::BurningDash => "Channel your life force to dash forward, damaging enemies. Consumes health while active.",
        SkillId::FlickerStrike => "Channel to rapidly dash between enemies, striking each one. Consumes energy while active.",
        SkillId::AmplifiedBell => "Deploy a bell on nearby ground. Hitting it triggers ring shocks that damage nearby enemies after a short delay.",
        SkillId::Spinner => "Spawn a follower spinner that initially darts forward, then pulls toward you dealing contact damage. While stealthed it idles and won't be picked up; when visible it despawns on contact.",
        SkillId::ExplosiveMine => "Instantly plant a mine at your feet. It arms after deployment and explodes when an enemy approaches, dealing area damage. Uses 1/3 energy per mine; energy recharges in 1/3 chunks.",
        SkillId::IceNova => "Instantly release a devastating burst of cold around you. Deals damage and freezes nearby enemies solid for a short duration.",
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
