use anyhow::Result;
use bevy::prelude::*;
use bevy::utils;
use leafwing_input_manager::prelude::ActionState;

use super::popup::PopupTimer;
use super::popup::PopupUi;
use super::AppState;
use crate::game::player::PlayerAction;

#[derive(Component)]
struct ControlsOverlay;

#[derive(Component)]
struct ControlsHint;

pub(super) fn plugin(app: &mut App) {
    app.add_systems(
        OnEnter(AppState::InGame),
        spawn_control_overlay,
    )
    .add_systems(
        OnTransition {
            from: AppState::MainMenu,
            to: AppState::InGame,
        },
        spawn_control_hint,
    )
    .add_systems(
        Update,
        (
            toggle_control_overlay.map(utils::dbg),
            hide_controls_hint.map(utils::dbg),
        ),
    );
}

fn spawn_control_hint(mut commands: Commands) {
    commands
        .popup()
        .insert((ControlsHint, PopupTimer::default()))
        .with_children(|popup| {
            popup.row().with_children(|row| {
                row.text("Press ");
                row.control_icon("C");
                row.text(" to show Controls");
            });
        });
}

fn spawn_control_overlay(mut commands: Commands) {
    commands
        .root()
        .insert(ControlsOverlay)
        .with_children(|root| {
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
                    row.text("Swap Melee/Ranged: ");
                    row.control_icon("H");
                    row.text(" or ");
                    row.control_icon("`");
                });
                container.row().with_children(|row| {
                    row.text("Swap Melee Weapon: ");
                    row.control_icon("G");
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

fn toggle_control_overlay(
    action_state_q: Query<&ActionState<PlayerAction>>,
    mut control_overlay_q: Query<&mut Visibility, With<ControlsOverlay>>,
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

fn hide_controls_hint(
    action_state_q: Query<&ActionState<PlayerAction>>,
    control_overlay_q: Query<Entity, With<ControlsHint>>,
    mut commands: Commands,
) -> Result<()> {
    let action_state = action_state_q.get_single()?;

    for entity in &control_overlay_q {
        if action_state.just_pressed(&PlayerAction::ToggleControlOverlay) {
            commands.entity(entity).despawn_recursive();
        }
    }

    Ok(())
}
