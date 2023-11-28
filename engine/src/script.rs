use std::marker::PhantomData;

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
        self.init_resource::<ScriptActionQueue<T>>();
        self.add_systems(
            GameTickUpdate,
            (
                (script_changeover_system::<T>.in_set(ScriptSet::Init),
                script_init_system::<T>.in_set(ScriptSet::Init)).chain(),
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
    type ActionParams: ScriptActionParams<Tracker = Self::Tracker>;
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
    fn set_slot(&mut self, _slot: &str, _state: bool) {}
    fn take_slots(&mut self) -> HashSet<String> {
        Default::default()
    }
    fn clear_slots(&mut self) {}
    fn do_start<'w>(
        &mut self,
        _entity: Entity,
        _settings: &Self::Settings,
        _param: &mut <Self::UpdateParam as SystemParam>::Item<'w, '_>,
        _queue: &mut Vec<ActionId>,
    ) {}
    fn do_stop<'w>(
        &mut self,
        _entity: Entity,
        _settings: &Self::Settings,
        _param: &mut <Self::UpdateParam as SystemParam>::Item<'w, '_>,
        _queue: &mut Vec<ActionId>,
    ) {}
}

pub trait ScriptRunIf: Clone + Send + Sync + 'static {
    type Tracker: ScriptTracker;
}

pub trait ScriptAction: Clone + Send + Sync + 'static {
    type Tracker: ScriptTracker;
    type ActionParams: ScriptActionParams<Tracker = Self::Tracker>;
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
    type Tracker: ScriptTracker;
    type ShouldRunParam: SystemParam + 'static;

    fn should_run<'w>(
        &self,
        _tracker: &mut Self::Tracker,
        _action_id: ActionId,
        _param: &mut <Self::ShouldRunParam as SystemParam>::Item<'w, '_>,
    ) -> Result<(), ScriptUpdateResult> {
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

struct ScriptRuntime<T: ScriptAsset> {
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

    fn build(mut self) -> ScriptRuntime<T> {
        self.runtime.tracker.finalize();
        self.runtime
    }
}

#[derive(Resource)]
struct ScriptActionQueue<T: ScriptAsset>(Vec<ActionId>, PhantomData<T>);

impl<T: ScriptAsset> Default for ScriptActionQueue<T> {
    fn default() -> Self {
        ScriptActionQueue(Vec::new(), PhantomData)
    }
}

fn script_changeover_system<T: ScriptAsset>(
    mut q_script: Query<(Entity, &mut ScriptPlayer<T>)>,
    mut params: ParamSet<(
        StaticSystemParam<<T::Tracker as ScriptTracker>::UpdateParam>,
        StaticSystemParam<<T::Action as ScriptAction>::Param>,
        StaticSystemParam<<T::ActionParams as ScriptActionParams>::ShouldRunParam>,
    )>,
    mut action_queue: ResMut<ScriptActionQueue<T>>,
) {
    for (e, player) in &mut q_script {
        let player = player.into_inner();
        let mut old_state = std::mem::replace(&mut player.state, ScriptPlayerState::Stopped);
        let script_rt = match &mut old_state {
            ScriptPlayerState::ChangingKey { ref mut old_runtime, .. } |
            ScriptPlayerState::ChangingHandle { ref mut old_runtime, .. } => {
                {
                    let mut tracker_param = params.p0().into_inner();
                    old_runtime.tracker.do_stop(
                        e,
                        &old_runtime.settings,
                        &mut tracker_param,
                        &mut action_queue.0,
                    );
                }
                old_runtime
            }
            _ => {
                player.state = old_state;
                continue;
            }
        };
        // trace!(
        //     "Script actions to run: {}",
        //     action_queue.len(),
        // );
        for action_id in action_queue.0.drain(..) {
            let action = &script_rt.actions[action_id];
            {
                let mut shouldrun_param = params.p2().into_inner();
                action.0.should_run(&mut script_rt.tracker, action_id, &mut shouldrun_param)
            }.err().unwrap_or_else(|| {
                let mut action_param = params.p1().into_inner();
                script_rt.actions[action_id].1.run(
                    e,
                    &script_rt.actions[action_id].0,
                    &mut script_rt.tracker,
                    &mut action_param,
                )
            });
        }
        player.state = match old_state {
            ScriptPlayerState::ChangingHandle { handle, old_runtime } => {
                ScriptPlayerState::PrePlayHandle { handle, old_runtime: Some(old_runtime) }
            }
            ScriptPlayerState::ChangingKey { key, old_runtime } => {
                ScriptPlayerState::PrePlayKey { key, old_runtime: Some(old_runtime) }
            }
            _ => old_state
        }
    }
}

fn script_driver_system<T: ScriptAsset>(
    mut q_script: Query<(Entity, &mut ScriptPlayer<T>)>,
    mut params: ParamSet<(
        StaticSystemParam<<T::Tracker as ScriptTracker>::UpdateParam>,
        StaticSystemParam<<T::Action as ScriptAction>::Param>,
        StaticSystemParam<<T::ActionParams as ScriptActionParams>::ShouldRunParam>,
    )>,
    mut action_queue: ResMut<ScriptActionQueue<T>>,
) {
    'outer: for (e, player) in &mut q_script {
        let player = player.into_inner();

        let mut is_loop = true;
        let mut is_end = false;
        while is_loop {
            is_loop = false;
            // do the borrow checker dance ;)
            let mut old_state = std::mem::replace(&mut player.state, ScriptPlayerState::Stopped);
            // enqueue actions
            let script_rt = match &mut old_state {
                ScriptPlayerState::Starting { ref mut runtime } => {
                    {
                        let mut tracker_param = params.p0().into_inner();
                        runtime.tracker.do_start(
                            e,
                            &runtime.settings,
                            &mut tracker_param,
                            &mut action_queue.0,
                        );
                    }
                    runtime
                }
                ScriptPlayerState::Playing { ref mut runtime } => {
                    {
                        let mut tracker_param = params.p0().into_inner();
                        let r = runtime.tracker.update(
                            e,
                            &runtime.settings,
                            &mut tracker_param,
                            &mut action_queue.0,
                        );
                        is_loop |= r.is_loop();
                        is_end |= r.is_end();
                    }
                    runtime
                }
                ScriptPlayerState::Stopping { ref mut runtime } => {
                    {
                        let mut tracker_param = params.p0().into_inner();
                        runtime.tracker.do_stop(
                            e,
                            &runtime.settings,
                            &mut tracker_param,
                            &mut action_queue.0,
                        );
                    }
                    runtime
                }
                _ => {
                    player.state = old_state;
                    continue 'outer;
                }
            };
            // trace!(
            //     "Script actions to run: {}",
            //     action_queue.len(),
            // );
            for action_id in action_queue.0.drain(..) {
                let action = &script_rt.actions[action_id];
                let r = {
                    let mut shouldrun_param = params.p2().into_inner();
                    action.0.should_run(&mut script_rt.tracker, action_id, &mut shouldrun_param)
                }.err().unwrap_or_else(|| {
                    let mut action_param = params.p1().into_inner();
                    script_rt.actions[action_id].1.run(
                        e,
                        &script_rt.actions[action_id].0,
                        &mut script_rt.tracker,
                        &mut action_param,
                    )
                });
                is_loop |= r.is_loop();
                is_end |= r.is_end();
            }
            // put back the correct state
            player.state = match old_state {
                ScriptPlayerState::Starting { runtime } => {
                    ScriptPlayerState::Playing { runtime }
                }
                ScriptPlayerState::Playing { runtime } => {
                    ScriptPlayerState::Playing { runtime }
                }
                ScriptPlayerState::Stopping { runtime: _ } => {
                    ScriptPlayerState::Stopped
                }
                _ => {
                    old_state
                }
            };
            if is_end && !is_loop {
                player.stop();
            }
        }
    }
}

fn script_init_system<T: ScriptAsset>(
    preloaded: Res<PreloadedAssets>,
    ass_script: Res<Assets<T>>,
    mut q_script: Query<(Entity, &mut ScriptPlayer<T>)>,
    mut params: ParamSet<(
        StaticSystemParam<<T::Tracker as ScriptTracker>::InitParam>,
        StaticSystemParam<T::BuildParam>,
    )>,
) {
    for (e, mut player) in &mut q_script {
        let handle = match &player.state {
            ScriptPlayerState::PrePlayHandle { handle, .. } => {
                handle.clone()
            }
            ScriptPlayerState::PrePlayKey { key, .. } => {
                if let Some(handle) = preloaded.get_single_asset(&key) {
                    handle
                } else {
                    continue;
                }
            },
            _ => continue,
        };
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
            let mut runtime = builder.build();
            // transfer slots
            {
                let old_state = std::mem::replace(&mut player.state, ScriptPlayerState::Stopped);
                let old_runtime = match old_state {
                    ScriptPlayerState::PrePlayHandle { old_runtime, .. } => old_runtime,
                    ScriptPlayerState::PrePlayKey { old_runtime, .. } => old_runtime,
                    _ => None,
                };
                let slots = old_runtime.map(|mut rt| rt.tracker.take_slots()).unwrap_or_default();
                for slot in slots.iter() {
                    runtime.tracker.set_slot(slot, true);
                }
            }
            player.state = ScriptPlayerState::Starting {
                runtime,
            };
        }
    }
}

#[derive(Component)]
pub struct ScriptPlayer<T: ScriptAsset> {
    state: ScriptPlayerState<T>,
}

enum ScriptPlayerState<T: ScriptAsset> {
    Stopped,
    PrePlayHandle {
        handle: Handle<T>,
        old_runtime: Option<ScriptRuntime<T>>,
    },
    PrePlayKey {
        key: String,
        old_runtime: Option<ScriptRuntime<T>>,
    },
    Starting {
        runtime: ScriptRuntime<T>,
    },
    Playing {
        runtime: ScriptRuntime<T>,
    },
    Stopping {
        runtime: ScriptRuntime<T>,
    },
    ChangingHandle {
        handle: Handle<T>,
        old_runtime: ScriptRuntime<T>,
    },
    ChangingKey {
        key: String,
        old_runtime: ScriptRuntime<T>,
    },
}

impl<T: ScriptAsset> Default for ScriptPlayer<T> {
    fn default() -> Self {
        Self {
            state: ScriptPlayerState::Stopped,
        }
    }
}

impl<T: ScriptAsset> ScriptPlayer<T> {
    pub fn new() -> Self {
        Self {
            state: ScriptPlayerState::Stopped,
        }
    }
    pub fn is_stopped(&self) -> bool {
        if let ScriptPlayerState::Stopped = self.state {
            true
        } else {
            false
        }
    }
    pub fn play_handle(&mut self, script: Handle<T>) {
        let old_state = std::mem::replace(&mut self.state, ScriptPlayerState::Stopped);
        self.state = match old_state {
            ScriptPlayerState::Playing { runtime } => {
                ScriptPlayerState::ChangingHandle {
                    handle: script,
                    old_runtime: runtime,
                }
            }
            ScriptPlayerState::Starting { runtime } => {
                ScriptPlayerState::PrePlayHandle {
                    handle: script,
                    old_runtime: Some(runtime),
                }
            }
            ScriptPlayerState::Stopping { runtime } => {
                ScriptPlayerState::ChangingHandle {
                    handle: script,
                    old_runtime: runtime,
                }
            }
            _ => {
                ScriptPlayerState::PrePlayHandle {
                    handle: script,
                    old_runtime: None,
                }
            },
        };
    }
    pub fn play_key(&mut self, key: &str) {
        let old_state = std::mem::replace(&mut self.state, ScriptPlayerState::Stopped);
        self.state = match old_state {
            ScriptPlayerState::Playing { runtime } => {
                ScriptPlayerState::ChangingKey {
                    key: key.into(),
                    old_runtime: runtime,
                }
            }
            ScriptPlayerState::Starting { runtime } => {
                ScriptPlayerState::PrePlayKey {
                    key: key.into(),
                    old_runtime: Some(runtime),
                }
            }
            ScriptPlayerState::Stopping { runtime } => {
                ScriptPlayerState::ChangingKey {
                    key: key.into(),
                    old_runtime: runtime,
                }
            }
            _ => {
                ScriptPlayerState::PrePlayKey {
                    key: key.into(),
                    old_runtime: None,
                }
            },
        };
    }
    pub fn stop(&mut self) {
        let old_state = std::mem::replace(&mut self.state, ScriptPlayerState::Stopped);
        self.state = match old_state {
            ScriptPlayerState::Playing { runtime } => ScriptPlayerState::Stopping { runtime },
            _ => ScriptPlayerState::Stopped,
        }
    }
    pub fn clear_slots(&mut self) {
        match &mut self.state {
            ScriptPlayerState::Playing { ref mut runtime } => {
                runtime.tracker.clear_slots();
            }
            ScriptPlayerState::Starting { ref mut runtime } => {
                runtime.tracker.clear_slots();
            }
            ScriptPlayerState::Stopping { ref mut runtime } => {
                runtime.tracker.clear_slots();
            }
            ScriptPlayerState::PrePlayHandle { old_runtime: Some(ref mut old_runtime), .. } => {
                old_runtime.tracker.clear_slots();
            }
            ScriptPlayerState::PrePlayKey { old_runtime: Some(ref mut old_runtime), .. } => {
                old_runtime.tracker.clear_slots();
            }
            ScriptPlayerState::ChangingHandle { old_runtime, .. } => {
                old_runtime.tracker.clear_slots();
            }
            ScriptPlayerState::ChangingKey { old_runtime, .. } => {
                old_runtime.tracker.clear_slots();
            }
            _ => {}
        }
    }
    pub fn set_slot(&mut self, slot: &str, state: bool) {
        match &mut self.state {
            ScriptPlayerState::Playing { ref mut runtime } => {
                runtime.tracker.set_slot(slot, state);
            }
            ScriptPlayerState::Starting { ref mut runtime } => {
                runtime.tracker.set_slot(slot, state);
            }
            ScriptPlayerState::Stopping { ref mut runtime } => {
                runtime.tracker.set_slot(slot, state);
            }
            ScriptPlayerState::PrePlayHandle { old_runtime: Some(ref mut old_runtime), .. } => {
                old_runtime.tracker.set_slot(slot, state);
            }
            ScriptPlayerState::PrePlayKey { old_runtime: Some(ref mut old_runtime), .. } => {
                old_runtime.tracker.set_slot(slot, state);
            }
            ScriptPlayerState::ChangingHandle { old_runtime, .. } => {
                old_runtime.tracker.set_slot(slot, state);
            }
            ScriptPlayerState::ChangingKey { old_runtime, .. } => {
                old_runtime.tracker.set_slot(slot, state);
            }
            _ => {}
        }
    }
}
