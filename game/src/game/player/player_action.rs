//! Player input action definitions and default bindings.
//!
//! Also provides a helper system to track the last non-zero horizontal input.
use leafwing_input_manager::prelude::*;
use theseeker_engine::input::InputManagerPlugin;

use crate::prelude::*;

pub struct PlayerActionPlugin;

impl Plugin for PlayerActionPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(InputManagerPlugin::<PlayerAction>::default());
        app.add_systems(
            crate::GameTickUpdate,
            track_last_move_dir
                .in_set(crate::game::player::PlayerStateSet::Input),
        );
    }
}

#[derive(Actionlike, PartialEq, Eq, Clone, Copy, Hash, Debug, Reflect)]
pub enum PlayerAction {
    #[actionlike(Axis)]
    Move,
    Jump,
    /// Non-jump up modifier used to select Up variants for skills (mapped to 'I')
    UpModifier,
    Skill1, // First equipped skill
    Skill2, // Second equipped skill
    Skill3, // Third equipped skill
    Skill4, // Fourth equipped skill
    Skill5, // Fifth equipped skill (Battery Pack)
    Fall,
    SwapCombatStyle,
    SwapMeleeWeapon,
    Interact,
    ToggleControlOverlay,
    TogglePassiveInventory,
}

impl PlayerAction {
    /// Default key/gamepad bindings used by the game.
    pub fn input_map() -> InputMap<Self> {
        InputMap::new([
            (Self::Jump, KeyCode::Space),
            (Self::Jump, KeyCode::KeyW),
            (Self::Jump, KeyCode::ArrowUp),
            (Self::UpModifier, KeyCode::KeyI),
            (Self::Fall, KeyCode::ArrowDown),
            (Self::Fall, KeyCode::KeyS),
            (Self::Skill1, KeyCode::Digit1),
            (Self::Skill1, KeyCode::KeyJ),
            (Self::Skill2, KeyCode::Digit2),
            (Self::Skill2, KeyCode::KeyK),
            (Self::Skill3, KeyCode::Digit3),
            (Self::Skill3, KeyCode::KeyL),
            (Self::Skill4, KeyCode::Digit4),
            (Self::Skill4, KeyCode::Semicolon),
            (Self::Skill5, KeyCode::Digit5),
            (Self::Skill5, KeyCode::Quote),
            (Self::SwapCombatStyle, KeyCode::KeyH),
            (
                Self::SwapCombatStyle,
                KeyCode::Backquote,
            ),
            (Self::SwapMeleeWeapon, KeyCode::KeyG),
            (Self::Interact, KeyCode::KeyF),
            (
                Self::ToggleControlOverlay,
                KeyCode::KeyC,
            ),
            (
                Self::TogglePassiveInventory,
                KeyCode::KeyM,
            ),
        ])
        .with_axis(Self::Move, VirtualAxis::ad())
        // .with_axis(
        //     Self::Move,
        //     VirtualAxis::new(KeyCode::KeyA, KeyCode::KeyD),
        // )
        .with_axis(
            Self::Move,
            VirtualAxis::new(KeyCode::ArrowLeft, KeyCode::ArrowRight),
        )
        .with_axis(Self::Move, VirtualAxis::dpad_x())
        .with_axis(
            Self::Move,
            GamepadControlAxis::new(GamepadAxis::LeftStickX),
        )
        // Note: Fall is a button; if modeling as an axis later, the enum would need updating
        .with_multiple([
            (Self::Fall, GamepadButton::DPadDown),
            (Self::Jump, GamepadButton::LeftTrigger2),
            (Self::Skill1, GamepadButton::West),
            (
                Self::Skill2,
                GamepadButton::RightTrigger2,
            ),
            (Self::Skill3, GamepadButton::South),
            (Self::Skill4, GamepadButton::East),
            (
                Self::SwapCombatStyle,
                GamepadButton::LeftTrigger,
            ),
            (
                Self::SwapMeleeWeapon,
                GamepadButton::RightTrigger,
            ),
            (Self::Interact, GamepadButton::North),
            (
                Self::ToggleControlOverlay,
                GamepadButton::Start,
            ),
        ])
    }
}

/// Maintain `LastMoveDir` as the last non-zero horizontal move input.
fn track_last_move_dir(
    mut query: Query<
        (
            &ActionState<PlayerAction>,
            Option<&mut super::states::LastMoveDir>,
            Entity,
        ),
        With<super::Player>,
    >,
    mut commands: Commands,
) {
    for (action_state, maybe_last, entity) in query.iter_mut() {
        let dir = action_state.clamped_value(&PlayerAction::Move).signum();
        if dir != 0.0 {
            if let Some(mut last) = maybe_last {
                last.0 = dir;
            } else {
                commands
                    .entity(entity)
                    .insert(super::states::LastMoveDir(dir));
            }
        }
    }
}
