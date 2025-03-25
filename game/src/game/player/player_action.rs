use leafwing_input_manager::{prelude::*, InputManagerBundle};
use theseeker_engine::input::InputManagerPlugin;

use crate::prelude::*;

pub struct PlayerActionPlugin;

impl Plugin for PlayerActionPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(InputManagerPlugin::<PlayerAction>::default());
    }
}

#[derive(Actionlike, PartialEq, Eq, Clone, Copy, Hash, Debug, Reflect)]
pub enum PlayerAction {
    Move,
    Jump,
    Attack,
    Dash,
    Whirl,
    Stealth,
    Fall,
    SwapCombatStyle,
    SwapMeleeWeapon,
    Interact,
    ToggleControlOverlay,
}

impl PlayerAction {
    pub fn input_manager_bundle() -> InputManagerBundle<Self> {
        InputManagerBundle::<Self>::with_map(Self::input_map())
    }

    pub fn input_map() -> InputMap<Self> {
        InputMap::new([
            (Self::Jump, KeyCode::Space),
            (Self::Jump, KeyCode::KeyW),
            (Self::Jump, KeyCode::ArrowUp),
            (Self::Fall, KeyCode::ArrowDown),
            (Self::Fall, KeyCode::KeyS),
            (Self::Attack, KeyCode::Digit1),
            (Self::Attack, KeyCode::KeyJ),
            (Self::Dash, KeyCode::KeyK),
            (Self::Dash, KeyCode::Digit2),
            (Self::Whirl, KeyCode::KeyL),
            (Self::Whirl, KeyCode::Digit3),
            (Self::Stealth, KeyCode::Digit4),
            (Self::Stealth, KeyCode::Semicolon),
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
        ])
        .with_axis(
            Self::Move,
            VirtualAxis::new(KeyCode::KeyA, KeyCode::KeyD),
        )
        .with_axis(
            Self::Move,
            VirtualAxis::new(KeyCode::ArrowLeft, KeyCode::ArrowRight),
        )
        .with_axis(Self::Move, VirtualAxis::dpad_x())
        .with_axis(
            Self::Move,
            GamepadControlAxis::new(GamepadAxis::LeftStickX),
        )
        .with_axis(
            Self::Fall,
            GamepadControlAxis::new(GamepadAxis::LeftStickY)
                .with_bounds(-1.0, 0.0),
        )
        .with_multiple([
            (Self::Fall, GamepadButton::DPadDown),
            (Self::Jump, GamepadButton::LeftTrigger2),
            (Self::Attack, GamepadButton::West),
            (Self::Dash, GamepadButton::RightTrigger2),
            (Self::Whirl, GamepadButton::South),
            (Self::Stealth, GamepadButton::East),
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
