use bevy::core_pipeline::clear_color::ClearColorConfig;
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
            camera_2d: Camera2d {
                clear_color: ClearColorConfig::Custom(Color::BLACK),
            },
            ..Default::default()
        },
    ));

    let container = commands
        .spawn((
            StateDespawnMarker,
            NodeBundle {
                background_color: BackgroundColor(Color::GRAY),
                style: Style {
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
                ..Default::default()
            },
        ))
        .id();

    let inner = commands
        .spawn((
            LoadingProgressIndicator,
            NodeBundle {
                background_color: BackgroundColor(Color::WHITE),
                style: Style {
                    width: Val::Percent(0.0),
                    height: Val::Percent(100.0),
                    ..Default::default()
                },
                ..Default::default()
            },
        ))
        .id();

    commands.entity(container).push_children(&[inner]);
}

fn update_loading_pct(
    mut q: Query<&mut Style, With<LoadingProgressIndicator>>,
    progress: Res<ProgressCounter>,
) {
    let progress: f32 = progress.progress().into();
    for mut style in q.iter_mut() {
        style.width = Val::Percent(progress * 100.0);
    }
}
