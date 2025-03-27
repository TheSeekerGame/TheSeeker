/// reimplementation of the leafwing_input_manager plugin to allow for input state update systems
/// in our custom fixed timestep
use core::hash::Hash;
use core::marker::PhantomData;
use std::fmt::Debug;

use bevy::app::{App, Plugin, Update};
use bevy::ecs::prelude::*;
use bevy::input::{ButtonState, InputSystem};
use bevy::prelude::{PostUpdate, PreUpdate};
use bevy::reflect::TypePath;
use leafwing_input_manager::action_state::{ActionData, ActionState};
use leafwing_input_manager::clashing_inputs::ClashStrategy;
use leafwing_input_manager::input_map::InputMap;
use leafwing_input_manager::input_processing::*;
use leafwing_input_manager::user_input::UserInput;
use leafwing_input_manager::Actionlike;

use crate::time::GameTickPost;
use crate::GameTickSet;

/// A [`Plugin`] that collects [`ButtonInput`](bevy::input::ButtonInput) from disparate sources,
/// producing an [`ActionState`] that can be conveniently checked
///
/// This plugin needs to be passed in an [`Actionlike`] enum type that you've created for your game.
/// Each variant represents a "virtual button" whose state is stored in an [`ActionState`] struct.
///
/// Each [`InputManagerBundle`](crate::InputManagerBundle) contains:
///  - an [`InputMap`] component, which stores an entity-specific mapping between the assorted input streams and an internal representation of "actions"
///  - an [`ActionState`] component, which stores the current input state for that entity in a source-agnostic fashion
///
/// If you have more than one distinct type of action (e.g., menu actions, camera actions, and player actions),
/// consider creating multiple `Actionlike` enums
/// and adding a copy of this plugin for each `Actionlike` type.
///
/// All actions can be dynamically enabled or disabled by calling the relevant methods on
/// `ActionState<A>`. This can be useful when working with states to pause the game, navigate
/// menus, and so on.
///
/// ## Systems
///
/// **WARNING:** These systems run during [`PreUpdate`].
/// If you have systems that care about inputs and actions that also run during this stage,
/// you must define an ordering between your systems or behavior will be very erratic.
/// The stable system sets for these systems are available under [`InputManagerSystem`] enum.
///
/// Complete list:
///
/// - [`tick_action_state`](crate::systems::tick_action_state), which resets the `pressed` and `just_pressed` fields of the [`ActionState`] each frame
/// - [`update_action_state`](crate::systems::update_action_state), which collects [`ButtonInput`](bevy::input::ButtonInput) resources to update the [`ActionState`]
/// - [`update_action_state_from_interaction`](crate::systems::update_action_state_from_interaction), for triggering actions from buttons
///    - powers the [`ActionStateDriver`](crate::action_driver::ActionStateDriver) component based on an [`Interaction`](bevy::ui::Interaction) component
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

impl<A: Actionlike + TypePath> Plugin for InputManagerPlugin<A> {
    fn build(&self, app: &mut App) {
        use leafwing_input_manager::systems::*;

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
            InputManagerSystem::Update.after(InputSystem),
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

        // app.register_type::<ActionState<A>>();
        // app.register_type::<InputMap<A>>();
        // app.register_type::<UserInput>();
        // app.register_type::<InputKind>();
        // app.register_type::<ActionData>();
        // app.register_type::<Modifier>();
        // app.register_type::<ActionState<A>>();
        // app.register_type::<VirtualDPad>();
        // app.register_type::<VirtualAxis>();
        // app.register_type::<SingleAxis>();
        // app.register_type::<DualAxis>();
        // app.register_type::<AxisType>();
        // app.register_type::<MouseWheelAxisType>();
        // app.register_type::<MouseMotionAxisType>();
        // app.register_type::<DualAxisData>();
        // app.register_type::<ButtonState>();
        // app.register_type::<MouseWheelDirection>();
        // app.register_type::<MouseMotionDirection>();
        // Processors
        // app.register_type::<AxisProcessor>();
        // app.register_type::<AxisBounds>();
        // app.register_type::<AxisExclusion>();
        // app.register_type::<AxisDeadZone>();
        // app.register_type::<DualAxisProcessor>();
        // app.register_type::<DualAxisInverted>();
        // app.register_type::<DualAxisSensitivity>();
        // app.register_type::<DualAxisBounds>();
        // app.register_type::<DualAxisExclusion>();
        // app.register_type::<DualAxisDeadZone>();
        // app.register_type::<CircleBounds>();
        // app.register_type::<CircleExclusion>();
        // app.register_type::<CircleDeadZone>();
        // Resources
        // app.init_resource::<ClashStrategy>();
    }
}

/// [`SystemSet`]s for the [`crate::systems`] used by this crate
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
