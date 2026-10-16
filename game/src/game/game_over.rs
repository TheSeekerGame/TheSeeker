use bevy::prelude::{Name, UiTargetCamera as TargetCamera};
use theseeker_engine::prelude::{
    in_state, resource_added, resource_exists, Button, Color,
    GameTickUpdate, GameTime, Resource,
};

use crate::camera::{MainCamera, SnapCameraNext};
use crate::game::combat::KillCount;
use crate::game::player::PlayerStateSet;
use crate::gamestate::GameState;
use crate::prelude::*;

/// A plugin that handles when the player has a game over
pub struct GameOverPlugin;

impl Plugin for GameOverPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            GameTickUpdate,
            on_game_over
                .run_if(in_state(GameState::Playing))
                .after(PlayerStateSet::Animation)
                .run_if(in_state(AppState::InGame))
                .run_if(resource_added::<GameOver>),
        );
        app.add_systems(
            Update,
            (
                update_fade_in,
                handle_game_over_buttons
                    .run_if(in_state(AppState::InGame))
                    .run_if(resource_exists::<GameOver>),
            ),
        );
    }
}

/// Fades the screen to a target opacity using master clock seconds.
#[derive(Component)]
#[component(storage = "SparseSet")]
pub struct FadeIn {
    progress: f32,
}

pub fn update_fade_in(
    mut commands: Commands,
    time: Res<Time<Real>>,
    mut query: Query<(
        Entity,
        &mut BackgroundColor,
        &mut FadeIn,
    )>,
) {
    for (entity, mut bg_color, mut fade_in) in query.iter_mut() {
        fade_in.progress += time.delta_secs() * 0.5;
        fade_in.progress = fade_in.progress.clamp(0.0, 1.0);
        bg_color.0.set_alpha(fade_in.progress * 0.77); // Max alpha of 0.77
        if fade_in.progress >= 1.0 {
            commands.entity(entity).remove::<FadeIn>();
        }
    }
}

/// Inserted on player despawn
#[derive(Resource)]
pub struct GameOver;

/// Marker for entities belonging to the game over UI so they can be cleaned.
#[derive(Component, Default)]
struct GameOverUi;

#[derive(Component, Copy, Clone, Eq, PartialEq)]
enum GameOverButtonAction {
    AbandonPlanet,
    NewExpedition,
}

#[expect(
    clippy::too_many_lines,
    reason = "This Bevy transition system builds the complete game-over scene and resets run counters as one UI flow."
)]
pub fn on_game_over(
    q_cam: Query<Entity, With<MainCamera>>,
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mut kill_count: ResMut<KillCount>,
    time: Res<GameTime>,
) {
    let Ok(cam_e) = q_cam.single() else {
        return;
    };

    commands.spawn((
        Node {
            width: Val::Percent(100.0),
            height: Val::Percent(100.0),
            flex_direction: FlexDirection::Column,
            ..default()
        },
        BackgroundColor(Color::srgba_u8(0, 0, 0, 0)),
        // This ensures we draw the ui above all other uis
        GlobalZIndex(i32::MAX - 1000),
        FadeIn { progress: 0.0 },
        TargetCamera(cam_e),
        StateDespawnMarker,
        GameOverUi,
    ));

    commands
        .spawn((
            Name::new("Game Over UI"),
            Node {
                width: Val::Percent(100.0),
                height: Val::Percent(100.),
                flex_direction: FlexDirection::Column,
                position_type: PositionType::Absolute,
                top: Val::Percent(20.0),
                justify_content: JustifyContent::FlexStart,
                align_items: AlignItems::Center,

                ..default()
            },
            GlobalZIndex(i32::MAX - 999),
            StateDespawnMarker,
            GameOverUi,
        ))
        .with_children(|column| {
            let mut style = TextFont {
                font: (asset_server.load("font/Tektur-Regular.ttf")).into(),
                font_size: FontSize::Px(42.0),
                ..Default::default()
            };

            column.spawn((Text::new("GAME OVER"), style.clone()));
            // Spacer
            column.spawn(Node {
                height: Val::Percent(10.0),
                ..default()
            });

            style.font_size = FontSize::Px(24.0);
            column.spawn((
                Text::new("You were killed by an Ice Crawler"),
                style.clone(),
            ));
            column.spawn((
                Text::new(format!("Kills: {}", kill_count.0)),
                style.clone(),
            ));
            let score = (kill_count.0 as f64 / time.time_in_seconds()) * 100.0;
            column.spawn((
                Text::new(format!("Score: {}", score as u32)),
                style.clone(),
            ));

            column.spawn(Node {
                height: Val::Percent(10.0),
                ..default()
            });

            column
                .spawn(Node {
                    width: Val::Percent(100.),
                    align_items: AlignItems::Center,
                    justify_content: JustifyContent::Center,
                    ..default()
                })
                .with_children(|row| {
                    // Buttons are non-interactive; behavior not implemented yet
                    row.spawn((
                        Button,
                        Node {
                            justify_content: JustifyContent::Center,
                            align_items: AlignItems::Center,
                            padding: UiRect::all(Val::Px(4.0)),
                            margin: UiRect::all(Val::Px(4.0)),
                            ..Default::default()
                        },
                        BackgroundColor(Color::NONE),
                        GameOverButtonAction::AbandonPlanet,
                        GameOverUi,
                    ))
                    .with_child((
                        Text::new("Abandon Planet?"),
                        style.clone(),
                    ));

                    row.spawn((Text::new("|"), style.clone()));

                    row.spawn((
                        Button,
                        Node {
                            justify_content: JustifyContent::Center,
                            align_items: AlignItems::Center,
                            padding: UiRect::all(Val::Px(4.0)),
                            margin: UiRect::all(Val::Px(4.0)),
                            ..Default::default()
                        },
                        BackgroundColor(Color::NONE),
                        GameOverButtonAction::NewExpedition,
                        GameOverUi,
                    ))
                    .with_child((
                        Text::new("New Expedition!"),
                        style.clone(),
                    ));
                });
        });

    kill_count.0 = 0;
}

fn handle_game_over_buttons(
    mut commands: Commands,
    mut interactions: Query<
        (
            &Interaction,
            &GameOverButtonAction,
            &mut BackgroundColor,
        ),
        (Changed<Interaction>, With<Button>),
    >,
    mut next_app_state: ResMut<NextState<AppState>>,
    mut next_game_state: ResMut<NextState<GameState>>,
    q_ui: Query<Entity, With<GameOverUi>>,
) {
    for (interaction, action, mut background) in interactions.iter_mut() {
        match *interaction {
            Interaction::Pressed => {
                background.0 = Color::srgba(0.2, 0.2, 0.25, 0.9);
                handle_pressed_game_over_button(
                    action,
                    &mut commands,
                    &q_ui,
                    &mut next_app_state,
                    &mut next_game_state,
                );
            },
            Interaction::Hovered => {
                background.0 = Color::srgba(0.15, 0.15, 0.2, 0.9);
            },
            Interaction::None => {
                background.0 = Color::srgba(0.08, 0.08, 0.1, 0.85);
            },
        }
    }
}

fn handle_pressed_game_over_button(
    action: &GameOverButtonAction,
    commands: &mut Commands,
    q_ui: &Query<Entity, With<GameOverUi>>,
    next_app_state: &mut NextState<AppState>,
    next_game_state: &mut NextState<GameState>,
) {
    despawn_game_over_ui(commands, q_ui);
    commands.remove_resource::<GameOver>();

    match action {
        GameOverButtonAction::AbandonPlanet => {
            next_app_state.set(AppState::MainMenu);
        },
        GameOverButtonAction::NewExpedition => {
            commands.insert_resource(SnapCameraNext);
            next_game_state.set(GameState::Playing);
        },
    }
}

fn despawn_game_over_ui(
    commands: &mut Commands,
    q_ui: &Query<Entity, With<GameOverUi>>,
) {
    for entity in q_ui.iter() {
        commands.entity(entity).despawn();
    }
}
