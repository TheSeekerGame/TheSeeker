use crate::appstate::AppState;
use crate::camera::MainCamera;
use crate::game::attack::KillCount;
use crate::prelude::*;

use super::popup::PopupUi;

pub struct KillCounterPlugin;

impl Plugin for KillCounterPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            OnEnter(AppState::InGame),
            spawn_killcounter.after(crate::camera::setup_main_camera),
        );
        app.add_systems(
            GameTickUpdate,
            update_counter.run_if(resource_changed::<KillCount>),
        );
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

    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                flex_direction: FlexDirection::Row,
                right: Val::Px(20.0),
                top: Val::Px(20.0),
                ..default()
            },
            TargetCamera(cam_e),
            StateDespawnMarker,
        ))
        .with_children(|row| {
            let font = TextFont {
                font: asset_server.load("font/Tektur-Regular.ttf"),
                font_size: 32.0,
                ..Default::default()
            };
            row.spawn((Text::new("Kills: "), font.clone()));
            row.spawn((KillCounterUi, Text::new("_"), font));
        });
}

pub fn update_counter(
    mut commands: Commands,
    mut query: Query<(Entity, &mut Text), With<KillCounterUi>>,
    kill_count: Res<KillCount>,
) {
    for (entity, mut text) in query.iter_mut() {
        text.0 = format!("{}", kill_count.0);
    }
}
