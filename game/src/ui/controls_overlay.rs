use anyhow::Result;
use bevy::ecs::system::EntityCommands;
use bevy::prelude::*;
use bevy::utils;
use leafwing_input_manager::prelude::ActionState;

use super::{AppState, Spawn, StateDespawnMarker};
use crate::game::player::PlayerAction;

const OVERLAY_COLOR: Color = Color::rgba(0.08, 0.10, 0.06, 0.65);
const BACKGROUND_COLOR: Color = Color::rgb(0.22, 0.27, 0.18);
const ICON_BACKGROUND_COLOR: Color = Color::rgb(0.32, 0.37, 0.28);
const TEXT_COLOR: Color = Color::rgb(0.98, 0.99, 0.94);
const SPACER_COLOR: Color = Color::rgb(0.20, 0.25, 0.15);

pub(super) fn plugin(app: &mut App) {
    app.add_systems(
        OnEnter(AppState::InGame),
        (
            spawn_control_overlay,
            spawn_control_hint,
        ),
    )
    .add_systems(
        Update,
        toggle_control_overlay.map(utils::dbg),
    );
}

fn spawn_control_hint(mut commands: Commands) {
    commands.popup().with_children(|popup| {
        popup.row().with_children(|row| {
            row.text("Press ");
            row.control_icon("C");
            row.text(" to show Controls");
        });
    });
}

fn spawn_control_overlay(mut commands: Commands) {
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

#[derive(Component)]
struct ControlOverlayRoot;

trait ControlsOverlay {
    fn root(&mut self) -> EntityCommands;
    fn container(&mut self) -> EntityCommands;
    fn row(&mut self) -> EntityCommands;
    fn text(&mut self, string: impl Into<String>) -> EntityCommands;
    fn control_icon(&mut self, string: impl Into<String>) -> EntityCommands;
    fn spacer(&mut self) -> EntityCommands;
    fn popup(&mut self) -> EntityCommands;
}

impl<T: Spawn> ControlsOverlay for T {
    fn root(&mut self) -> EntityCommands {
        self.spawn((
            Name::new("controls_overlay_root"),
            ControlOverlayRoot,
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
                visibility: Visibility::Hidden,
                background_color: BackgroundColor(OVERLAY_COLOR),
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
                    padding: UiRect::axes(Val::Px(24.0), Val::Px(12.0)),
                    ..default()
                },
                background_color: BackgroundColor(BACKGROUND_COLOR),
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
                    color: TEXT_COLOR,
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
                background_color: BackgroundColor(ICON_BACKGROUND_COLOR),
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
                        color: TEXT_COLOR,
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
                    height: Val::Px(2.0),
                    width: Val::Percent(100.0),
                    margin: UiRect::vertical(Val::Px(7.0)),
                    ..default()
                },
                background_color: BackgroundColor(SPACER_COLOR),
                ..default()
            },
        ))
    }

    fn popup(&mut self) -> EntityCommands {
        self.spawn((
            Name::new("controls_popup"),
            NodeBundle {
                style: Style {
                    margin: UiRect::new(
                        Val::Auto,
                        Val::Auto,
                        Val::Percent(65.0),
                        Val::Auto,
                    ),
                    padding: UiRect::axes(Val::Px(16.0), Val::Px(4.0)),
                    ..default()
                },
                background_color: BackgroundColor(OVERLAY_COLOR),
                ..default()
            },
        ))
    }
}

fn toggle_control_overlay(
    action_state_q: Query<&ActionState<PlayerAction>>,
    mut control_overlay_q: Query<&mut Visibility, With<ControlOverlayRoot>>,
) -> Result<()> {
    let action_state = action_state_q.get_single()?;
    if action_state.just_pressed(&PlayerAction::ToggleControlOverlay) {
        for mut visibility in &mut control_overlay_q {
            *visibility = match *visibility {
                Visibility::Inherited | Visibility::Visible => {
                    Visibility::Hidden
                },
                Visibility::Hidden => Visibility::Visible,
            }
        }
    }

    Ok(())
}
