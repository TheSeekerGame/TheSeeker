use sickle_ui::ui_builder::{UiBuilderExt, UiRoot};
use sickle_ui::ui_style::*;
use sickle_ui::widgets::prelude::*;

use crate::appstate::AppState;
use crate::game::player::{
    Attacking, CanDash, CanStealth, Player, PlayerConfig, WhirlAbility,
};
use crate::graphics::player_hp;
use crate::prelude::*;
use crate::ui::ability_widget::{
    AbilityWidget, AbilityWidgetCommands, AbilityWidgetConfig,
    UiAbilityWidgetExt,
};

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
) {
    let mut ability_bar = Entity::PLACEHOLDER;
    commands
        .ui_builder(UiRoot)
        .row(|row| {
            row.style()
                .align_self(AlignSelf::End)
                .justify_self(JustifySelf::Center)
                .justify_content(JustifyContent::Center);
            row.named("ability_bar_ui");
            ability_bar = row.column(|_| {}).id();
        })
        .insert(StateDespawnMarker)
        .id();

    commands.ui_builder(ability_bar).row(|row| {
        // todo: after alpha, refactor this to be more generic; either a progress bar or a health bar widget
        row.container(
            (
                NodeBundle {
                    style: Style {
                        width: Val::Px(270.0),
                        height: Val::Px(10.0),
                        // TODO: Change this to relative positioning later
                        position_type: PositionType::Absolute,
                        bottom: Val::Px(-125.0),
                        left: Val::Px(45.0),
                        ..default()
                    },
                    background_color: Color::rgb(0.0, 0.0, 0.0).into(),
                    ..default()
                },
                Name::new("hp_bg"),
            ),
            |parent| {
                parent.spawn((
                    MaterialNodeBundle {
                        style: Style {
                            width: Val::Percent(100.0),
                            height: Val::Percent(100.0),
                            ..default()
                        },
                        material: ui_materials.add(player_hp::Material {
                            factor: 1.0,
                            background_color: Color::rgb(
                                0.23137254901960785,
                                0.12549019607843137,
                                0.12549019607843137,
                            ),
                            filled_color: Color::rgb(
                                0.6352941176470588,
                                0.196078431372549022,
                                0.3058823529411765,
                            ),
                        }),
                        ..default()
                    },
                    player_hp::Bar(Entity::PLACEHOLDER),
                    Name::new("hp_bar"),
                    PlayerHpUI,
                ));
            },
        );
    });
    commands.ui_builder(ability_bar).container(
        ImageBundle {
            style: Style {
                justify_content: JustifyContent::Center,
                ..Default::default()
            },
            ..Default::default()
        },
        |row| {
            row.style()
                .image("ui/game/ToolbarFrame.png")
                .width(Val::Px(360.0))
                .height(Val::Px(150.0))
                .column_gap(Val::Px(10.0));
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
                "ui/game/StealthSkillIcon.png",
                StealthAbilityUI,
                true,
            ));
        },
    );
    commands.ui_builder(ability_bar).row(|row| {
        // The xp bar; placeholder until xp exists
        row.container(
            (
                NodeBundle {
                    style: Style {
                        width: Val::Px(270.0),
                        height: Val::Px(5.0),
                        // TODO: Change this to relative positioning later
                        position_type: PositionType::Absolute,
                        bottom: Val::Px(15.0),
                        left: Val::Px(45.0),
                        ..default()
                    },
                    background_color: Color::rgb(0.0, 0.0, 0.0).into(),
                    z_index: ZIndex::Global(-1),
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
                            ..default()
                        },
                        material: ui_materials.add(player_hp::Material {
                            factor: 1.0,
                            background_color: Color::rgb(
                                0.17647058823529413,
                                0.15294117647058825,
                                0.22745098039215686,
                            ),
                            filled_color: Color::rgb(
                                0.24705882352941178,
                                0.3137254901960784,
                                0.43137254901960786,
                            ),
                        }),
                        ..default()
                    },
                    Name::new("xp_bar"),
                ));
            },
        );
    });
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
