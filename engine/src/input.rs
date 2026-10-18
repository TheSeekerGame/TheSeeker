/// reimplementation of the leafwing_input_manager plugin to allow for input state update systems
/// in our custom fixed timestep
use core::hash::Hash;
use core::marker::PhantomData;
use std::fmt::Debug;

use bevy::app::{App, Plugin, Update};
use bevy::ecs::prelude::*;
use bevy::input::gamepad::GamepadButton;
use bevy::input::keyboard::KeyCode;
use bevy::input::mouse::*;
use bevy::input::InputSystems;
use bevy::prelude::{PostUpdate, PreUpdate};
use bevy::reflect::TypePath;
use leafwing_input_manager::action_state::{ActionState, ButtonData};
use leafwing_input_manager::clashing_inputs::ClashStrategy;
use leafwing_input_manager::input_map::InputMap;
use leafwing_input_manager::input_processing::*;
use leafwing_input_manager::plugin::CentralInputStorePlugin;
use leafwing_input_manager::prelude::updating::CentralInputStore;
use leafwing_input_manager::prelude::{
    AxislikeChord, ButtonlikeChord, DualAxislikeChord, GamepadControlAxis,
    GamepadControlDirection, GamepadStick, ModifierKey, MouseMove,
    MouseMoveAxis, MouseMoveDirection, MouseScroll, MouseScrollAxis,
    MouseScrollDirection, RegisterUserInput, TripleAxislikeChord, VirtualAxis,
    VirtualDPad, VirtualDPad3D,
};

use leafwing_input_manager::Actionlike;

use crate::time::GameTickPost;
use crate::GameTickSet;

/// Collects physical input in `PreUpdate` and delivers it to the custom game ticks.
///
/// Install one plugin per action enum. Entities carry an [`InputMap`] and an
/// [`ActionState`]; input collection runs after Bevy input and Leafwing's unified
/// input store. Systems consuming render-frame actions run after
/// [`InputManagerSystem::Update`].
///
/// The fixed action bank is selected before simulation and restored afterward.
/// Edges remain pending on frames with no ticks and are cleared in
/// [`GameTickPost`] between catch-up ticks. Modal gameplay suppression runs after
/// collection, before the gameplay systems that consume actions.
pub struct InputManagerPlugin<A: Actionlike> {
    _phantom: PhantomData<A>,
}

// Deriving default induces an undesired bound on the generic
impl<A: Actionlike> Default for InputManagerPlugin<A> {
    fn default() -> Self {
        Self {
            _phantom: PhantomData,
        }
    }
}

impl<A: Actionlike + TypePath + bevy::reflect::GetTypeRegistration> Plugin
    for InputManagerPlugin<A>
{
    fn build(&self, app: &mut App) {
        use leafwing_input_manager::systems::*;

        if !app.is_plugin_added::<CentralInputStorePlugin>() {
            app.add_plugins(CentralInputStorePlugin);
            app.add_systems(PreUpdate, clear_central_input_store
                .before(leafwing_input_manager::plugin::InputManagerSystem::Unify));
        }

        // Main schedule
        app.add_systems(
            PreUpdate,
            tick_action_state::<A>
                .in_set(InputManagerSystem::Tick)
                .before(InputManagerSystem::Update),
        )
        .add_systems(
            PostUpdate,
            release_on_input_map_removed::<A>,
        );

        app.add_systems(
            PreUpdate,
            update_action_state::<A>.in_set(InputManagerSystem::Update),
        );

        app.configure_sets(
            PreUpdate,
            InputManagerSystem::Update.after(InputSystems)
                .after(leafwing_input_manager::plugin::InputManagerSystem::Unify),
        );

        // GameTickUpdate just_pressed state fixes
        app.add_systems(
            Update,
            (
                swap_to_fixed_update::<A>,
                // we want to update the ActionState only once, even if the GameTickUpdate schedule runs multiple times
                update_action_state::<A>,
            )
                .chain()
                .in_set(GameTickSet::Pre),
        );

        app.add_systems(
            GameTickPost,
            release_on_input_map_removed::<A>,
        );
        app.add_systems(
            // this needs to run between runs of GameTickUpdate
            GameTickPost,
            tick_action_state::<A>
                .in_set(InputManagerSystem::Tick)
                .before(InputManagerSystem::Update),
        );
        app.add_systems(
            Update,
            swap_to_update::<A>.in_set(GameTickSet::Post),
        );

        // #[cfg(feature = "mouse")]
        app.register_buttonlike_input::<MouseButton>()
            .register_buttonlike_input::<MouseMoveDirection>()
            .register_buttonlike_input::<MouseButton>()
            .register_axislike_input::<MouseMoveAxis>()
            .register_dual_axislike_input::<MouseMove>()
            .register_buttonlike_input::<MouseScrollDirection>()
            .register_axislike_input::<MouseScrollAxis>()
            .register_dual_axislike_input::<MouseScroll>();

        // #[cfg(feature = "keyboard")]
        app.register_buttonlike_input::<KeyCode>()
            .register_buttonlike_input::<ModifierKey>();

        // #[cfg(feature = "gamepad")]
        app.register_buttonlike_input::<GamepadControlDirection>()
            .register_axislike_input::<GamepadControlAxis>()
            .register_dual_axislike_input::<GamepadStick>()
            .register_buttonlike_input::<GamepadButton>();

        // Virtual Axes
        app.register_axislike_input::<VirtualAxis>()
            .register_dual_axislike_input::<VirtualDPad>()
            .register_triple_axislike_input::<VirtualDPad3D>();

        // Chords
        app.register_buttonlike_input::<ButtonlikeChord>()
            .register_axislike_input::<AxislikeChord>()
            .register_dual_axislike_input::<DualAxislikeChord>()
            .register_triple_axislike_input::<TripleAxislikeChord>();

        // General-purpose reflection
        app.register_type::<ActionState<A>>()
            .register_type::<InputMap<A>>()
            .register_type::<ButtonData>()
            .register_type::<ActionState<A>>()
            .register_type::<CentralInputStore>();

        // Processors
        app.register_type::<AxisProcessor>()
            .register_type::<AxisBounds>()
            .register_type::<AxisExclusion>()
            .register_type::<AxisDeadZone>()
            .register_type::<DualAxisProcessor>()
            .register_type::<DualAxisInverted>()
            .register_type::<DualAxisSensitivity>()
            .register_type::<DualAxisBounds>()
            .register_type::<DualAxisExclusion>()
            .register_type::<DualAxisDeadZone>()
            .register_type::<CircleBounds>()
            .register_type::<CircleExclusion>()
            .register_type::<CircleDeadZone>();

        // Resources
        app.init_resource::<ClashStrategy>();
    }
}

/// [`SystemSet`]s for the input systems used by this crate
///
/// `Reset` must occur before `Update`
#[derive(SystemSet, Clone, Hash, Debug, PartialEq, Eq)]
pub enum InputManagerSystem {
    /// Advances action timers.
    ///
    /// Cleans up the state of the input manager, clearing `just_pressed` and `just_released`
    Tick,
    /// Collects input data to update the [`ActionState`]
    Update,
    /// Manually control the [`ActionState`]
    ///
    /// Must run after [`InputManagerSystem::Update`] or the action state will be overridden
    ManualControl,
}

#[cfg(test)]
mod tests;
