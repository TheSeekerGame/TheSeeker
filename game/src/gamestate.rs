use crate::prelude::*;

pub struct GameStatePlugin;

impl Plugin for GameStatePlugin {
    fn build(&self, app: &mut App) {
        app.init_state::<GameState>();
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
    mut time: ResMut<Time<Physics>>,
) {
    if input.just_pressed(KeyCode::KeyP) {
        next_state.set(GameState::Paused);
        //pause physics
        time.pause();
    }
}

pub fn unpause(
    mut next_state: ResMut<NextState<GameState>>,
    input: Res<ButtonInput<KeyCode>>,
    mut time: ResMut<Time<Physics>>,
) {
    if input.just_pressed(KeyCode::KeyP) {
        next_state.set(GameState::Playing);
        time.unpause();
    }
}


