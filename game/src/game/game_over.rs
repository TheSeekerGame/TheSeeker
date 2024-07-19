use crate::camera::MainCamera;
use crate::game::gentstate::Dead;
use crate::game::player::{Player, PlayerStateSet};
use crate::gamestate::GameState;
use crate::prelude::{
    default, AlignItems, App, AppState, AssetServer, BackgroundColor, Commands, Component, Entity,
    FlexDirection, Has, IntoSystemConfigs, JustifyContent, NodeBundle, Plugin, PositionType, Query,
    Res, Style, TargetCamera, Text, TextBundle, TextStyle, Time, UiRect, Update, Val, With,
};
use sickle_ui::ui_builder::{UiBuilderExt, UiRoot};
use sickle_ui::ui_style::{
    SetNodeAlignItemsExt, SetNodeBottomExt, SetNodeJustifyContentsExt, SetNodePositionTypeExt,
    SetNodeTopExt, SetNodeWidthExt,
};
use sickle_ui::widgets::prelude::*;
use theseeker_engine::gent::Gent;
use theseeker_engine::prelude::{in_state, Color, GameTickUpdate};

/// A plugin that handles when the player has a game over
pub struct GameOverPlugin;

impl Plugin for GameOverPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            GameTickUpdate,
            on_game_over
                .run_if(in_state(GameState::Playing))
                .before(PlayerStateSet::Transition)
                .run_if(in_state(AppState::InGame)),
        );
        app.add_systems(Update, update_fade_in);
    }
}

#[derive(Component)]
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
        fade_in.progress += time.delta_seconds() * 0.5;
        fade_in.progress = fade_in.progress.clamp(0.0, 1.0);
        bg_color.0.set_a(fade_in.progress * 0.77); // Max alpha of 0.77
        if fade_in.progress >= 1.0 {
            commands.entity(entity).remove::<FadeIn>();
        }
    }
}

pub fn on_game_over(
    query: Query<(Entity, &Gent, Has<Dead>), (With<Player>)>,
    q_cam: Query<Entity, With<MainCamera>>,
    mut commands: Commands,
    asset_server: Res<AssetServer>,
) {
    let Ok(cam_e) = q_cam.get_single() else {
        return;
    };
    if let Some((e, g, is_dead)) = query.iter().next() {
        if !is_dead {
            return;
        }
    } else {
        return;
    };

    commands.ui_builder(UiRoot).container(
        (
            NodeBundle {
                style: Style {
                    width: Val::Percent(100.0),
                    height: Val::Percent(100.0),
                    flex_direction: FlexDirection::Column,
                    ..default()
                },
                background_color: BackgroundColor(Color::Rgba {
                    red: 0.0,
                    green: 0.0,
                    blue: 0.0,
                    alpha: 0.0,
                }),
                ..default()
            },
            FadeIn { progress: 0.0 },
            TargetCamera(cam_e),
        ),
        |_| {},
    );

    commands.ui_builder(UiRoot).column(|column| {
        column
            .style()
            .position_type(PositionType::Absolute)
            .top(Val::Percent(20.0))
            .justify_content(JustifyContent::FlexStart)
            .align_items(AlignItems::Center)
            .width(Val::Percent(100.0));
        column.named("Game Over UI");

        let base_style = TextStyle {
            font: asset_server.load("font/Tektur-Regular.ttf"),
            font_size: 42.0,
            color: Default::default(),
        };

        column.spawn(TextBundle::from_section(
            "GAME OVER",
            base_style.clone(),
        ));
        // Spacer
        column.spawn(NodeBundle {
            style: Style {
                height: Val::Px(100.0),
                ..default()
            },
            ..default()
        });
        column.spawn(TextBundle::from_section(
            "You were killed by an Ice Crawler",
            TextStyle {
                font_size: 24.0,
                ..base_style
            },
        ));
    });
}
