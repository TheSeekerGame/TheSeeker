use bevy::ecs::system::EntityCommands;
use bevy::prelude::*;

use super::{Spawn, StateDespawnMarker};

const OVERLAY_COLOR: Color = Color::rgba(0.08, 0.10, 0.06, 0.65);
const BACKGROUND_COLOR: Color = Color::rgba(0.0, 0.0, 0.0, 0.8);
const ICON_BACKGROUND_COLOR: Color = Color::rgba(0.32, 0.37, 0.28, 1.0);
const TEXT_COLOR: Color = Color::rgb(0.98, 0.99, 0.94);
const SPACER_COLOR: Color = Color::rgb(0.20, 0.25, 0.15);
const POPUP_DURATION_SECS: f32 = 5.0;

pub(super) fn plugin(app: &mut App) {
    app.add_systems(
        Update,
        despawn_popup_on_timer.run_if(any_with_component::<PopupTimer>),
    );
}

#[derive(Component)]
struct Popup;

#[derive(Component, Deref, DerefMut)]
pub struct PopupTimer(Timer);

impl PopupTimer {
    pub fn from_secs(secs: f32) -> Self {
        Self(Timer::from_seconds(
            secs,
            TimerMode::Once,
        ))
    }
}

impl Default for PopupTimer {
    fn default() -> Self {
        Self::from_secs(POPUP_DURATION_SECS)
    }
}

pub trait PopupUi {
    fn root(&mut self) -> EntityCommands;
    fn container(&mut self) -> EntityCommands;
    fn row(&mut self) -> EntityCommands;
    fn text(&mut self, string: impl Into<String>) -> EntityCommands;
    fn control_icon(&mut self, string: impl Into<String>) -> EntityCommands;
    fn spacer(&mut self) -> EntityCommands;
    fn popup(&mut self) -> EntityCommands;
}

impl<T: Spawn> PopupUi for T {
    fn root(&mut self) -> EntityCommands {
        self.spawn((
            Name::new("popup_root"),
            StateDespawnMarker,
            NodeBundle {
                style: Style {
                    position_type: PositionType::Absolute,
                    width: Val::Percent(100.0),
                    height: Val::Percent(100.0),
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
            Name::new("popup_container"),
            NodeBundle {
                style: Style {
                    position_type: PositionType::Absolute,
                    margin: UiRect {
                        left: Val::Percent(25.0),
                        right: Val::Auto,
                        top: Val::Auto,
                        bottom: Val::Auto,
                    },
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
            Name::new("popup_row"),
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
            Name::new("popup_text"),
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
            Name::new("popup_icon"),
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
                Name::new("popup_icon_text"),
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
            Name::new("popup_spacer"),
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
            Name::new("popup"),
            Popup,
            NodeBundle {
                style: Style {
                    margin: UiRect::new(
                        Val::Auto,
                        Val::Auto,
                        Val::Percent(40.0),
                        Val::Auto,
                    ),
                    padding: UiRect::axes(Val::Px(16.0), Val::Px(4.0)),
                    ..default()
                },
                background_color: BackgroundColor(BACKGROUND_COLOR),
                ..default()
            },
            StateDespawnMarker,
        ))
    }
}

fn despawn_popup_on_timer(
    mut popup_q: Query<(Entity, &mut PopupTimer), With<Popup>>,
    time: Res<Time>,
    mut commands: Commands,
) {
    for (entity, mut timer) in &mut popup_q {
        if timer.tick(time.delta()).finished() {
            commands.entity(entity).despawn_recursive();
        }
    }
}
