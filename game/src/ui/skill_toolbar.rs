use crate::game::player::{
    FlickerAbility, Passive, Passives, Player, SkillInventory, WhirlAbility,
};
use crate::graphics::{ability_cooldown, player_hp};
use crate::prelude::*;

use crate::ui::ability_widget::{AbilityWidget, AbilityWidgetCommands};
use std::f32::consts::PI;

pub struct SkillToolbarPlugin;

#[derive(Component)]
pub struct AbilityBarRoot;

impl Plugin for SkillToolbarPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(AppState::InGame), spawn_toolbar);
        app.add_systems(
            GameTickUpdate,
            (
                assign_hp_bar_on_bar_added,
                assign_hp_bar_on_player_added,
            ),
        );
        app.add_systems(
            Update,
            (
                apply_toolbar_preset,
                update_toolbar_skills,
                ensure_toolbar_populated,
            )
                .chain()
                .run_if(in_state(AppState::InGame)),
        );
        app.add_systems(
            Update,
            update_skill_cooldowns.run_if(in_state(AppState::InGame)),
        );
    }
}

#[derive(Component, Clone, Copy)]
struct ToolbarPreset {
    num_slots: usize,
}

fn spawn_toolbar(
    mut commands: Commands,
    mut ui_materials: ResMut<Assets<player_hp::Material>>,
    game_ui_assets: Res<crate::assets::GameUiAssets>,
    player_passives: Query<&Passives, With<Player>>,
) {
    // Create root UI container at bottom center of screen
    let root = commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                bottom: Val::Px(0.0),
                left: Val::Px(0.0),
                right: Val::Px(0.0),
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                flex_direction: FlexDirection::Column,
                ..default()
            },
            StateDespawnMarker,
            AbilityBarRoot,
            Name::new("ability_bar_ui"),
        ))
        .id();

    // Create ability bar container
    let ability_bar = commands
        .spawn((
            Node {
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                position_type: PositionType::Relative,
                ..default()
            },
            BackgroundColor(Color::NONE),
        ))
        .id();

    commands.entity(root).add_children(&[ability_bar]);

    // Toolbar frame with abilities
    let has_battery = player_passives
        .single()
        .map(|p| p.contains(&Passive::BatteryPack))
        .unwrap_or(false);
    let frame_image = if has_battery {
        game_ui_assets.toolbar_frame_5slots.clone()
    } else {
        game_ui_assets.toolbar_frame.clone()
    };
    let num_slots = if has_battery { 5 } else { 4 };
    let frame_width = if has_battery { 86.0 * 5.0 } else { 72.0 * 5.0 };
    let frame_height = 30.0 * 5.0;

    // Container to anchor bars (behind) and frame (above)
    let toolbar_container = commands
        .spawn((
            Node {
                width: Val::Px(frame_width),
                height: Val::Px(frame_height),
                position_type: PositionType::Relative,
                ..default()
            },
            BackgroundColor(Color::NONE),
            Name::new("toolbar_container"),
        ))
        .id();

    commands.entity(ability_bar).add_child(toolbar_container);

    let toolbar_frame = commands
        .spawn((
            ImageNode::new(frame_image),
            Node {
                width: Val::Px(frame_width),
                height: Val::Px(frame_height),
                position_type: PositionType::Absolute,
                left: Val::Px(0.0),
                top: Val::Px(0.0),
                ..default()
            },
            BackgroundColor(Color::NONE),
            ZIndex(1),
            Name::new("toolbar_frame"),
            ToolbarFrame,
            ToolbarPreset { num_slots },
        ))
        .id();
    commands.entity(toolbar_container).add_child(toolbar_frame);

    // Create fixed slot containers for skills at fixed positions
    for slot in 0..num_slots {
        // Align to art window: 45px left margin at 5x scale, 70px stride
        let x_offset = 45.0 + (slot as f32) * 70.0;
        let slot_container = commands
            .spawn((
                Node {
                    width: Val::Px(60.0),
                    height: Val::Px(60.0),
                    position_type: PositionType::Absolute,
                    left: Val::Px(x_offset),
                    top: Val::Px(45.0),
                    ..default()
                },
                SkillSlotContainer { slot },
                Name::new(format!("skill_slot_{}", slot)),
            ))
            .id();
        commands.entity(toolbar_frame).add_child(slot_container);
    }

    // HP bar container - absolute within toolbar frame
    let hp_container = commands
        .spawn((
            Node {
                width: Val::Px(if has_battery { 340.0 } else { 270.0 }),
                height: Val::Px(if has_battery { 12.0 } else { 10.0 }),
                position_type: PositionType::Absolute,
                top: Val::Px(if has_battery { 112.0 } else { 115.0 }),
                left: Val::Px(45.0),
                ..default()
            },
            BackgroundColor(Color::srgb(0.0, 0.0, 0.0)),
            ZIndex(0),
            Name::new("hp_bg"),
        ))
        .id();

    // HP bar fill
    let hp_bar = commands
        .spawn((
            MaterialNode(
                ui_materials.add(player_hp::Material {
                    factor: 1.0,
                    background_color: Color::srgb(
                        0.23137254901960785,
                        0.12549019607843137,
                        0.12549019607843137,
                    )
                    .into(),
                    filled_color: Color::srgb(
                        0.6352941176470588,
                        0.196078431372549022,
                        0.3058823529411765,
                    )
                    .into(),
                }),
            ),
            Node {
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                ..default()
            },
            player_hp::Bar(Entity::PLACEHOLDER),
            Name::new("hp_bar"),
            PlayerHpUI,
        ))
        .id();

    commands.entity(hp_container).add_children(&[hp_bar]);
    commands.entity(toolbar_container).add_child(hp_container);

    // Skill widgets are spawned dynamically based on equipped skills

    // XP bar container - absolute within toolbar frame
    let xp_container = commands
        .spawn((
            Node {
                width: Val::Px(if has_battery { 340.0 } else { 270.0 }),
                height: Val::Px(if has_battery { 6.0 } else { 5.0 }),
                position_type: PositionType::Absolute,
                bottom: Val::Px(15.0),
                left: Val::Px(45.0),
                ..default()
            },
            BackgroundColor(Color::srgb(0.0, 0.0, 0.0)),
            ZIndex(0),
            Name::new("xp_bg"),
        ))
        .id();

    // XP bar fill
    let xp_bar = commands
        .spawn((
            MaterialNode(
                ui_materials.add(player_hp::Material {
                    factor: 1.0,
                    background_color: Color::srgb(
                        0.17647058823529413,
                        0.15294117647058825,
                        0.22745098039215686,
                    )
                    .into(),
                    filled_color: Color::srgb(
                        0.24705882352941178,
                        0.3137254901960784,
                        0.43137254901960786,
                    )
                    .into(),
                }),
            ),
            Node {
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                ..default()
            },
            Name::new("xp_bar"),
        ))
        .id();

    commands.entity(xp_container).add_children(&[xp_bar]);
    commands.entity(toolbar_container).add_child(xp_container);
}

fn apply_toolbar_preset(
    mut commands: Commands,
    mut ui_materials: ResMut<Assets<player_hp::Material>>,
    game_ui_assets: Res<crate::assets::GameUiAssets>,
    passives_q: Query<&Passives, With<Player>>,
    frames_q: Query<(Entity, &ToolbarPreset), With<ToolbarFrame>>,
    roots_q: Query<Entity, With<AbilityBarRoot>>,
) {
    let Ok(passives) = passives_q.single() else {
        return;
    };
    // Early exit unless passives changed; avoids unnecessary work each frame.
    // We avoid `Ref<Passives>` here for API compatibility with other callers.
    let desired = if passives.contains(&Passive::BatteryPack) {
        5
    } else {
        4
    };

    // If a frame exists with the correct preset, do nothing
    if let Ok((_, preset)) = frames_q.single() {
        if preset.num_slots == desired {
            return;
        }
    }
    // Despawn existing toolbar root (and its children) and rebuild for current passives
    if let Ok(root_entity) = roots_q.single() {
        commands.entity(root_entity).despawn();
    }
    // Rebuild fresh using the existing spawn function and current passives
    spawn_toolbar(
        commands,
        ui_materials,
        game_ui_assets,
        passives_q,
    );
}

fn assign_hp_bar_on_bar_added(
    player: Query<Entity, With<Player>>,
    mut hp_bar_q: Query<&mut player_hp::Bar, Added<PlayerHpUI>>,
) {
    let Some(player) = player.iter().next() else {
        return;
    };
    for mut hp_bar in hp_bar_q.iter_mut() {
        hp_bar.0 = player;
    }
}

fn assign_hp_bar_on_player_added(
    player: Query<Entity, Added<Player>>,
    mut hp_bar_q: Query<&mut player_hp::Bar, With<PlayerHpUI>>,
) {
    let Some(player) = player.iter().next() else {
        return;
    };
    for mut hp_bar in hp_bar_q.iter_mut() {
        hp_bar.0 = player;
    }
}

#[derive(Component, Clone)]
pub struct PlayerHpUI;

#[derive(Component)]
pub struct ToolbarFrame;

#[derive(Component)]
pub struct SkillSlotContainer {
    pub slot: usize, // 0-3
}

#[derive(Component, Clone)]
pub struct SkillSlotUI {
    pub slot: usize, // 0-3
}

fn update_toolbar_skills(
    mut commands: Commands,
    player_query: Query<
        &SkillInventory,
        (
            With<Player>,
            Or<(
                Added<SkillInventory>,
                Changed<SkillInventory>,
            )>,
        ),
    >,
    container_query: Query<(
        Entity,
        &SkillSlotContainer,
        Option<&Children>,
    )>,
    _slot_widget_query: Query<Entity, With<SkillSlotUI>>,
    mut ability_materials: ResMut<Assets<ability_cooldown::Material>>,
    game_ui_assets: Res<crate::assets::GameUiAssets>,
) {
    // Update when `SkillInventory` is added or changes
    let Ok(skill_inventory) = player_query.single() else {
        return;
    };

    // Clear all slot containers
    for (_container_entity, _container, maybe_children) in container_query.iter()
    {
        if let Some(children) = maybe_children {
            for &child in children {
                commands.entity(child).despawn();
            }
        }
    }

    // Add skill widgets to appropriate slot containers
    for (slot_index, skill_id) in skill_inventory.equipped.iter().enumerate() {
        // Find the container for this slot
        let Some((container_entity, _, _)) = container_query
            .iter()
            .find(|(_, container, _)| container.slot == slot_index)
        else {
            continue;
        };

        let icon_handle = match skill_id {
            crate::game::player::skills::types::SkillId::Attack => {
                game_ui_assets.attack_skill_icon.clone()
            },
            crate::game::player::skills::types::SkillId::Dash => {
                game_ui_assets.dash_skill_icon.clone()
            },
            crate::game::player::skills::types::SkillId::DashStrike => {
                game_ui_assets.dash_strike_skill_icon.clone()
            },
            crate::game::player::skills::types::SkillId::Whirl => {
                game_ui_assets.whirl_skill_icon.clone()
            },
            crate::game::player::skills::types::SkillId::Stealth => {
                game_ui_assets.stealth_skill_icon.clone()
            },
            crate::game::player::skills::types::SkillId::BurningDash => {
                game_ui_assets.burning_dash_skill_icon.clone()
            },
            crate::game::player::skills::types::SkillId::FlickerStrike => {
                game_ui_assets.flicker_strike_skill_icon.clone()
            },
            crate::game::player::skills::types::SkillId::AmplifiedBell => {
                game_ui_assets.amplified_bell_skill_icon.clone()
            },
            crate::game::player::skills::types::SkillId::Spinner => {
                game_ui_assets.spinner_skill_icon.clone()
            },
            crate::game::player::skills::types::SkillId::ExplosiveMine => {
                game_ui_assets.mine_skill_icon.clone()
            },
            crate::game::player::skills::types::SkillId::IceNova => {
                game_ui_assets.ice_nova_skill_icon.clone()
            },
        };

        // Determine fill rotation based on skill type
        let dir_up = match skill_id {
            crate::game::player::skills::types::SkillId::Whirl => false,
            crate::game::player::skills::types::SkillId::FlickerStrike => false,
            crate::game::player::skills::types::SkillId::AmplifiedBell => true,
            crate::game::player::skills::types::SkillId::Spinner => true,
            _ => true,
        };

        spawn_ability_widget_in_container(
            &mut commands,
            &mut ability_materials,
            icon_handle,
            slot_index,
            dir_up,
            container_entity,
        );
    }
}

fn spawn_ability_widget_in_container(
    commands: &mut Commands,
    ability_materials: &mut ResMut<Assets<ability_cooldown::Material>>,
    image_handle: Handle<Image>,
    slot: usize,
    dir_up: bool,
    container: Entity,
) {
    // Ability icon
    let ability_icon = commands
        .spawn((
            ImageNode::new(image_handle),
            Node {
                width: Val::Px(60.0),
                height: Val::Px(60.0),
                position_type: PositionType::Absolute,
                ..default()
            },
            AbilityWidget,
            SkillSlotUI { slot },
            Name::new(format!("ability_slot_{}", slot)),
        ))
        .id();

    // Ability cooldown overlay
    let cooldown_material = ability_materials.add(ability_cooldown::Material {
        factor: 0.3,
        background_color: Color::srgba(0.0, 0.0, 0.0, 0.0).into(),
        filled_color: Color::srgba(0.25, 0.25, 0.0, 0.75).into(),
    });

    let ability_cooldown = commands
        .spawn((
            MaterialNode(cooldown_material),
            Node {
                width: Val::Px(60.0),
                height: Val::Px(60.0),
                position_type: PositionType::Absolute,
                ..default()
            },
            Transform::from_rotation(Quat::from_axis_angle(
                Vec3::Z,
                if dir_up { -1.0 } else { 1.0 } * PI * 0.5,
            )),
            SkillSlotUI { slot },
            Name::new(format!(
                "ability_cooldown_slot_{}",
                slot
            )),
        ))
        .id();

    // Add both icon and cooldown overlay to the container
    commands
        .entity(container)
        .add_children(&[ability_icon, ability_cooldown]);
}

/// Fallback system to ensure toolbar is populated if both player and toolbar exist
fn ensure_toolbar_populated(
    mut commands: Commands,
    player_query: Query<&SkillInventory, With<Player>>,
    container_query: Query<(
        Entity,
        &SkillSlotContainer,
        Option<&Children>,
    )>,
    skill_widget_query: Query<Entity, With<SkillSlotUI>>,
    mut ability_materials: ResMut<Assets<ability_cooldown::Material>>,
    game_ui_assets: Res<crate::assets::GameUiAssets>,
) {
    // Only act if player exists and containers exist
    let Ok(skill_inventory) = player_query.single() else {
        return;
    };

    // Check if any containers have skill widgets
    let mut any_populated = false;
    for (_, _, maybe_children) in container_query.iter() {
        if let Some(children) = maybe_children {
            for child_entity in children.iter() {
                if skill_widget_query.contains(child_entity) {
                    any_populated = true;
                    break;
                }
            }
            if any_populated {
                break;
            }
        }
    }

    // If no skill widgets exist in any container, populate them
    if !any_populated && !skill_inventory.equipped.is_empty() {
        for (slot_index, skill_id) in
            skill_inventory.equipped.iter().enumerate()
        {
            // Find the container for this slot
            let Some((container_entity, _, _)) = container_query
                .iter()
                .find(|(_, container, _)| container.slot == slot_index)
            else {
                continue;
            };

            let icon_handle = match skill_id {
                crate::game::player::skills::types::SkillId::Attack => {
                    game_ui_assets.attack_skill_icon.clone()
                },
                crate::game::player::skills::types::SkillId::Dash => {
                    game_ui_assets.dash_skill_icon.clone()
                },
                crate::game::player::skills::types::SkillId::DashStrike => {
                    game_ui_assets.dash_strike_skill_icon.clone()
                },
                crate::game::player::skills::types::SkillId::Whirl => {
                    game_ui_assets.whirl_skill_icon.clone()
                },
                crate::game::player::skills::types::SkillId::Stealth => {
                    game_ui_assets.stealth_skill_icon.clone()
                },
                crate::game::player::skills::types::SkillId::BurningDash => {
                    game_ui_assets.burning_dash_skill_icon.clone()
                },
                crate::game::player::skills::types::SkillId::FlickerStrike => {
                    game_ui_assets.flicker_strike_skill_icon.clone()
                },
                crate::game::player::skills::types::SkillId::AmplifiedBell => {
                    game_ui_assets.amplified_bell_skill_icon.clone()
                },
                crate::game::player::skills::types::SkillId::Spinner => {
                    game_ui_assets.spinner_skill_icon.clone()
                },
                crate::game::player::skills::types::SkillId::ExplosiveMine => {
                    game_ui_assets.mine_skill_icon.clone()
                },
                crate::game::player::skills::types::SkillId::IceNova => {
                    game_ui_assets.ice_nova_skill_icon.clone()
                },
            };

            let dir_up = match skill_id {
                crate::game::player::skills::types::SkillId::Whirl => false,
                crate::game::player::skills::types::SkillId::FlickerStrike => {
                    false
                },
                crate::game::player::skills::types::SkillId::AmplifiedBell => {
                    true
                },
                crate::game::player::skills::types::SkillId::Spinner => true,
                _ => true,
            };

            spawn_ability_widget_in_container(
                &mut commands,
                &mut ability_materials,
                icon_handle,
                slot_index,
                dir_up,
                container_entity,
            );
        }
    }
}

fn update_skill_cooldowns(
    player_query: Query<
        (
            Entity,
            &SkillInventory,
            &WhirlAbility,
            &FlickerAbility,
            &crate::game::player::skills::explosive_mine::ExplosiveMineAbility,
        ),
        With<Player>,
    >,
    ui_query: Query<(Entity, &SkillSlotUI), Without<AbilityWidget>>,
    mut commands: Commands,
    cooldowns: Res<crate::game::player::skills::cooldowns::Cooldowns>,
) {
    let Some((player_entity, skill_inventory, whirl, flicker, mine)) =
        player_query.iter().next()
    else {
        return;
    };

    let whirl_cap = crate::game::player::skills::types::whirl_metadata()
        .max_energy;
    let flicker_cap =
        crate::game::player::skills::types::flicker_strike_metadata()
            .max_energy;

    for (entity, slot_ui) in ui_query.iter() {
        if let Some(skill_id) = skill_inventory.get_skill_at_slot(slot_ui.slot)
        {
            // Special handling for whirl (reverse energy bar)
            if skill_id == crate::game::player::skills::types::SkillId::Whirl {
                let factor = 1.0 - whirl.energy / whirl_cap;
                commands.entity(entity).factor(factor);
            } else if skill_id
                == crate::game::player::skills::types::SkillId::FlickerStrike
            {
                // Special handling for flicker (reverse energy bar)
                let factor = 1.0 - flicker.energy / flicker_cap;
                commands.entity(entity).factor(factor);
            } else if skill_id
                == crate::game::player::skills::types::SkillId::ExplosiveMine
            {
                // Discrete 3-chunk model: show only full chunks as available
                let chunks_ready = mine.energy.floor().clamp(0.0, 3.0) as i32;
                let factor = 1.0 - (chunks_ready as f32 / 3.0);
                commands.entity(entity).factor(factor);
            } else {
                // Standard cooldown handling
                if let Some(entry) = cooldowns.get(player_entity, skill_id) {
                    let denom = entry.initial.max(1.0);
                    let factor =
                        (entry.remaining / denom).clamp(0.0, 1.0) as f32;
                    commands.entity(entity).factor(factor);
                } else {
                    commands.entity(entity).factor(0.0);
                }
            }
        }
    }
}
