use bevy::ecs::system::EntityCommands;
use bevy::prelude::*;

use super::{AppState, Spawn, StateDespawnMarker};

pub(super) fn plugin(app: &mut App) {
    app.add_systems(OnEnter(AppState::InGame), setup);
}

fn setup(mut commands: Commands) {
    commands.root().with_children(|root| {
        root.container().with_children(|container| {
            container.row().with_children(|row| {
                row.text("Move Left: ");
                row.control_icon("A");
                row.text(" or ");
                row.control_icon("[<]");
            });
            container.row().with_children(|row| {
                row.text("Move Right: ");
                row.control_icon("D");
                row.text(" or ");
                row.control_icon("[>]");
            });
            container.row().with_children(|row| {
                row.text("Jump: ");
                row.control_icon("W");
                row.text(" or ");
                row.control_icon("SPACE");
                row.text(" or ");
                row.control_icon("[^]");
            });
            container.spacer();
            container.row().with_children(|row| {
                row.text("Attack: ");
                row.control_icon("J");
                row.text(" or ");
                row.control_icon("1");
            });
            container.row().with_children(|row| {
                row.text("Dash: ");
                row.control_icon("K");
                row.text(" or ");
                row.control_icon("2");
            });
            container.row().with_children(|row| {
                row.text("Whirl: ");
                row.control_icon("L");
                row.text(" or ");
                row.control_icon("3");
            });
            container.row().with_children(|row| {
                row.text("Stealth: ");
                row.control_icon(";");
                row.text(" or ");
                row.control_icon("4");
            });
            container.row().with_children(|row| {
                row.text("Swap Weapon: ");
                row.control_icon("H");
                row.text(" or ");
                row.control_icon("`");
            });
            container.spacer();
            container.row().with_children(|row| {
                row.text("Interact: ");
                row.control_icon("F");
            });
            container.spacer();
            container.row().with_children(|row| {
                row.text("Show/Hide Controls: ");
                row.control_icon("C");
            });
        });
    });
}

trait ControlsOverlay {
    fn root(&mut self) -> EntityCommands;
    fn container(&mut self) -> EntityCommands;
    fn row(&mut self) -> EntityCommands;
    fn text(&mut self, string: impl Into<String>) -> EntityCommands;
    fn control_icon(&mut self, string: impl Into<String>) -> EntityCommands;
    fn spacer(&mut self) -> EntityCommands;
}

impl<T: Spawn> ControlsOverlay for T {
    fn root(&mut self) -> EntityCommands {
        self.spawn((
            Name::new("controls_overlay_root"),
            StateDespawnMarker,
            NodeBundle {
                style: Style {
                    display: Display::Flex,
                    position_type: PositionType::Absolute,
                    width: Val::Percent(100.0),
                    height: Val::Percent(100.0),
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
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
        self.spawn((
            Name::new("controls_overlay_container"),
            NodeBundle {
                style: Style {
                    display: Display::Flex,
                    flex_direction: FlexDirection::Column,
                    padding: UiRect::all(Val::Px(12.0)),
                    ..default()
                },
                background_color: BackgroundColor(Color::rgb(0.3, 0.32, 0.28)),
                ..default()
            },
        ))
    }

    fn row(&mut self) -> EntityCommands {
        self.spawn((
            Name::new("controls_overlay_row"),
            NodeBundle {
                style: Style {
                    display: Display::Flex,
                    width: Val::Percent(100.0),
                    height: Val::Auto,
                    align_items: AlignItems::Center,
                    justify_content: JustifyContent::Start,
                    padding: UiRect::vertical(Val::Px(2.0)),
                    ..default()
                },
                ..default()
            },
        ))
    }

    fn text(&mut self, value: impl Into<String>) -> EntityCommands {
        self.spawn((
            Name::new("controls_overlay_text"),
            TextBundle::from_section(
                value,
                TextStyle {
                    font_size: 14.0,
                    color: Color::rgb(1.0, 1.0, 1.0),
                    ..default()
                },
            ),
        ))
    }

    fn control_icon(&mut self, value: impl Into<String>) -> EntityCommands {
        let mut entity = self.spawn((
            Name::new("controls_overlay_icon"),
            NodeBundle {
                style: Style {
                    align_items: AlignItems::Center,
                    justify_content: JustifyContent::Center,
                    padding: UiRect::all(Val::Px(4.0)),
                    min_width: Val::Px(20.0),
                    ..default()
                },
                background_color: BackgroundColor(Color::rgb(0.2, 0.2, 0.2)),
                ..default()
            },
        ));

        entity.with_children(|node| {
            node.spawn((
                Name::new("controls_overlay_icon_text"),
                TextBundle::from_section(
                    value,
                    TextStyle {
                        font_size: 14.0,
                        color: Color::rgb(1.0, 1.0, 1.0),
                        ..default()
                    },
                ),
            ));
        });

        entity
    }

    fn spacer(&mut self) -> EntityCommands {
        self.spawn((
            Name::new("controls_overlay_spacer"),
            NodeBundle {
                style: Style {
                    height: Val::Px(1.0),
                    width: Val::Percent(100.0),
                    margin: UiRect::all(Val::Px(2.0)),
                    ..default()
                },
                background_color: BackgroundColor(Color::rgb(0.2, 0.1, 0.15)),
                ..default()
            },
        ))
    }
}
