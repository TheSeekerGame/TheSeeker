use crate::appstate::{AppState, StateDespawnMarker};
use crate::assets::{MainMenuAssets, UiAssets};
use crate::camera::MainCamera;
use crate::game::attack::Health;
use crate::game::player::{
    Attacking, CanAttack, CanDash, Dashing, FocusAbility, FocusState, Player, PlayerConfig,
    WhirlAbility,
};
use crate::graphics::hp_bar::HpBarUiMaterial;
use crate::prelude::*;
use crate::ui::ability_widget::{
    AbilityWidget, AbilityWidgetCommands, AbilityWidgetConfig, UiAbilityWidgetExt,
};
use sickle_ui::ui_builder::{UiBuilderExt, UiRoot};
use sickle_ui::ui_style::*;
use sickle_ui::widgets::prelude::*;

pub struct SkillToolbarPlugin;

impl Plugin for SkillToolbarPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            OnEnter(AppState::InGame),
            spawn_toolbar.after(crate::camera::setup_main_camera),
        );
        app.add_systems(Update, update_attack_ability_ui);
        app.add_systems(Update, update_dash_ability_ui);
        app.add_systems(Update, update_whirl_ability_ui);
        app.add_systems(Update, update_focus_ability_ui);
        app.add_systems(
            OnExit(AppState::InGame),
            despawn_toolbar,
        );
    }
}

fn spawn_toolbar(
    mut commands: Commands,
    uiassets: Res<UiAssets>,
    menuassets: Res<MainMenuAssets>,
    mut q_cam: Query<(Entity, &GlobalTransform, &Camera), With<MainCamera>>,
) {
    println!("about to spawn toolbar");
    let Ok((cam_e, cam_pos, cam)) = q_cam.get_single() else {
        return;
    };
    // Use the UI builder with plain bundles and direct setting of bundle props
    let mut root_entity = Entity::PLACEHOLDER;
    println!("spawning toolbar");
    commands.ui_builder(UiRoot).container(
        (
            NodeBundle {
                style: Style {
                    width: Val::Percent(100.0),
                    height: Val::Percent(100.0),
                    flex_direction: FlexDirection::ColumnReverse,
                    ..default()
                },
                ..default()
            },
            TargetCamera(cam_e),
        ),
        |container| {},
    );
    // Make a bar centered 20px from the bottom of the screen
    let mut ability_bar = Entity::PLACEHOLDER;
    commands
        .ui_builder(UiRoot)
        .row(|row| {
            row.style()
                .position_type(PositionType::Absolute)
                .bottom(Val::Px(20.0))
                .justify_content(JustifyContent::Center);
            ability_bar = row
                .column(|column| {
                    //column.style().align_self(AlignSelf::Center);
                })
                .id();
        })
        .id();

    commands.ui_builder(ability_bar).row(|row| {
        row.ability_widget(AbilityWidgetConfig::from(
            "ui/game/AttackSkillIcon.png",
            AttackAbilityUI,
            true,
        ));
        row.ability_widget(AbilityWidgetConfig::from(
            "ui/game/DashSkillIcon.png",
            DashAbilityUI,
            true,
        ));
        row.ability_widget(AbilityWidgetConfig::from(
            "ui/game/WhirlSkillIcon.png",
            WhirlAbilityUI,
            false,
        ));
        row.ability_widget(AbilityWidgetConfig::from(
            "ui/game/FocusSkillIcon.png",
            FocusAbilityUI,
            true,
        ));
    });
}

fn despawn_toolbar() {}

#[derive(Component, Clone)]
pub struct AttackAbilityUI;
#[derive(Component, Clone)]
pub struct DashAbilityUI;
#[derive(Component, Clone)]
pub struct WhirlAbilityUI;
#[derive(Component, Clone)]
pub struct FocusAbilityUI;

fn update_attack_ability_ui(
    player: Query<(Option<&Attacking>, Option<&CanAttack>), With<Player>>,
    ui: Query<
        Entity,
        (
            With<AttackAbilityUI>,
            Without<AbilityWidget>,
        ),
    >,
    mut commands: Commands,
) {
    let Some((attack, can_attack)) = player.iter().next() else {
        return;
    };
    for (entity) in ui.iter() {
        let factor = if let Some(attack) = attack {
            1.0 - attack.ticks as f32 / (Attacking::MAX * 8) as f32
        } else if let Some(can_attack) = can_attack {
            0.0
        } else {
            0.0
        };
        commands.entity(entity).factor(factor);
    }
}

fn update_dash_ability_ui(
    player: Query<(&CanDash), With<Player>>,
    ui: Query<
        Entity,
        (
            With<DashAbilityUI>,
            Without<AbilityWidget>,
        ),
    >,
    config: Res<PlayerConfig>,
    mut commands: Commands,
) {
    let Some((can_dash)) = player.iter().next() else {
        return;
    };
    for (entity) in ui.iter() {
        let factor = can_dash.remaining_cooldown / config.dash_cooldown_duration;
        commands.entity(entity).factor(factor);
    }
}

fn update_whirl_ability_ui(
    player: Query<(&WhirlAbility), With<Player>>,
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
    let Some((whirl)) = player.iter().next() else {
        return;
    };
    for (entity) in ui.iter() {
        let factor = 1.0 - whirl.energy / config.max_whirl_energy;
        commands.entity(entity).factor(factor);
    }
}

fn update_focus_ability_ui(
    player: Query<(&FocusAbility), With<Player>>,
    focus_ui: Query<
        Entity,
        (
            With<FocusAbilityUI>,
            Without<AbilityWidget>,
        ),
    >,
    mut image_ui: Query<
        &mut BackgroundColor,
        (
            With<AbilityWidget>,
            With<FocusAbilityUI>,
        ),
    >,
    mut commands: Commands,
    time: Res<Time>,
) {
    let Some((focus)) = player.iter().next() else {
        return;
    };
    if focus.state == FocusState::InActive {
        for (entity) in focus_ui.iter() {
            let factor = 1.0 - focus.recharge / 10.0;
            commands.entity(entity).factor(factor);
        }
        for (mut bg) in image_ui.iter_mut() {
            bg.0 = Color::WHITE;
        }
    } else {
        for (mut bg) in image_ui.iter_mut() {
            // Makes the focus icon blink while focus is primed
            bg.0 =
                Color::WHITE * (1.1 + 0.2 * (time.elapsed_seconds_wrapped() * 10.0).sin().signum());
        }
    }
}
