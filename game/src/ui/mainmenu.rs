use super::spawn_menuentry;
use crate::assets::{MainMenuAssets, UiAssets};
use crate::prelude::*;

pub struct MainMenuPlugin;

impl Plugin for MainMenuPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            OnEnter(AppState::MainMenu),
            spawn_mainmenu,
        );
    }
}

fn spawn_mainmenu(
    mut commands: Commands,
    uiassets: Res<UiAssets>,
    menuassets: Res<MainMenuAssets>,
) {
    commands.spawn((
        Camera2dBundle::default(),
        StateDespawnMarker,
    ));

    let e_menu_root = commands
        .spawn((
            StateDespawnMarker,
            NodeBundle {
                background_color: BackgroundColor(Color::rgb(0.0, 0.0, 0.0)),
                style: Style {
                    position_type: PositionType::Absolute,
                    left: Val::Px(0.),
                    right: Val::Px(0.),
                    top: Val::Px(0.),
                    bottom: Val::Px(0.),
                    flex_direction: FlexDirection::Column,
                    align_items: AlignItems::Center,
                    justify_content: JustifyContent::SpaceEvenly,
                    ..Default::default()
                },
                ..Default::default()
            },
        ))
        .id();
    let e_logo_image = commands
        .spawn((ImageBundle {
            style: Style {
                ..Default::default()
            },
            image: UiImage {
                texture: menuassets.background.clone(),
                ..Default::default()
            },
            ..Default::default()
        },))
        .id();
    let e_menu_wrapper = commands
        .spawn((NodeBundle {
            style: Style {
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                ..Default::default()
            },
            ..Default::default()
        },))
        .id();

    let e_butt_play = spawn_menuentry(
        &mut commands,
        &uiassets,
        OnClick::new().cli("AppState InGame"),
        "mainmenu-entry-play",
    );
    let e_butt_exit = spawn_menuentry(
        &mut commands,
        &uiassets,
        OnClick::new().cli("exit"),
        "mainmenu-entry-exit",
    );

    commands
        .entity(e_menu_root)
        .push_children(&[e_logo_image, e_menu_wrapper]);
    commands.entity(e_menu_wrapper).push_children(&[
        e_butt_play,
        e_butt_exit,
    ]);
}
