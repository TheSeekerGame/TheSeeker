use crate::prelude::*;
use crate::ui::{OpenPanels, UiInputSet};

pub struct GameStatePlugin;

impl Plugin for GameStatePlugin {
    fn build(&self, app: &mut App) {
        app.init_state::<GameState>().init_resource::<PauseRequests>();
        app.add_systems(PreUpdate, update_pause_requests.in_set(UiInputSet::Pause)
            .run_if(in_state(AppState::InGame)));
        app.add_systems(OnExit(AppState::InGame), clear_pause_requests);
    }
}

#[derive(States, Clone, Eq, PartialEq, Hash, Debug, Default)]
pub enum GameState {
    #[default]
    Playing,
    Paused,
}

/// Independent owners keep closing a panel from cancelling a manual pause.
#[derive(Resource, Default)]
pub(crate) struct PauseRequests {
    manual: bool,
    modal: bool,
}

impl PauseRequests {
    pub(crate) fn is_paused(&self) -> bool { self.manual || self.modal }
    pub(crate) fn resume_manual(&mut self) { self.manual = false; }
}

fn update_pause_requests(
    input: Res<ButtonInput<KeyCode>>,
    panels: OpenPanels,
    mut requests: ResMut<PauseRequests>,
    state: Res<State<GameState>>,
    mut next: ResMut<NextState<GameState>>,
    mut transport: ResMut<GameTransport>,
) {
    if input.just_pressed(KeyCode::KeyP) { requests.manual = !requests.manual; }
    requests.modal = panels.any();
    apply_pause(&requests, state.get(), &mut next, &mut transport);
}

fn apply_pause(requests: &PauseRequests, current: &GameState, next: &mut NextState<GameState>, transport: &mut GameTransport) {
    let desired = if requests.is_paused() { GameState::Paused } else { GameState::Playing };
    transport.paused = requests.is_paused();
    if *current != desired { next.set(desired); }
}

fn clear_pause_requests(
    mut requests: ResMut<PauseRequests>,
    mut next: ResMut<NextState<GameState>>,
    mut transport: ResMut<GameTransport>,
) {
    *requests = default();
    transport.paused = false;
    next.set(GameState::Playing);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn closing_a_panel_preserves_manual_pause_and_transport() {
        let mut requests = PauseRequests { manual: true, modal: true };
        let mut next = NextState::default();
        let mut transport = GameTransport::default();
        requests.modal = false;
        apply_pause(&requests, &GameState::Playing, &mut next, &mut transport);
        assert!(transport.paused);
        assert!(matches!(next, NextState::Pending(GameState::Paused)));
        requests.manual = false;
        apply_pause(&requests, &GameState::Paused, &mut next, &mut transport);
        assert!(!transport.paused);
        assert!(matches!(next, NextState::Pending(GameState::Playing)));
    }
}
