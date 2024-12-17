use bevy::ecs::system::EntityCommands;
use bevy::prelude::*;

use super::{AppState, Spawn, StateDespawnMarker};

pub(super) fn plugin(app: &mut App) {
    app.add_systems(OnEnter(AppState::InGame), setup);
}

fn setup(mut commands: Commands) {
    commands.root().with_children(|root| {
        root.container();
    });
}

trait ControlsOverlay {
    fn root(&mut self) -> EntityCommands;
    fn container(&mut self) -> EntityCommands;
}

impl<T: Spawn> ControlsOverlay for T {
    fn root(&mut self) -> EntityCommands {
        self.ui_spawn((
            Name::new("controls_overlay_root"),
            StateDespawnMarker,
            NodeBundle {
                style: Style {
                    display: Display::Flex,
                    position_type: PositionType::Absolute,
                    width: Val::Px(100.0),
                    height: Val::Px(100.0),
                    ..default()
                },
                background_color: BackgroundColor(Color::rgba(
                    0.0, 0.0, 0.0, 0.5,
                )),
                ..default()
            },
        ))
    }

    fn container(&mut self) -> EntityCommands {
        self.ui_spawn((
            Name::new("controls_overlay_container"),
            NodeBundle {
                style: Style {
                    display: Display::Flex,
                    width: Val::Px(200.0),
                    height: Val::Px(100.0),
                    ..default()
                },
                background_color: BackgroundColor(Color::rgb(1.0, 1.0, 1.0)),
                ..default()
            },
        ))
    }
}
