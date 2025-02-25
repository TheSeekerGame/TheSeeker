use leafwing_input_manager::axislike::AxisType;
use leafwing_input_manager::prelude::{
    Actionlike, InputMap, SingleAxis, VirtualAxis,
    WithAxisProcessingPipelineExt,
};
use leafwing_input_manager::InputManagerBundle;
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
    SwapSwitchStyle,
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
            (Self::SwapSwitchStyle, KeyCode::KeyT),
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
        .with_multiple([
            (
                Self::Move,
                VirtualAxis::from_keys(KeyCode::KeyA, KeyCode::KeyD),
            ),
            (
                Self::Move,
                VirtualAxis::from_keys(KeyCode::ArrowLeft, KeyCode::ArrowRight),
            ),
        ])
        .with(
            Self::Move,
            VirtualAxis::horizontal_dpad(),
        )
        .with_multiple([
            (
                Self::Move,
                SingleAxis::new(AxisType::Gamepad(
                    GamepadAxisType::LeftStickX,
                )),
            ),
            (
                Self::Fall,
                SingleAxis::new(AxisType::Gamepad(
                    GamepadAxisType::LeftStickY,
                ))
                .with_bounds(-1.0, 0.0),
            ),
        ])
        .with_multiple([
            (Self::Fall, GamepadButtonType::DPadDown),
            (
                Self::Jump,
                GamepadButtonType::LeftTrigger2,
            ),
            (Self::Attack, GamepadButtonType::West),
            (
                Self::Dash,
                GamepadButtonType::RightTrigger2,
            ),
            (Self::Whirl, GamepadButtonType::South),
            (Self::Stealth, GamepadButtonType::East),
            (
                Self::SwapCombatStyle,
                GamepadButtonType::LeftTrigger,
            ),
            (
                Self::SwapSwitchStyle,
                GamepadButtonType::Mode,
            ),
            (
                Self::SwapMeleeWeapon,
                GamepadButtonType::RightTrigger,
            ),
            (Self::Interact, GamepadButtonType::North),
            (
                Self::ToggleControlOverlay,
                GamepadButtonType::Start,
            ),
        ])
    }
}
