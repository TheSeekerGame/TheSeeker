use bevy::asset::Asset;
use bevy::ecs::system::{StaticSystemParam, SystemParam};

use crate::prelude::*;

pub mod common;
pub mod label;

pub struct ScriptPlugin;

impl Plugin for ScriptPlugin {
    fn build(&self, app: &mut App) {
        app.configure_sets(
            GameTickUpdate,
            (
                ScriptSet::Init.after(AssetsSet::ResolveKeysFlush),
                ScriptSet::InitFlush.after(ScriptSet::Init),
                ScriptSet::Run.after(ScriptSet::InitFlush),
                ScriptSet::RunFlush.after(ScriptSet::Run),
            ),
        );
        app.add_systems(
            GameTickUpdate,
            (
                apply_deferred.in_set(ScriptSet::InitFlush),
                apply_deferred.in_set(ScriptSet::RunFlush),
            ),
        );
        app.add_plugins((
            self::label::ScriptLabelPlugin,
            self::common::CommonScriptPlugin,
        ));
    }
}

/// Use this for system ordering relative to scripts
/// (within the `GameTickUpdate` schedule)
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ScriptSet {
    /// This is when scripts get initialized (the `ScriptRuntime` component added to entities)
    Init,
    InitFlush,
    /// This is when scripts get run/updated
    Run,
    RunFlush,
}

pub trait ScriptAppExt {
    fn add_script_runtime<T: ScriptAsset>(&mut self) -> &mut Self;
}

impl ScriptAppExt for App {
    fn add_script_runtime<T: ScriptAsset>(&mut self) -> &mut Self {
        self.add_systems(
            GameTickUpdate,
            (
                script_init_system::<T>.in_set(ScriptSet::Init),
                script_driver_system::<T>.in_set(ScriptSet::Run),
            ),
        );
        self
    }
}

type ActionId = usize;

pub trait ScriptAsset: Asset + Sized + Send + Sync + 'static {
    type Action: ScriptAction;
    type RunIf: ScriptRunIf<Tracker = Self::Tracker>;
    type Tracker: ScriptTracker<RunIf = Self::RunIf>;

    fn init(&self) -> ScriptRuntime<Self>;
}

pub trait ScriptTracker: Default + Send + Sync + 'static {
    type RunIf: ScriptRunIf;
    type Param: SystemParam + 'static;

    fn track_action(&mut self, run_if: &Self::RunIf, action_id: ActionId);
    fn finalize(&mut self);
    fn update<'w>(
        &mut self,
        entity: Entity,
        param: &mut <Self::Param as SystemParam>::Item<'w, '_>,
        queue: &mut Vec<ActionId>,
    ) -> ScriptUpdateResult;
}

pub trait ScriptRunIf: Clone + Send + Sync + 'static {
    type Tracker: ScriptTracker;
}

pub trait ScriptAction: Clone + Send + Sync + 'static {
    type Param: SystemParam + 'static;
    fn run<'w>(&self, entity: Entity, param: &mut <Self::Param as SystemParam>::Item<'w, '_>);
}

/// Returned by `ScriptTracker::update` to indicate the status of a script
pub enum ScriptUpdateResult {
    /// Nothing unusual
    NormalRun,
    /// The script is done, no actions remain that can be run ever in the future
    Finished,
    /// The script wants to be forcefully finished, regardless of remaining actions
    Terminated,
}

pub struct ScriptRuntimeBuilder<T: ScriptAsset> {
    runtime: ScriptRuntime<T>,
}

#[derive(Component)]
pub struct ScriptRuntime<T: ScriptAsset> {
    actions: Vec<T::Action>,
    tracker: T::Tracker,
}

impl<T: ScriptAsset> ScriptRuntimeBuilder<T> {
    pub fn new() -> Self {
        Self {
            runtime: ScriptRuntime {
                actions: Default::default(),
                tracker: Default::default(),
            },
        }
    }

    pub fn add_action(mut self, run_if: &T::RunIf, action: &T::Action) -> Self {
        let action_id = self.runtime.actions.len();
        self.runtime.actions.push(action.clone());
        self.runtime.tracker.track_action(run_if, action_id);
        self
    }

    pub fn build(mut self) -> ScriptRuntime<T> {
        self.runtime.tracker.finalize();
        self.runtime
    }
}

fn script_driver_system<T: ScriptAsset>(
    mut commands: Commands,
    mut q_script: Query<(Entity, &mut ScriptRuntime<T>)>,
    tracker_param: StaticSystemParam<<T::Tracker as ScriptTracker>::Param>,
    action_param: StaticSystemParam<<T::Action as ScriptAction>::Param>,
    mut action_queue: Local<Vec<ActionId>>,
) {
    let mut tracker_param = tracker_param.into_inner();
    let mut action_param = action_param.into_inner();
    for (e, mut script_rt) in &mut q_script {
        dbg!("blah");
        let r = script_rt
            .tracker
            .update(e, &mut tracker_param, &mut action_queue);
        trace!(
            "Script actions to run: {}",
            action_queue.len()
        );
        for action_id in action_queue.drain(..) {
            script_rt.actions[action_id].run(e, &mut action_param);
        }
        match r {
            ScriptUpdateResult::NormalRun => {},
            ScriptUpdateResult::Finished | ScriptUpdateResult::Terminated => {
                commands.entity(e).despawn_recursive();
            },
        }
    }
}

fn script_init_system<T: ScriptAsset>(
    mut commands: Commands,
    ass_script: Res<Assets<T>>,
    q_script_handle: Query<(Entity, &Handle<T>), Without<ScriptRuntime<T>>>,
) {
    for (e, handle) in &q_script_handle {
        if let Some(script) = ass_script.get(&handle) {
            commands.entity(e).insert(script.init());
            debug!("Initialized new script.");
        }
    }
}
