use crate::appstate::AppState;
use crate::camera::MainCamera;
use crate::game::attack::KillCount;
use crate::graphics::hp_bar::HpBarUiMaterial;
use crate::prelude::*;
use crate::ui::ability_widget::UiAbilityWidgetExt;
use sickle_ui::ui_builder::{UiBuilderExt, UiRoot};
use sickle_ui::ui_commands::SetTextExt;
use sickle_ui::ui_style::*;
use sickle_ui::widgets::prelude::*;

pub struct KillCounterPlugin;

impl Plugin for KillCounterPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            OnEnter(AppState::InGame),
            spawn_killcounter.after(crate::camera::setup_main_camera),
        );
        app.add_systems(GameTickUpdate, update_counter);
    }
}

#[derive(Component)]
pub struct KillCounterUi;

fn spawn_killcounter(
    mut commands: Commands,
    q_cam: Query<Entity, With<MainCamera>>,
    asset_server: Res<AssetServer>,
) {
    let Ok(cam_e) = q_cam.get_single() else {
        return;
    };

    commands.ui_builder(UiRoot).container(
        (
            NodeBundle {
                style: Style {
                    position_type: PositionType::Absolute,
                    flex_direction: FlexDirection::Row,
                    right: Val::Px(20.0),
                    top: Val::Px(20.0),
                    ..default()
                },
                ..default()
            },
            TargetCamera(cam_e),
            StateDespawnMarker,
        ),
        |row| {
            let style = TextStyle {
                font: asset_server.load("font/Tektur-Regular.ttf"),
                font_size: 32.0,
                color: Default::default(),
            };
            row.spawn(TextBundle::from_section(
                format!("Kills: "),
                style.clone(),
            ));
            row.spawn((
                KillCounterUi,
                TextBundle::from_section(format!("_"), style.clone()),
            ));
        },
    );
}

pub fn update_counter(
    mut commands: Commands,
    kill_count: Res<KillCount>,
    query: Query<(Entity, &Text), With<KillCounterUi>>,
) {
    for (entity, text) in query.iter() {
        commands.entity(entity).set_text(
            format!("{}", kill_count.0),
            Some(text.sections[0].style.clone()),
        );
    }
}
