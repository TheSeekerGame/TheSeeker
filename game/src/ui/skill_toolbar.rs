use crate::appstate::{AppState, StateDespawnMarker};
use crate::assets::{MainMenuAssets, UiAssets};
use crate::camera::MainCamera;
use crate::game::attack::Health;
use crate::game::player::{CanDash, Dashing, Player, PlayerConfig};
use crate::graphics::hp_bar::HpBarUiMaterial;
use crate::prelude::*;
use crate::ui::ability_widget::{AbilityWidgetCommands, AbilityWidgetConfig, UiAbilityWidgetExt};
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
        app.add_systems(Update, update_dash_ability_ui);
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
        ));
        row.ability_widget(AbilityWidgetConfig::from(
            "ui/game/DashSkillIcon.png",
            DashAbilityUI,
        ));
        row.ability_widget(AbilityWidgetConfig::from(
            "ui/game/WhirlSkillIcon.png",
            WhirlAbilityUI,
        ));
        row.ability_widget(AbilityWidgetConfig::from(
            "ui/game/FocusSkillIcon.png",
            FocusAbilityUI,
        ));
    });
}

fn despawn_toolbar() {}

#[derive(Component)]
pub struct AttackAbilityUI;
#[derive(Component)]
pub struct DashAbilityUI;
#[derive(Component)]
pub struct WhirlAbilityUI;
#[derive(Component)]
pub struct FocusAbilityUI;

fn update_dash_ability_ui(
    player: Query<(&CanDash, Option<&Dashing>), With<Player>>,
    ui: Query<Entity, With<DashAbilityUI>>,
    config: Res<PlayerConfig>,
    mut commands: Commands,
) {
    let Ok((can_dash, dashing)) = player.get_single() else {
        return;
    };
    for (entity) in ui.iter() {
        let factor = can_dash.remaining_cooldown / config.dash_cooldown_duration;
        commands.entity(entity).factor(factor);
    }
}
