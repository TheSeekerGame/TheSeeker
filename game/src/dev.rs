use crate::prelude::*;
use bevy::diagnostic::{FrameTimeDiagnosticsPlugin, LogDiagnosticsPlugin};
use crate::gamestate::{pause, unpause};

pub struct DevPlugin;

impl Plugin for DevPlugin {
    fn build(&self, app: &mut App) {
        app.register_clicommand_args("spawn_phystester", cli_spawn_phystester);
        app.register_clicommand_args("spawn_script", cli_spawn_script);
        app.register_clicommand_args("spawn_anim", cli_spawn_anim);
        app.add_systems(
            Last,
            debug_progress
                .run_if(resource_exists::<ProgressCounter>)
                .after(iyes_progress::TrackedProgressSet),
        );
        app.add_systems(
            GameTickUpdate,
            (
                pause.run_if(in_state(GameState::Playing)),
                unpause.run_if(in_state(GameState::Paused)),
            ),
        );
        app.add_plugins(PhysicsDebugPlugin::default());
        app.add_plugins((
            FrameTimeDiagnosticsPlugin,
            LogDiagnosticsPlugin::default(),
        ));
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

#[allow(dead_code)]
fn debug_spawn_player(mut commands: Commands) {
    use crate::game::player::PlayerBlueprint;

    commands.spawn((
        PlayerBlueprint,
        SpatialBundle { ..default() },
    ));
}

fn cli_spawn_phystester(In(args): In<Vec<String>>, mut commands: Commands) {
    if args.len() != 2 {
        error!("\"spawn_phystester <x> <y>\"");
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

fn cli_spawn_script(In(args): In<Vec<String>>, world: &mut World) {
    use theseeker_engine::script::common::ScriptBundle;
    use theseeker_engine::script::ScriptPlayer;

    if args.len() != 1 {
        error!("\"spawn_script <script_asset_key>\"");
        return;
    }
    let mut player = ScriptPlayer::new();
    player.play_key(args[0].as_str());
    world.spawn(ScriptBundle { player });
}

fn cli_spawn_anim(In(args): In<Vec<String>>, world: &mut World) {
    use theseeker_engine::animation::SpriteAnimationBundle;
    use theseeker_engine::script::ScriptPlayer;

    if args.len() != 1 && args.len() != 3 {
        error!("\"spawn_anim <anim_asset_key> [<x> <y>]\"");
        return;
    }

    let (mut x, mut y) = (0.0, 0.0);
    if args.len() == 3 {
        if let (Ok(xx), Ok(yy)) = (args[1].parse(), args[2].parse()) {
            x = xx;
            y = yy;
        }
    }

    let mut player = ScriptPlayer::new();
    player.play_key(args[0].as_str());

    world.spawn((
        SpriteSheetBundle {
            transform: Transform::from_xyz(x, y, 101.0),
            ..default()
        },
        SpriteAnimationBundle { player },
    ));
}
