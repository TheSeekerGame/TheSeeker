use crate::prelude::*;

pub struct GameStatePlugin;

impl Plugin for GameStatePlugin {
    fn build(&self, app: &mut App) {
        app.init_state::<GameState>();
        // Wire pause/unpause on P key in the standard Bevy Update schedule
        app.add_systems(
            Update,
            (
                pause.run_if(in_state(GameState::Playing)),
                unpause.run_if(in_state(GameState::Paused)),
            ),
        );
    }
}

#[derive(States, Clone, Eq, PartialEq, Hash, Debug, Default)]
pub enum GameState {
    #[default]
    Playing,
    Paused,
}

pub fn pause(
    mut next_state: ResMut<NextState<GameState>>,
    input: Res<ButtonInput<KeyCode>>,
) {
    if input.just_pressed(KeyCode::KeyP) {
        next_state.set(GameState::Paused);
    }
}

pub fn unpause(
    mut next_state: ResMut<NextState<GameState>>,
    input: Res<ButtonInput<KeyCode>>,
) {
    if input.just_pressed(KeyCode::KeyP) {
        next_state.set(GameState::Playing);
    }
}
