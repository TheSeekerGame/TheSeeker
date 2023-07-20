use bevy::core_pipeline::clear_color::ClearColorConfig;

use crate::prelude::*;

pub struct LoadscreenPlugin<S: States> {
    pub state: S,
}

impl<S: States> Plugin for LoadscreenPlugin<S> {
    fn build(&self, app: &mut App) {
        app.add_system(setup_loadscreen.in_schedule(OnEnter(self.state.clone())));
        app.add_system(
            despawn_all_recursive::<With<LoadscreenCleanup>>
                .in_schedule(OnExit(self.state.clone())),
        );
        app.add_system(update_loading_pct.run_if(in_state(self.state.clone())));
    }
}

#[derive(Component)]
struct LoadscreenCleanup;

#[derive(Component)]
struct LoadingProgressIndicator;

fn setup_loadscreen(mut commands: Commands) {
    commands.spawn((LoadscreenCleanup, Camera2dBundle {
        camera_2d: Camera2d {
            clear_color: ClearColorConfig::Custom(Color::BLACK),
        },
        ..Default::default()
    }));

    let container = commands
        .spawn((LoadscreenCleanup, NodeBundle {
            background_color: BackgroundColor(Color::GRAY),
            style: Style {
                size: Size::new(Val::Auto, Val::Auto),
                position_type: PositionType::Absolute,
                position: UiRect {
                    bottom: Val::Percent(48.0),
                    top: Val::Percent(48.0),
                    left: Val::Percent(20.0),
                    right: Val::Percent(20.0),
                },
                padding: UiRect::all(Val::Px(2.0)),
                flex_direction: FlexDirection::Column,
                justify_content: JustifyContent::Center,
                ..Default::default()
            },
            ..Default::default()
        }))
        .id();

    let inner = commands
        .spawn((LoadingProgressIndicator, NodeBundle {
            background_color: BackgroundColor(Color::WHITE),
            style: Style {
                size: Size::new(Val::Percent(0.0), Val::Percent(100.0)),
                ..Default::default()
            },
            ..Default::default()
        }))
        .id();

    commands.entity(container).push_children(&[inner]);
}

fn update_loading_pct(
    mut q: Query<&mut Style, With<LoadingProgressIndicator>>,
    progress: Res<ProgressCounter>,
) {
    let progress: f32 = progress.progress().into();
    for mut style in q.iter_mut() {
        style.size.width = Val::Percent(progress * 100.0);
    }
}
