use crate::prelude::*;

pub struct DevPlugin;

impl Plugin for DevPlugin {
    fn build(&self, app: &mut App) {
        app.register_clicommand_args("phystester_at", cli_phystester_at);
        app.add_systems(
            Last,
            debug_progress
                .run_if(resource_exists::<ProgressCounter>())
                .after(iyes_progress::TrackedProgressSet),
        );
    }
}

fn debug_progress(counter: Res<ProgressCounter>) {
    let progress = counter.progress();
    let progress_full = counter.progress_complete();
    trace!(
        "Progress: {}/{}; Full Progress: {}/{}",
        progress.done,
        progress.total,
        progress_full.done,
        progress_full.total,
    );
}

/// Temporary function to use during development
///
/// If there is no proper code to set up a camera in a given app state (or whatever)
/// yet, use this to spawn a default 2d camera.
#[allow(dead_code)]
fn debug_setup_camera(mut commands: Commands) {
    commands.spawn((
        Camera2dBundle::default(),
        StateDespawnMarker,
    ));
}

fn cli_phystester_at(In(args): In<Vec<String>>, mut commands: Commands) {
    if args.len() != 2 {
        error!("\"phystester_at <x> <y>\"");
        return;
    }
    if let (Ok(x), Ok(y)) = (args[0].parse(), args[1].parse()) {
        commands.spawn((
            RigidBody::Dynamic,
            Mass(1.0),
            Collider::ball(4.0),
            SpriteBundle {
                sprite: Sprite {
                    color: Color::PINK,
                    custom_size: Some(Vec2::splat(8.0)),
                    ..Default::default()
                },
                transform: Transform::from_xyz(x, y, 100.0),
                ..Default::default()
            },
        ));
    }
}
