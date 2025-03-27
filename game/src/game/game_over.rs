use bevy::core::Name;
use bevy::hierarchy::BuildChildren;
use theseeker_engine::prelude::{
    in_state, resource_exists, Button, Color, GameTickUpdate, GameTime,
    Resource,
};

use crate::camera::MainCamera;
use crate::game::attack::KillCount;
use crate::game::player::PlayerStateSet;
use crate::gamestate::GameState;
use crate::locale::L10nKey;
use crate::prelude::*;

use super::pickups::DropTracker;

/// A plugin that handles when the player has a game over
pub struct GameOverPlugin;

impl Plugin for GameOverPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            GameTickUpdate,
            on_game_over
                .run_if(in_state(GameState::Playing))
                .after(PlayerStateSet::Transition)
                .run_if(in_state(AppState::InGame))
                .run_if(resource_exists::<GameOver>),
        );
        app.add_systems(Update, update_fade_in);
    }
}

#[derive(Component)]
#[component(storage = "SparseSet")]
pub struct FadeIn {
    progress: f32,
}

pub fn update_fade_in(
    mut commands: Commands,
    time: Res<Time>,
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

pub fn on_game_over(
    q_cam: Query<Entity, With<MainCamera>>,
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mut kill_count: ResMut<KillCount>,
    time: Res<GameTime>,
) {
    let Ok(cam_e) = q_cam.get_single() else {
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
        ))
        .with_children(|column| {
            let mut style = TextFont {
                font: asset_server.load("font/Tektur-Regular.ttf"),
                font_size: 42.0,
                ..Default::default()
            };

            column.spawn((Text::new("GAME OVER"), style.clone()));
            // Spacer
            column.spawn(Node {
                height: Val::Percent(10.0),
                ..default()
            });

            style.font_size = 24.0;
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
                    // TODO: Add behavior to buttons
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
                    ))
                    .with_child((
                        L10nKey("Abandon Planet?".to_owned()),
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
                    ))
                    .with_child((
                        L10nKey("New Expedition!".to_owned()),
                        Text::new("New Expedition!"),
                        style.clone(),
                    ));
                });
        });

    kill_count.0 = 0;

    // TODO: Move this to some less obscure system that resets game state.
    commands.insert_resource(DropTracker::default());
    commands.remove_resource::<GameOver>();
}
