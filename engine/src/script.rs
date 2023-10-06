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

/// Resource to track when the current game level was entered/loaded
#[derive(Resource)]
pub struct LevelLoadTime {
    /// The time since app startup, when the level was spawned
    pub time: Duration,
    /// The GameTime Tick
    pub tick: u64,
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

pub type ActionId = usize;

pub trait ScriptAsset: Asset + Sized + Send + Sync + 'static {
    type Settings: Sized + Send + Sync + 'static;
    type RunIf: ScriptRunIf<Tracker = Self::Tracker>;
    type Action: ScriptAction<Tracker = Self::Tracker, ActionParams = Self::ActionParams>;
    type ActionParams: ScriptActionParams;
    type Tracker: ScriptTracker<RunIf = Self::RunIf, Settings = Self::Settings>;
    type BuildParam: SystemParam + 'static;

    fn build<'w>(
        &self,
        builder: ScriptRuntimeBuilder<Self>,
        entity: Entity,
        param: &mut <Self::BuildParam as SystemParam>::Item<'w, '_>,
    ) -> ScriptRuntimeBuilder<Self>;

    fn into_settings(&self) -> Self::Settings;
}

pub trait ScriptTracker: Default + Send + Sync + 'static {
    type RunIf: ScriptRunIf;
    type Settings: Sized + Send + Sync + 'static;
    type InitParam: SystemParam + 'static;
    type UpdateParam: SystemParam + 'static;

    fn init<'w>(
        &mut self,
        entity: Entity,
        settings: &Self::Settings,
        param: &mut <Self::InitParam as SystemParam>::Item<'w, '_>,
    );
    fn transfer_progress(&mut self, other: &Self);
    fn track_action(&mut self, run_if: &Self::RunIf, action_id: ActionId);
    fn finalize(&mut self);
    fn update<'w>(
        &mut self,
        entity: Entity,
        settings: &Self::Settings,
        param: &mut <Self::UpdateParam as SystemParam>::Item<'w, '_>,
        queue: &mut Vec<ActionId>,
    ) -> ScriptUpdateResult;
}

pub trait ScriptRunIf: Clone + Send + Sync + 'static {
    type Tracker: ScriptTracker;
}

pub trait ScriptAction: Clone + Send + Sync + 'static {
    type Tracker: ScriptTracker;
    type ActionParams: ScriptActionParams;
    type Param: SystemParam + 'static;
    fn run<'w>(
        &self,
        entity: Entity,
        actionparams: &Self::ActionParams,
        tracker: &mut Self::Tracker,
        param: &mut <Self::Param as SystemParam>::Item<'w, '_>,
    ) -> ScriptUpdateResult;
}

pub trait ScriptActionParams: Clone + Send + Sync + 'static {
    fn should_run(&self) -> Result<(), ScriptUpdateResult> {
        Ok(())
    }
}

/// Returned by `ScriptTracker::update` to indicate the status of a script
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ScriptUpdateResult {
    /// Nothing unusual
    NormalRun,
    /// There may be more actions to run; do another update
    Loop,
    /// The script is done, no actions remain that can be run ever in the future
    Finished,
    /// The script wants to be forcefully finished, regardless of remaining actions
    Terminated,
}

impl ScriptUpdateResult {
    pub fn is_loop(self) -> bool {
        self == ScriptUpdateResult::Loop
    }

    pub fn is_end(self) -> bool {
        self == ScriptUpdateResult::Finished || self == ScriptUpdateResult::Terminated
    }
}

pub struct ScriptRuntimeBuilder<T: ScriptAsset> {
    runtime: ScriptRuntime<T>,
}

#[derive(Component)]
pub struct ScriptRuntime<T: ScriptAsset> {
    settings: <T::Tracker as ScriptTracker>::Settings,
    actions: Vec<(T::ActionParams, T::Action)>,
    tracker: T::Tracker,
}

impl<T: ScriptAsset> ScriptRuntimeBuilder<T> {
    pub fn new<'w>(
        entity: Entity,
        settings: <T::Tracker as ScriptTracker>::Settings,
        param: &mut <<T::Tracker as ScriptTracker>::InitParam as SystemParam>::Item<'w, '_>,
    ) -> Self {
        let mut tracker = T::Tracker::default();
        tracker.init(entity, &settings, param);
        ScriptRuntimeBuilder {
            runtime: ScriptRuntime {
                settings,
                actions: vec![],
                tracker,
            },
        }
    }

    pub fn add_action(
        mut self,
        run_if: &T::RunIf,
        action: &T::Action,
        params: &T::ActionParams,
    ) -> Self {
        let action_id = self.runtime.actions.len();
        self.runtime.actions.push((params.clone(), action.clone()));
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
    mut params: ParamSet<(
        StaticSystemParam<<T::Tracker as ScriptTracker>::UpdateParam>,
        StaticSystemParam<<T::Action as ScriptAction>::Param>,
    )>,
    mut action_queue: Local<Vec<ActionId>>,
) {
    for (e, script_rt) in &mut q_script {
        let script_rt = script_rt.into_inner();

        let mut is_loop = true;
        let mut is_end = false;
        while is_loop {
            is_loop = false;
            {
                let mut tracker_param = params.p0().into_inner();
                let settings = &script_rt.settings;
                let tracker = &mut script_rt.tracker;
                let r = tracker.update(
                    e,
                    settings,
                    &mut tracker_param,
                    &mut action_queue,
                );
                is_loop |= r.is_loop();
                is_end |= r.is_end();
            }
            // trace!(
            //     "Script actions to run: {}",
            //     action_queue.len(),
            // );
            for action_id in action_queue.drain(..) {
                let action = &script_rt.actions[action_id];
                let r = if let Err(r) = action.0.should_run() {
                    r
                } else {
                    let mut action_param = params.p1().into_inner();
                    script_rt.actions[action_id].1.run(
                        e,
                        &script_rt.actions[action_id].0,
                        &mut script_rt.tracker,
                        &mut action_param,
                    )
                };
                is_loop |= r.is_loop();
                is_end |= r.is_end();
            }
        }
        if is_end {
            commands.entity(e).despawn_recursive();
        }
    }
}

fn script_init_system<T: ScriptAsset>(
    mut commands: Commands,
    ass_script: Res<Assets<T>>,
    q_script_handle: Query<(Entity, &Handle<T>), Changed<Handle<T>>>,
    mut params: ParamSet<(
        StaticSystemParam<<T::Tracker as ScriptTracker>::InitParam>,
        StaticSystemParam<T::BuildParam>,
    )>,
) {
    for (e, handle) in &q_script_handle {
        if let Some(script) = ass_script.get(&handle) {
            let settings = script.into_settings();
            let builder = {
                let mut tracker_init_param = params.p0().into_inner();
                ScriptRuntimeBuilder::new(e, settings, &mut tracker_init_param)
            };
            let builder = {
                let mut build_param = params.p1().into_inner();
                script.build(builder, e, &mut build_param)
            };
            commands.entity(e).insert(builder.build());
            debug!("Initialized new script.");
        }
    }
}
