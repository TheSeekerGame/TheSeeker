use crate::appstate::AppState;
use crate::camera::MainCamera;
use crate::game::player::{
    Attacking, CanAttack, CanDash, FocusAbility, FocusState, Player, PlayerConfig, WhirlAbility,
};
use crate::graphics::hp_bar::{HpBackground, HpBar, HpBarUiMaterial};
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
        app.add_systems(GameTickUpdate, assign_hp_bar);
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
    q_cam: Query<Entity, With<MainCamera>>,
    mut ui_materials: ResMut<Assets<HpBarUiMaterial>>,
) {
    let Ok(cam_e) = q_cam.get_single() else {
        return;
    };

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
        |_| {},
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
            row.named("ability_bar_ui");
            ability_bar = row.column(|_| {}).id();
        })
        .id();

    commands.ui_builder(ability_bar).row(|row| {
        // todo: after alpha, refactor this to be more generic; either a progress bar widget,
        //  or proper health bar widget
        row.container(
            (
                NodeBundle {
                    style: Style {
                        width: Val::Percent(100.0),
                        height: Val::Px(14.0),
                        padding: UiRect::all(Val::Px(1.0)),
                        ..default()
                    },
                    background_color: Color::rgb(0.75, 0.75, 0.75).into(),
                    ..default()
                },
                Name::new("hp_bg"),
                HpBackground(Entity::PLACEHOLDER),
            ),
            |parent| {
                parent.spawn((
                    MaterialNodeBundle {
                        style: Style {
                            width: Val::Percent(100.0),
                            height: Val::Percent(100.0),
                            align_self: AlignSelf::Center,
                            ..default()
                        },
                        material: ui_materials.add(HpBarUiMaterial {
                            factor: 1.0,
                            background_color: Color::rgb(0.15, 0.15, 0.15).into(),
                            filled_color: Color::rgb(0.8, 0.2, 0.2).into(),
                        }),
                        ..default()
                    },
                    HpBar(Entity::PLACEHOLDER),
                    Name::new("hp_bar"),
                    PlayerHpUI,
                ));
            },
        );
    });
    commands
        .ui_builder(ability_bar)
        .container(ImageBundle::default(), |row| {
            row.style()
                .image("ui/game/ToolbarFrame.png")
                .width(Val::Px(320.0))
                .height(Val::Px(80.0))
                .justify_content(JustifyContent::SpaceAround);
            //.justify_content(JustifyContent::SpaceAround);
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
    commands.ui_builder(ability_bar).row(|row| {
        // The xp bar; placeholder until xp exists
        row.container(
            (
                NodeBundle {
                    style: Style {
                        width: Val::Percent(100.0),
                        height: Val::Px(7.0),
                        padding: UiRect::all(Val::Px(1.0)),
                        ..default()
                    },
                    background_color: Color::rgb(0.75, 0.75, 0.75).into(),
                    ..default()
                },
                Name::new("xp_bg"),
            ),
            |parent| {
                parent.spawn((
                    MaterialNodeBundle {
                        style: Style {
                            width: Val::Percent(100.0),
                            height: Val::Percent(100.0),
                            align_self: AlignSelf::Center,
                            ..default()
                        },
                        material: ui_materials.add(HpBarUiMaterial {
                            factor: 1.0,
                            background_color: Color::rgb(0.15, 0.15, 0.15).into(),
                            filled_color: Color::rgb(0.1, 0.6, 0.1).into(),
                        }),
                        ..default()
                    },
                    Name::new("xp_bar"),
                ));
            },
        );
    });
}

fn despawn_toolbar() {}

fn assign_hp_bar(
    player: Query<Entity, Added<Player>>,
    mut hp_bar_q: Query<&mut HpBar, With<PlayerHpUI>>,
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
pub struct FocusAbilityUI;

fn update_attack_ability_ui(
    player: Query<(Option<&Attacking>, Has<CanAttack>), With<Player>>,
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
    for entity in ui.iter() {
        let factor = if let Some(attack) = attack {
            1.0 - attack.ticks as f32 / (Attacking::MAX * 8) as f32
        } else if can_attack {
            0.0
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
    config: Res<PlayerConfig>,
    mut commands: Commands,
) {
    let Some(can_dash) = player.iter().next() else {
        return;
    };
    for entity in ui.iter() {
        let factor = can_dash.remaining_cooldown / config.dash_cooldown_duration;
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

fn update_focus_ability_ui(
    player: Query<&FocusAbility, With<Player>>,
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
    let Some(focus) = player.iter().next() else {
        return;
    };
    if focus.state == FocusState::InActive {
        for entity in focus_ui.iter() {
            let factor = 1.0 - focus.recharge / 10.0;
            commands.entity(entity).factor(factor);
        }
        for mut bg in image_ui.iter_mut() {
            bg.0 = Color::WHITE;
        }
    } else {
        for mut bg in image_ui.iter_mut() {
            // Makes the focus icon blink while focus is primed
            bg.0 =
                Color::WHITE * (1.1 + 0.2 * (time.elapsed_seconds_wrapped() * 10.0).sin().signum());
        }
    }
}
