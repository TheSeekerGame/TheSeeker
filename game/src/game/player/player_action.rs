//! Player input action definitions and default bindings.
//!
//! Also resolves the player's effective horizontal movement/facing intent.
use leafwing_input_manager::prelude::*;
use theseeker_engine::input::InputManagerPlugin;
use theseeker_engine::time::GameTime;

use crate::game::gentstate::Facing;
use crate::game::player::states::HorizontalInputIntent;
use crate::prelude::*;

pub struct PlayerActionPlugin;

impl Plugin for PlayerActionPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(InputManagerPlugin::<PlayerAction>::default());
        app.add_systems(
            crate::GameTickUpdate,
            resolve_horizontal_input_intent
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
    SwapMeleeWeapon,
    Interact,
    ToggleControlOverlay,
    TogglePassiveInventory,
    DebugUnlockAllProgression,
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
            (Self::Interact, KeyCode::KeyF),
            (
                Self::ToggleControlOverlay,
                KeyCode::KeyC,
            ),
            (
                Self::TogglePassiveInventory,
                KeyCode::KeyM,
            ),
            (
                Self::DebugUnlockAllProgression,
                KeyCode::Digit0,
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
            (Self::Interact, GamepadButton::North),
            (
                Self::ToggleControlOverlay,
                GamepadButton::Start,
            ),
        ])
    }
}

/// Resolve movement and facing so horizontal motion can stay independent from facing
/// when the player presses the opposite direction while continuing to hold the original.
fn resolve_horizontal_input_intent(
    keyboard: Res<ButtonInput<KeyCode>>,
    panels: crate::ui::OpenPanels,
    time: Res<GameTime>,
    mut query: Query<
        (
            &mut ActionState<PlayerAction>,
            &mut HorizontalInputIntent,
            &mut super::states::LastMoveDir,
            &mut Facing,
            Option<&super::autoaim::RangedAttackFacingLock>,
            Option<&crate::game::actors::action::ActionFacing>,
        ),
        With<super::Player>,
    >,
) {
    let modal_ui_open = panels.any();

    for (
        mut action_state,
        mut intent,
        mut last_move_dir,
        mut facing,
        facing_lock,
        action_facing,
    ) in query.iter_mut() {
        let locked_facing = action_facing.map(crate::game::actors::action::ActionFacing::facing)
            .or_else(|| facing_lock.and_then(|lock| lock.active_facing(time.tick())));

        if modal_ui_open {
            action_state.reset(&PlayerAction::Jump);
            action_state.reset(&PlayerAction::Fall);
            action_state.reset(&PlayerAction::Move);
            action_state.reset(&PlayerAction::Interact);
            *intent = HorizontalInputIntent {
                movement: 0.0,
                facing: None,
            };
            apply_resolved_facing(&mut facing, locked_facing, None);
            continue;
        }

        let axis_movement = action_state.clamped_value(&PlayerAction::Move);
        let left_held = is_pressed(
            &keyboard,
            &[KeyCode::KeyA, KeyCode::ArrowLeft],
        );
        let right_held = is_pressed(
            &keyboard,
            &[KeyCode::KeyD, KeyCode::ArrowRight],
        );
        let left_just_pressed = is_just_pressed(
            &keyboard,
            &[KeyCode::KeyA, KeyCode::ArrowLeft],
        );
        let right_just_pressed = is_just_pressed(
            &keyboard,
            &[KeyCode::KeyD, KeyCode::ArrowRight],
        );

        let previous_intent = *intent;
        let fallback_movement = fallback_horizontal_movement(
            previous_intent,
            last_move_dir.0,
            axis_movement,
            *facing,
        );
        let fallback_facing = previous_intent.facing.unwrap_or(*facing);

        *intent = match (left_held, right_held) {
            (true, false) => HorizontalInputIntent {
                movement: -1.0,
                facing: Some(Facing::Left),
            },
            (false, true) => HorizontalInputIntent {
                movement: 1.0,
                facing: Some(Facing::Right),
            },
            (true, true) => both_directions_intent(
                left_just_pressed,
                right_just_pressed,
                fallback_movement,
                fallback_facing,
            ),
            (false, false) => axis_horizontal_intent(axis_movement),
        };

        if intent.movement != 0.0 {
            last_move_dir.0 = intent.movement.signum();
        }
        apply_resolved_facing(&mut facing, locked_facing, intent.facing);
    }
}

fn apply_resolved_facing(
    facing: &mut Facing,
    locked_facing: Option<Facing>,
    intent_facing: Option<Facing>,
) {
    if let Some(locked_facing) = locked_facing {
        *facing = locked_facing;
        return;
    }

    if let Some(intent_facing) = intent_facing {
        *facing = intent_facing;
    }
}

fn fallback_horizontal_movement(
    previous_intent: HorizontalInputIntent,
    last_move_dir: f32,
    axis_movement: f32,
    facing: Facing,
) -> f32 {
    [previous_intent.movement, last_move_dir, axis_movement]
        .into_iter()
        .find(|movement| *movement != 0.0)
        .map(f32::signum)
        .unwrap_or_else(|| facing.direction())
}

fn both_directions_intent(
    left_just_pressed: bool,
    right_just_pressed: bool,
    fallback_movement: f32,
    fallback_facing: Facing,
) -> HorizontalInputIntent {
    match (left_just_pressed, right_just_pressed) {
        (true, false) => HorizontalInputIntent {
            movement: 1.0,
            facing: Some(Facing::Left),
        },
        (false, true) => HorizontalInputIntent {
            movement: -1.0,
            facing: Some(Facing::Right),
        },
        _ => HorizontalInputIntent {
            movement: fallback_movement,
            facing: Some(fallback_facing),
        },
    }
}

fn axis_horizontal_intent(axis_movement: f32) -> HorizontalInputIntent {
    HorizontalInputIntent {
        movement: axis_movement,
        facing: facing_from_axis(axis_movement),
    }
}

fn facing_from_axis(axis_movement: f32) -> Option<Facing> {
    match axis_movement.total_cmp(&0.0) {
        std::cmp::Ordering::Greater => Some(Facing::Right),
        std::cmp::Ordering::Less => Some(Facing::Left),
        std::cmp::Ordering::Equal => None,
    }
}

fn is_pressed(keyboard: &ButtonInput<KeyCode>, keys: &[KeyCode]) -> bool {
    keys.iter().copied().any(|key| keyboard.pressed(key))
}

fn is_just_pressed(keyboard: &ButtonInput<KeyCode>, keys: &[KeyCode]) -> bool {
    keys.iter().copied().any(|key| keyboard.just_pressed(key))
}
