use bevy::{color::palettes::css::GRAY, render::camera::ClearColorConfig};
use iyes_progress::TrackedProgressSet;

use crate::prelude::*;

pub struct LoadscreenPlugin<S: States> {
    pub state: S,
}

impl<S: States> Plugin for LoadscreenPlugin<S> {
    fn build(&self, app: &mut App) {
        app.add_systems(
            OnEnter(self.state.clone()),
            setup_loadscreen,
        );
        app.add_systems(
            Last,
            update_loading_pct
                .after(TrackedProgressSet)
                .run_if(in_state(self.state.clone())),
        );
    }
}

#[derive(Component)]
struct LoadingProgressIndicator;

fn setup_loadscreen(mut commands: Commands) {
    commands.spawn((
        StateDespawnMarker,
        Camera2dBundle {
            camera: Camera {
                clear_color: ClearColorConfig::Custom(Color::BLACK),
                ..Default::default()
            },
            ..Default::default()
        },
    ));

    let container = commands
        .spawn((
            StateDespawnMarker,
            Node {
                width: Val::Auto,
                height: Val::Auto,
                position_type: PositionType::Absolute,
                bottom: Val::Percent(48.0),
                top: Val::Percent(48.0),
                left: Val::Percent(20.0),
                right: Val::Percent(20.0),
                padding: UiRect::all(Val::Px(2.0)),
                flex_direction: FlexDirection::Column,
                justify_content: JustifyContent::Center,
                ..Default::default()
            },
            BackgroundColor::from(GRAY),
        ))
        .id();

    let inner = commands
        .spawn((
            LoadingProgressIndicator,
            Node {
                width: Val::Percent(0.0),
                height: Val::Percent(100.0),
                ..Default::default()
            },
            BackgroundColor(Color::WHITE),
        ))
        .id();

    commands.entity(container).add_children(&[inner]);
}

fn update_loading_pct(
    mut q: Query<&mut Node, With<LoadingProgressIndicator>>,
    progress: Res<ProgressCounter>,
) {
    let progress: f32 = progress.progress().into();
    for mut node in q.iter_mut() {
        node.width = Val::Percent(progress * 100.0);
    }
}
