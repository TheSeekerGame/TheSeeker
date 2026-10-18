use crate::camera::GameplayPixelScale;
use crate::game::player::{Player, PlayerAction, PlayerStateSet};
use crate::game::player::InputBuffer;
use std::collections::HashSet;
use crate::prelude::*;
use crate::ui::skill_toolbar::SkillToolbarPlugin;
use bevy::ecs::system::SystemParam;
use bevy::ui::UiScale;
use leafwing_input_manager::prelude::ActionState;

pub(crate) mod capsule_ui;
mod controls_overlay;
pub(crate) mod inventory;
pub(crate) mod mainmenu;
pub(crate) mod pixel_canvas;
pub(crate) mod pixel_font;
pub(crate) mod pixel_panel;
mod planet_status;
mod screen_frame;
mod skill_toolbar;

pub struct UiPlugin;

impl Plugin for UiPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<pixel_canvas::ArtLibrary>();
        app.add_systems(Update, sync_ui_scale.after(crate::camera::update_camera));
        configure_input_ownership(app);
        app.add_systems(OnExit(AppState::InGame), reset_ui);
        app.add_plugins((
            controls_overlay::plugin,
            capsule_ui::CapsuleUiPlugin,
            inventory::InventoryPlugin,
            screen_frame::ScreenFramePlugin,
            planet_status::PlanetStatusHudPlugin,
            self::mainmenu::MainMenuPlugin,
            SkillToolbarPlugin,
        ));
    }
}

fn configure_input_ownership(app: &mut App) {
    app.init_resource::<GameplayInputGuard>();
    app.configure_sets(PreUpdate, (UiInputSet::Menu, UiInputSet::Inventory, UiInputSet::Panels, UiInputSet::Pause, UiInputSet::Suppress)
        .chain().after(theseeker_engine::input::InputManagerSystem::Update));
    app.add_systems(PreUpdate, suppress_frame_input.in_set(UiInputSet::Suppress).run_if(in_state(AppState::InGame)));
    app.add_systems(GameTickUpdate, suppress_gameplay_input.before(PlayerStateSet::Input)
        .run_if(in_state(GameState::Playing).and_then(in_state(AppState::InGame))));
}

/// Frame input resolves modal ownership before state transitions and simulation admission.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum UiInputSet { Menu, Inventory, Panels, Pause, Suppress }

/// Root of an always-on gameplay HUD element, hidden while a cinematic plays.
#[derive(Component)]
pub(crate) struct GameplayHud;

/// UI scales with the gameplay pixel size.
pub(crate) fn sync_ui_scale(pixel_scale: Option<Res<GameplayPixelScale>>, mut ui_scale: ResMut<UiScale>) {
    let scale = pixel_scale.map_or(1.0, |scale| scale.ui_scale_factor());
    if (ui_scale.0 - scale).abs() > f32::EPSILON {
        ui_scale.0 = scale;
    }
}

/// Keyboard panels and the pause menu own gameplay input while open.
#[derive(SystemParam)]
pub(crate) struct OpenPanels<'w> {
    capsule: Res<'w, capsule_ui::CapsuleUiState>,
    inventory: Res<'w, inventory::InventoryState>,
    menu: Res<'w, mainmenu::MenuState>,
}

impl OpenPanels<'_> {
    pub(crate) fn menu_input_consumed(&self) -> bool { self.menu.blocks_panel_input() }

    pub(crate) fn any(&self) -> bool {
        self.capsule.is_open() || self.inventory.is_open() || self.menu.is_open()
    }
}

#[derive(Resource, Default)]
struct GameplayInputGuard {
    blocked: HashSet<PlayerAction>,
    was_modal: bool,
}

const GAMEPLAY_ACTIONS: [PlayerAction; 13] = [
    PlayerAction::Move, PlayerAction::Jump, PlayerAction::Fall, PlayerAction::Interact,
    PlayerAction::Skill1, PlayerAction::Skill2, PlayerAction::Skill3, PlayerAction::Skill4,
    PlayerAction::Skill5, PlayerAction::SwapMeleeWeapon, PlayerAction::UpModifier,
    PlayerAction::ToggleControlOverlay, PlayerAction::DebugUnlockAllProgression,
];

/// Drops modal input in both action banks, including edges collected on zero-tick frames.
fn suppress_frame_input(
    panels: OpenPanels,
    pause: Res<crate::gamestate::PauseRequests>,
    mut guard: ResMut<GameplayInputGuard>,
    mut players: Query<(&mut ActionState<PlayerAction>, Option<&mut InputBuffer>), With<Player>>,
) {
    let modal = panels.any() || panels.menu.blocks_panel_input() || pause.is_paused();
    for (mut actions, buffer) in &mut players {
        suppress_frame_actions(&mut actions, &mut guard, modal);
        if let Some(mut buffer) = buffer.filter(|_| modal || guard.was_modal) { buffer.clear_all(); }
        if modal || guard.was_modal {
            actions.set_update_state_from_state();
            actions.set_fixed_update_state_from_state();
        }
    }
    guard.was_modal = modal;
}

fn suppress_gameplay_input(
    panels: OpenPanels,
    guard: Res<GameplayInputGuard>,
    mut players: Query<&mut ActionState<PlayerAction>, With<Player>>,
) {
    for mut actions in &mut players {
        suppress_tick_actions(&mut actions, panels.any(), &guard);
    }
}

fn suppress_frame_actions(actions: &mut ActionState<PlayerAction>, guard: &mut GameplayInputGuard, modal: bool) {
    for action in GAMEPLAY_ACTIONS {
        let active = if action == PlayerAction::Move { actions.value(&action).abs() > f32::EPSILON } else { actions.pressed(&action) };
        if modal && active {
            guard.blocked.insert(action);
        } else if !active {
            guard.blocked.remove(&action);
        }
        if modal || guard.blocked.contains(&action) { actions.reset(&action); }
    }
}

fn suppress_tick_actions(actions: &mut ActionState<PlayerAction>, modal: bool, guard: &GameplayInputGuard) {
    for action in GAMEPLAY_ACTIONS {
        if modal || guard.blocked.contains(&action) { actions.reset(&action); }
    }
}

fn reset_ui(
    mut capsule: ResMut<capsule_ui::CapsuleUiState>,
    mut inventory: ResMut<inventory::InventoryState>,
    mut menu: ResMut<mainmenu::MenuState>,
    mut guard: ResMut<GameplayInputGuard>,
) {
    *capsule = default();
    *inventory = default();
    *menu = default();
    *guard = default();
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::ecs::system::RunSystemOnce;
    use leafwing_input_manager::systems::{swap_to_fixed_update, swap_to_update};

    #[test]
    fn modal_input_is_flushed_from_both_action_banks_and_held_keys_require_release() {
        let mut world = World::new();
        let mut actions = ActionState::default();
        actions.press(&PlayerAction::Jump);
        actions.set_update_state_from_state();
        actions.set_fixed_update_state_from_state();
        let player = world.spawn(actions).id();
        let mut guard = GameplayInputGuard::default();
        {
            let mut actions = world.get_mut::<ActionState<PlayerAction>>(player).unwrap();
            suppress_frame_actions(&mut actions, &mut guard, true);
            actions.set_update_state_from_state();
            actions.set_fixed_update_state_from_state();
        }
        world.run_system_once(swap_to_fixed_update::<PlayerAction>).unwrap();
        {
            let mut actions = world.get_mut::<ActionState<PlayerAction>>(player).unwrap();
            assert!(!actions.just_pressed(&PlayerAction::Jump));
            assert!(!actions.pressed(&PlayerAction::Jump));
            actions.press(&PlayerAction::Jump);
            suppress_tick_actions(&mut actions, false, &guard);
            assert!(!actions.pressed(&PlayerAction::Jump), "a key held across resume stays owned by the menu");
        }
        world.run_system_once(swap_to_update::<PlayerAction>).unwrap();
        let mut actions = world.get_mut::<ActionState<PlayerAction>>(player).unwrap();
        assert!(!actions.pressed(&PlayerAction::Jump));
        actions.release(&PlayerAction::Jump);
        suppress_frame_actions(&mut actions, &mut guard, false);
        actions.press(&PlayerAction::Jump);
        suppress_frame_actions(&mut actions, &mut guard, false);
        assert!(actions.just_pressed(&PlayerAction::Jump), "a new gameplay press is accepted after release");
    }
}
