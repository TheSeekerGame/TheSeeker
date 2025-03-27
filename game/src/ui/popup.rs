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
            Node {
                position_type: PositionType::Absolute,
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                ..default()
            },
            Visibility::Hidden,
            BackgroundColor(OVERLAY_COLOR),
        ))
    }

    fn container(&mut self) -> EntityCommands {
        self.spawn((
            Name::new("popup_container"),
            Node {
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
            BackgroundColor(BACKGROUND_COLOR),
        ))
    }

    fn row(&mut self) -> EntityCommands {
        self.spawn((
            Name::new("popup_row"),
            Node {
                display: Display::Flex,
                width: Val::Percent(100.0),
                height: Val::Auto,
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Start,
                padding: UiRect::vertical(Val::Px(2.0)),
                ..default()
            },
        ))
    }

    fn text(&mut self, text: impl Into<String>) -> EntityCommands {
        self.spawn((
            Name::new("popup_text"),
            Text::new(text),
            TextFont::from_font_size(14.0),
            TextColor(TEXT_COLOR),
        ))
    }

    fn control_icon(&mut self, text: impl Into<String>) -> EntityCommands {
        let mut entity = self.spawn((
            Name::new("popup_icon"),
            Node {
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                padding: UiRect::all(Val::Px(4.0)),
                min_width: Val::Px(20.0),
                ..default()
            },
            BackgroundColor(ICON_BACKGROUND_COLOR),
        ));

        entity.with_children(|mut node| {
            // TODO: Is this disambiguation correct?
            crate::ui::Spawn::spawn(
                node,
                (
                    Name::new("popup_icon_text"),
                    Text::new(text),
                    TextFont::from_font_size(14.0),
                    TextColor(TEXT_COLOR),
                ),
            );
        });

        entity
    }

    fn spacer(&mut self) -> EntityCommands {
        self.spawn((
            Name::new("popup_spacer"),
            Node {
                height: Val::Px(2.0),
                width: Val::Percent(100.0),
                margin: UiRect::vertical(Val::Px(7.0)),
                ..default()
            },
            BackgroundColor(SPACER_COLOR),
        ))
    }

    fn popup(&mut self) -> EntityCommands {
        self.spawn((
            Name::new("popup"),
            Popup,
            Node {
                margin: UiRect::new(
                    Val::Auto,
                    Val::Auto,
                    Val::Percent(40.0),
                    Val::Auto,
                ),
                padding: UiRect::axes(Val::Px(16.0), Val::Px(4.0)),
                ..default()
            },
            BackgroundColor(BACKGROUND_COLOR),
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
