use crate::game::player::{
    Attacking, CanDash, CanStealth, Player, PlayerConfig, WhirlAbility,
};
use crate::graphics::{ability_cooldown, player_hp};
use crate::prelude::*;
use crate::ui::ability_widget::{
    AbilityWidget,
    AbilityWidgetCommands,
};
use std::f32::consts::PI;

pub struct SkillToolbarPlugin;

impl Plugin for SkillToolbarPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            OnEnter(AppState::InGame),
            spawn_toolbar.after(crate::camera::setup_main_camera),
        );
        app.add_systems(GameTickUpdate, assign_hp_bar);
        app.add_systems(Update, update_attack_ability_ui);
        app.add_systems(Update, update_dash_ability_ui);
        app.add_systems(Update, update_whirl_ability_ui);
        app.add_systems(Update, update_stealth_ability_ui);
    }
}

fn spawn_toolbar(
    mut commands: Commands,
    mut ui_materials: ResMut<Assets<player_hp::Material>>,
    mut ability_materials: ResMut<Assets<ability_cooldown::Material>>,
    game_ui_assets: Res<crate::assets::GameUiAssets>,
) {
    // Create root UI container at bottom center of screen
    let root = commands.spawn((
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
        Name::new("ability_bar_ui"),
    )).id();

    // Create ability bar container
    let ability_bar = commands.spawn((
        Node {
            flex_direction: FlexDirection::Column,
            align_items: AlignItems::Center,
            justify_content: JustifyContent::Center,
            position_type: PositionType::Relative,
            ..default()
        },
        BackgroundColor(Color::NONE),
    )).id();

    commands.entity(root).add_children(&[ability_bar]);

    // Toolbar Frame with abilities - create this first
    let toolbar_frame = commands.spawn((
        ImageNode::new(game_ui_assets.toolbar_frame.clone()),
        Node {
            width: Val::Px(360.0),
            height: Val::Px(150.0),
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            column_gap: Val::Px(10.0),
            flex_direction: FlexDirection::Row,
            position_type: PositionType::Relative,
            ..default()
        },
        BackgroundColor(Color::NONE),
        Name::new("toolbar_frame"),
    )).id();

    // HP Bar Container - positioned absolutely within the toolbar frame
    let hp_container = commands.spawn((
        Node {
            width: Val::Px(270.0),
            height: Val::Px(10.0),
            position_type: PositionType::Absolute,
            top: Val::Px(115.0), // Position from top of toolbar (150 - 35 = 115)
            left: Val::Px(45.0),
            ..default()
        },
        BackgroundColor(Color::srgb(0.0, 0.0, 0.0)),
        ZIndex(-1), // Render below the toolbar frame
        Name::new("hp_bg"),
    )).id();

    // HP Bar Fill
    let hp_bar = commands.spawn((
        MaterialNode(ui_materials.add(player_hp::Material {
            factor: 1.0,
            background_color: Color::srgb(
                0.23137254901960785,
                0.12549019607843137,
                0.12549019607843137,
            ).into(),
            filled_color: Color::srgb(
                0.6352941176470588,
                0.196078431372549022,
                0.3058823529411765,
            ).into(),
        })),
        Node {
            width: Val::Percent(100.0),
            height: Val::Percent(100.0),
            ..default()
        },
        player_hp::Bar(Entity::PLACEHOLDER),
        Name::new("hp_bar"),
        PlayerHpUI,
    )).id();

    commands.entity(hp_container).add_children(&[hp_bar]);

    // Create ability widgets
    spawn_ability_widget(&mut commands, &mut ability_materials, game_ui_assets.attack_skill_icon.clone(), AttackAbilityUI, true, toolbar_frame);
    spawn_ability_widget(&mut commands, &mut ability_materials, game_ui_assets.dash_skill_icon.clone(), DashAbilityUI, true, toolbar_frame);
    spawn_ability_widget(&mut commands, &mut ability_materials, game_ui_assets.whirl_skill_icon.clone(), WhirlAbilityUI, false, toolbar_frame);
    spawn_ability_widget(&mut commands, &mut ability_materials, game_ui_assets.stealth_skill_icon.clone(), StealthAbilityUI, true, toolbar_frame);

    // XP Bar Container - positioned absolutely within the toolbar frame
    let xp_container = commands.spawn((
        Node {
            width: Val::Px(270.0),
            height: Val::Px(5.0),
            position_type: PositionType::Absolute,
            bottom: Val::Px(15.0), // 3 asset pixels * 5x scaling = 15 screen pixels from bottom
            left: Val::Px(45.0),
            ..default()
        },
        BackgroundColor(Color::srgb(0.0, 0.0, 0.0)),
        ZIndex(-1), // Render below the toolbar frame
        Name::new("xp_bg"),
    )).id();

    // XP Bar Fill
    let xp_bar = commands.spawn((
        MaterialNode(ui_materials.add(player_hp::Material {
            factor: 1.0,
            background_color: Color::srgb(
                0.17647058823529413,
                0.15294117647058825,
                0.22745098039215686,
            ).into(),
            filled_color: Color::srgb(
                0.24705882352941178,
                0.3137254901960784,
                0.43137254901960786,
            ).into(),
        })),
        Node {
            width: Val::Percent(100.0),
            height: Val::Percent(100.0),
            ..default()
        },
        Name::new("xp_bar"),
    )).id();

    commands.entity(xp_container).add_children(&[xp_bar]);

    // Build the hierarchy: add bars first (lower z-order), then toolbar frame on top
    commands.entity(ability_bar).add_children(&[hp_container, xp_container, toolbar_frame]);
}

fn spawn_ability_widget<T: Component + Clone>(
    commands: &mut Commands,
    ability_materials: &mut ResMut<Assets<ability_cooldown::Material>>,
    image_handle: Handle<Image>,
    tracking_component: T,
    dir_up: bool,
    parent: Entity,
) {
    // Ability container
    let ability_container = commands.spawn((
        Node {
            width: Val::Px(60.0),
            height: Val::Px(60.0),
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            ..default()
        },
        BackgroundColor(Color::NONE),
    )).id();

    // Ability icon
    let ability_icon = commands.spawn((
        ImageNode::new(image_handle),
        Node {
            width: Val::Px(60.0),
            height: Val::Px(60.0),
            position_type: PositionType::Absolute,
            ..default()
        },
        AbilityWidget,
        tracking_component.clone(),
        Name::new("ability"),
    )).id();

    // Ability cooldown overlay
    let cooldown_material = ability_materials.add(ability_cooldown::Material {
        factor: 0.3,
        background_color: Color::srgba(0.0, 0.0, 0.0, 0.0).into(),
        filled_color: Color::srgba(0.25, 0.25, 0.0, 0.75).into(),
    });

    let ability_cooldown = commands.spawn((
        MaterialNode(cooldown_material),
        Node {
            width: Val::Px(60.0),
            height: Val::Px(60.0),
            position_type: PositionType::Absolute,
            ..default()
        },
        Transform::from_rotation(
            Quat::from_axis_angle(
                Vec3::Z,
                if dir_up { -1.0 } else { 1.0 } * PI * 0.5,
            ),
        ),
        tracking_component,
        Name::new("ability"),
    )).id();

    commands.entity(ability_container).add_children(&[ability_icon, ability_cooldown]);
    commands.entity(parent).add_children(&[ability_container]);
}

fn assign_hp_bar(
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

#[derive(Component, Clone)]
pub struct AttackAbilityUI;
#[derive(Component, Clone)]
pub struct DashAbilityUI;
#[derive(Component, Clone)]
pub struct WhirlAbilityUI;
#[derive(Component, Clone)]
pub struct StealthAbilityUI;

fn update_attack_ability_ui(
    player: Query<Option<&Attacking>, With<Player>>,
    ui: Query<
        Entity,
        (
            With<AttackAbilityUI>,
            Without<AbilityWidget>,
        ),
    >,
    mut commands: Commands,
) {
    let Some(attack) = player.iter().next() else {
        return;
    };
    for entity in ui.iter() {
        let factor = if let Some(attack) = attack {
            1.0 - attack.ticks as f32 / (Attacking::MAX * 8) as f32
        } else {
            0.0
        };
        commands.entity(entity).factor(factor);
    }
}

fn update_dash_ability_ui(
    player: Query<&CanDash, With<Player>>,
    ui: Query<
        Entity,
        (
            With<DashAbilityUI>,
            Without<AbilityWidget>,
        ),
    >,
    mut commands: Commands,
) {
    let Some(can_dash) = player.iter().next() else {
        return;
    };
    for entity in ui.iter() {
        let factor = can_dash.remaining_cooldown / can_dash.total_cooldown;
        commands.entity(entity).factor(factor);
    }
}

fn update_whirl_ability_ui(
    player: Query<&WhirlAbility, With<Player>>,
    ui: Query<
        Entity,
        (
            With<WhirlAbilityUI>,
            Without<AbilityWidget>,
        ),
    >,
    config: Res<PlayerConfig>,
    mut commands: Commands,
) {
    let Some(whirl) = player.iter().next() else {
        return;
    };
    for entity in ui.iter() {
        let factor = 1.0 - whirl.energy / config.max_whirl_energy;
        commands.entity(entity).factor(factor);
    }
}

fn update_stealth_ability_ui(
    player: Query<&CanStealth, With<Player>>,
    stealth_ui: Query<
        Entity,
        (
            With<StealthAbilityUI>,
            Without<AbilityWidget>,
        ),
    >,
    mut commands: Commands,
    config: Res<PlayerConfig>,
) {
    let Some(stealth) = player.iter().next() else {
        return;
    };
    for entity in stealth_ui.iter() {
        let factor = stealth.remaining_cooldown / config.stealth_cooldown;
        commands.entity(entity).factor(factor);
    }
}
