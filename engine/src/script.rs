use std::marker::PhantomData;

use bevy::asset::Asset;
use bevy::ecs::system::{StaticSystemParam, SystemParam};

use crate::assets::config::DynamicConfigValue;
use crate::assets::script::ScriptConfig;
use crate::prelude::*;

pub mod common;
pub mod label;

pub struct ScriptPlugin;

impl Plugin for ScriptPlugin {
    fn build(&self, app: &mut App) {
        app.configure_sets(
            GameTickUpdate,
            (
                ScriptSet::Init.after(AssetsSet::ResolveKeys),
                ScriptSet::Run.after(ScriptSet::Init),
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
    /// This is when scripts get run/updated
    Run,
}

/// Resource to track when the current game level was entered/loaded
#[derive(Resource)]
pub struct LevelLoadTime {
    /// The time since app startup, when the level was spawned
    pub time: Duration,
    /// The GameTime Tick
    pub tick: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ScriptActionTiming {
    Unknown,
    UnknownTick,
    Time(Duration),
    Tick(u64),
}

pub trait ScriptAppExt {
    fn add_script_runtime<T: ScriptAsset>(&mut self) -> &mut Self;
}

impl ScriptAppExt for App {
    fn add_script_runtime<T: ScriptAsset>(&mut self) -> &mut Self {
        self.init_resource::<ScriptActionQueue<T>>();
        self.init_resource::<ScriptRunCounts<T>>();
        self.add_systems(
            GameTickUpdate,
            (
                (
                    script_changeover_system::<T>.in_set(ScriptSet::Init),
                    script_init_system::<T>.in_set(ScriptSet::Init),
                )
                    .chain(),
                script_driver_system::<T>.in_set(ScriptSet::Run),
            ),
        );
        self
    }
}

#[derive(Debug, Default)]
pub struct ScriptMetadata {
    pub key: Option<String>,
    pub key_previous: Option<String>,
    pub runcount: u32,
}

pub type ActionId = usize;

pub trait ScriptAsset: Asset + Sized + Send + Sync + 'static {
    type Settings: Sized + Send + Sync + 'static;
    type RunIf: ScriptRunIf<Tracker = Self::Tracker>;
    type Action: ScriptAction<Tracker = Self::Tracker, ActionParams = Self::ActionParams>;
    type ActionParams: ScriptActionParams<Tracker = Self::Tracker>;
    type Tracker: ScriptTracker<
            RunIf = Self::RunIf,
            Settings = Self::Settings,
            ActionParams = Self::ActionParams,
        >;
    type BuildParam: SystemParam + 'static;

    fn build(
        &self,
        builder: ScriptRuntimeBuilder<Self>,
        entity: Entity,
        param: &mut <Self::BuildParam as SystemParam>::Item<'_, '_>,
    ) -> ScriptRuntimeBuilder<Self>;

    fn into_settings(&self) -> Self::Settings;
}

pub trait ScriptTracker: Default + Send + Sync + 'static {
    type RunIf: ScriptRunIf;
    type Settings: Sized + Send + Sync + 'static;
    type InitParam: SystemParam + 'static;
    type UpdateParam: SystemParam + 'static;
    type CarryoverParam: SystemParam + 'static;
    type ActionParams: ScriptActionParams<Tracker = Self>;
    type Carryover: Default + Sized + Send + Sync + 'static;

    fn init(
        &mut self,
        entity: Entity,
        settings: &Self::Settings,
        metadata: &ScriptMetadata,
        carryover: Self::Carryover,
        param: &mut <Self::InitParam as SystemParam>::Item<'_, '_>,
    );
    fn transfer_progress(&mut self, other: &Self);
    fn track_action(
        &mut self,
        run_if: &Self::RunIf,
        params: &Self::ActionParams,
        action_id: ActionId,
    );
    fn finalize(&mut self);
    fn update(
        &mut self,
        entity: Entity,
        settings: &Self::Settings,
        param: &mut <Self::UpdateParam as SystemParam>::Item<'_, '_>,
        queue: &mut Vec<QueuedAction>,
    ) -> ScriptUpdateResult;
    fn queue_extra_actions(
        &mut self,
        _settings: &Self::Settings,
        _queue: &mut Vec<QueuedAction>,
    ) {
    }
    fn set_slot(
        &mut self,
        _timing: ScriptActionTiming,
        _slot: &str,
        _state: bool,
    ) {
    }
    fn has_slot(&self, _slot: &str) -> bool {
        false
    }
    fn take_slots(&mut self, _timing: ScriptActionTiming) -> HashSet<String> {
        Default::default()
    }
    fn clear_slots(&mut self, _timing: ScriptActionTiming) {}
    fn do_start(
        &mut self,
        _entity: Entity,
        _settings: &Self::Settings,
        _param: &mut <Self::UpdateParam as SystemParam>::Item<'_, '_>,
        _queue: &mut Vec<QueuedAction>,
    ) {
    }
    fn do_stop(
        &mut self,
        _entity: Entity,
        _settings: &Self::Settings,
        _param: &mut <Self::UpdateParam as SystemParam>::Item<'_, '_>,
        _queue: &mut Vec<QueuedAction>,
    ) {
    }
    fn produce_carryover(
        &self,
        entity: Entity,
        param: &mut <Self::CarryoverParam as SystemParam>::Item<'_, '_>,
    ) -> Self::Carryover;
}

pub trait ScriptRunIf: Clone + Send + Sync + 'static {
    type Tracker: ScriptTracker;
}

pub trait ScriptAction: Clone + Send + Sync + 'static {
    type Tracker: ScriptTracker;
    type ActionParams: ScriptActionParams<Tracker = Self::Tracker>;
    type Param: SystemParam + 'static;
    fn run(
        &self,
        entity: Entity,
        timing: ScriptActionTiming,
        actionparams: &Self::ActionParams,
        tracker: &mut Self::Tracker,
        param: &mut <Self::Param as SystemParam>::Item<'_, '_>,
    ) -> ScriptUpdateResult;
}

pub trait ScriptActionParams: Clone + Send + Sync + 'static {
    type Tracker: ScriptTracker;
    type ShouldRunParam: SystemParam + 'static;

    fn should_run(
        &self,
        _entity: Entity,
        _tracker: &mut Self::Tracker,
        _action_id: ActionId,
        _param: &mut <Self::ShouldRunParam as SystemParam>::Item<'_, '_>,
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
        self == ScriptUpdateResult::Finished
            || self == ScriptUpdateResult::Terminated
    }
}

#[derive(Resource)]
pub struct ScriptRunCounts<T: ScriptAsset> {
    counts: HashMap<AssetId<T>, u32>,
}

impl<T: ScriptAsset> Default for ScriptRunCounts<T> {
    fn default() -> Self {
        Self {
            counts: Default::default(),
        }
    }
}

pub struct ScriptRuntimeBuilder<T: ScriptAsset> {
    runtime: ScriptRuntime<T>,
}

struct ScriptRuntime<T: ScriptAsset> {
    key: Option<String>,
    config: ScriptConfig,
    settings: <T::Tracker as ScriptTracker>::Settings,
    actions: Vec<(T::ActionParams, T::Action)>,
    tracker: T::Tracker,
}

impl<T: ScriptAsset> ScriptRuntimeBuilder<T> {
    pub fn new(
        entity: Entity,
        settings: <T::Tracker as ScriptTracker>::Settings,
        metadata: &ScriptMetadata,
        carryover: <T::Tracker as ScriptTracker>::Carryover,
        param: &mut <<T::Tracker as ScriptTracker>::InitParam as SystemParam>::Item<'_, '_>,
    ) -> Self {
        let mut tracker = T::Tracker::default();
        tracker.init(
            entity, &settings, metadata, carryover, param,
        );
        ScriptRuntimeBuilder {
            runtime: ScriptRuntime {
                key: metadata.key.clone(),
                config: ScriptConfig(Default::default()),
                settings,
                actions: vec![],
                tracker,
            },
        }
    }

    pub fn asset_key(&self) -> Option<&str> {
        self.runtime.key.as_ref().map(|x| x.as_str())
    }

    pub fn replace_config(&mut self, config: &ScriptConfig) {
        self.runtime.config = config.clone();
    }

    pub fn add_action(
        mut self,
        run_if: &T::RunIf,
        action: &T::Action,
        params: &T::ActionParams,
    ) -> Self {
        let action_id = self.runtime.actions.len();
        self.runtime.actions.push((params.clone(), action.clone()));
        self.runtime.tracker.track_action(run_if, params, action_id);
        self
    }

    pub fn tracker(&self) -> &T::Tracker {
        &self.runtime.tracker
    }

    pub fn tracker_mut(&mut self) -> &mut T::Tracker {
        &mut self.runtime.tracker
    }

    pub fn tracker_do(mut self, f: impl FnOnce(&mut T::Tracker)) -> Self {
        f(self.tracker_mut());
        self
    }

    fn build(mut self) -> ScriptRuntime<T> {
        self.runtime.tracker.finalize();
        self.runtime
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QueuedAction {
    pub timing: ScriptActionTiming,
    pub action: ActionId,
}

#[derive(Resource)]
struct ScriptActionQueue<T: ScriptAsset>(Vec<QueuedAction>, PhantomData<T>);

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
        StaticSystemParam<
            <T::ActionParams as ScriptActionParams>::ShouldRunParam,
        >,
    )>,
    mut action_queue: ResMut<ScriptActionQueue<T>>,
) {
    for (e, player) in &mut q_script {
        let player = player.into_inner();
        let mut state = std::mem::replace(
            &mut player.state,
            ScriptPlayerState::Stopped,
        );
        let script_rt = match &mut state {
            ScriptPlayerState::ChangingKey { runtime, .. }
            | ScriptPlayerState::ChangingHandle { runtime, .. } => {
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
            },
            _ => {
                player.state = state;
                continue;
            },
        };
        // Need to sort to ensure actions run in the order they were
        // originally defined in the script asset.
        action_queue.0.sort_unstable_by_key(|qa| qa.action);
        for qa in action_queue.0.drain(..) {
            let action = &script_rt.actions[qa.action];
            {
                let mut shouldrun_param = params.p2().into_inner();
                action.0.should_run(
                    e,
                    &mut script_rt.tracker,
                    qa.action,
                    &mut shouldrun_param,
                )
            }
            .err()
            .unwrap_or_else(|| {
                let mut action_param = params.p1().into_inner();
                script_rt.actions[qa.action].1.run(
                    e,
                    qa.timing,
                    &script_rt.actions[qa.action].0,
                    &mut script_rt.tracker,
                    &mut action_param,
                )
            });
        }
        player.state = match state {
            ScriptPlayerState::ChangingHandle { handle, runtime } => {
                ScriptPlayerState::PrePlayHandle {
                    handle,
                    runtime: Some(runtime),
                }
            },
            ScriptPlayerState::ChangingKey { key, runtime } => {
                ScriptPlayerState::PrePlayKey {
                    key,
                    runtime: Some(runtime),
                }
            },
            _ => state,
        }
    }
}

fn script_driver_system<T: ScriptAsset>(
    mut q_script: Query<(Entity, &mut ScriptPlayer<T>)>,
    mut params: ParamSet<(
        StaticSystemParam<<T::Tracker as ScriptTracker>::UpdateParam>,
        StaticSystemParam<<T::Action as ScriptAction>::Param>,
        StaticSystemParam<
            <T::ActionParams as ScriptActionParams>::ShouldRunParam,
        >,
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
            let mut state = std::mem::replace(
                &mut player.state,
                ScriptPlayerState::Stopped,
            );
            // enqueue actions
            let script_rt = match &mut state {
                ScriptPlayerState::Starting { runtime } => {
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
                },
                ScriptPlayerState::Playing { runtime } => {
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
                },
                ScriptPlayerState::Stopping { runtime } => {
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
                },
                _ => {
                    player.state = state;
                    continue 'outer;
                },
            };
            loop {
                script_rt.tracker.queue_extra_actions(
                    &script_rt.settings,
                    &mut action_queue.0,
                );
                if action_queue.0.is_empty() {
                    break;
                }
                // Need to sort to ensure actions run in the order they were
                // originally defined in the script asset.
                action_queue.0.sort_unstable_by_key(|qa| qa.action);
                for qa in action_queue.0.drain(..) {
                    let action = &script_rt.actions[qa.action];
                    let r = {
                        let mut shouldrun_param = params.p2().into_inner();
                        action.0.should_run(
                            e,
                            &mut script_rt.tracker,
                            qa.action,
                            &mut shouldrun_param,
                        )
                    }
                    .err()
                    .unwrap_or_else(|| {
                        let mut action_param = params.p1().into_inner();
                        script_rt.actions[qa.action].1.run(
                            e,
                            qa.timing,
                            &script_rt.actions[qa.action].0,
                            &mut script_rt.tracker,
                            &mut action_param,
                        )
                    });
                    is_loop |= r.is_loop();
                    is_end |= r.is_end();
                }
            }
            // put back the correct state
            player.state = match state {
                ScriptPlayerState::Starting { runtime } => {
                    ScriptPlayerState::Playing { runtime }
                },
                ScriptPlayerState::Playing { runtime } => {
                    ScriptPlayerState::Playing { runtime }
                },
                ScriptPlayerState::Stopping { runtime: _ } => {
                    ScriptPlayerState::Stopped
                },
                _ => state,
            };
            if is_end && !is_loop {
                player.stop();
            }
        }
    }
}

fn script_init_system<T: ScriptAsset>(
    gt: Res<GameTime>,
    preloaded: Res<PreloadedAssets>,
    ass_script: Res<Assets<T>>,
    mut runcounts: ResMut<ScriptRunCounts<T>>,
    mut q_script: Query<(Entity, &mut ScriptPlayer<T>)>,
    mut params: ParamSet<(
        StaticSystemParam<<T::Tracker as ScriptTracker>::InitParam>,
        StaticSystemParam<T::BuildParam>,
        StaticSystemParam<<T::Tracker as ScriptTracker>::CarryoverParam>,
    )>,
) {
    for (e, mut player) in &mut q_script {
        // Extract handle from player state
        let handle = match &player.state {
            ScriptPlayerState::PrePlayHandle { handle, .. } => handle.clone(),
            ScriptPlayerState::PrePlayKey { key, .. } => {
                if let Some(handle) = preloaded.get_single_asset(&key) {
                    handle
                } else {
                    continue;
                }
            },
            _ => continue,
        };

        // Proceed if script is available
        if let Some(script) = ass_script.get(&handle) {
            let state = std::mem::replace(
                &mut player.state,
                ScriptPlayerState::Stopped,
            );
            let mut metadata = ScriptMetadata::default();

            // runtime is Option<ScriptRuntime<T>> at this point
            let runtime = match state {
                ScriptPlayerState::PrePlayHandle { runtime, .. } => runtime,
                ScriptPlayerState::PrePlayKey { runtime, key, .. } => {
                    metadata.key = Some(key);
                    runtime
                },
                _ => None,
            };

            // Generate carryover using runtime (if present)
            let carryover = {
                let mut carryover_param = params.p2().into_inner();
                runtime
                    .as_ref()
                    .map(|rt| {
                        rt.tracker.produce_carryover(e, &mut carryover_param)
                    })
                    .unwrap_or_default()
            };

            // Extract metadata info from runtime
            metadata.key_previous =
                runtime.as_ref().and_then(|rt| rt.key.clone());

            metadata.runcount =
                if let Some(count) = runcounts.counts.get_mut(&handle.id()) {
                    let r = *count;
                    *count += 1;
                    r
                } else {
                    runcounts.counts.insert(handle.id(), 1);
                    0
                };

            // Build the script runtime
            let settings = script.into_settings();

            let builder = {
                let mut tracker_init_param = params.p0().into_inner();
                ScriptRuntimeBuilder::new(
                    e,
                    settings,
                    &metadata,
                    carryover,
                    &mut tracker_init_param,
                )
            };

            let builder = {
                let mut build_param = params.p1().into_inner();
                script.build(builder, e, &mut build_param)
            };

            // Reuse the same name: now runtime is ScriptRuntime<T>
            let mut runtime = builder.build();

            // Transfer slots
            let timing = ScriptActionTiming::Tick(gt.tick());
            let slots = runtime.tracker.take_slots(timing);

            for slot in slots.iter() {
                runtime.tracker.set_slot(timing, slot, true);
            }

            // Move into Starting state
            player.state = ScriptPlayerState::Starting { runtime };
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
        runtime: Option<ScriptRuntime<T>>,
    },
    PrePlayKey {
        key: String,
        runtime: Option<ScriptRuntime<T>>,
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
        runtime: ScriptRuntime<T>,
    },
    ChangingKey {
        key: String,
        runtime: ScriptRuntime<T>,
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
        let state = std::mem::replace(
            &mut self.state,
            ScriptPlayerState::Stopped,
        );
        self.state = match state {
            ScriptPlayerState::Playing { runtime } => {
                ScriptPlayerState::ChangingHandle {
                    handle: script,
                    runtime,
                }
            },
            ScriptPlayerState::Starting { runtime } => {
                ScriptPlayerState::PrePlayHandle {
                    handle: script,
                    runtime: Some(runtime),
                }
            },
            ScriptPlayerState::Stopping { runtime } => {
                ScriptPlayerState::ChangingHandle {
                    handle: script,
                    runtime,
                }
            },
            _ => ScriptPlayerState::PrePlayHandle {
                handle: script,
                runtime: None,
            },
        };
    }

    pub fn play_key(&mut self, key: &str) {
        let state = std::mem::replace(
            &mut self.state,
            ScriptPlayerState::Stopped,
        );
        self.state = match state {
            ScriptPlayerState::Playing { runtime } => {
                ScriptPlayerState::ChangingKey {
                    key: key.into(),
                    runtime,
                }
            },
            ScriptPlayerState::Starting { runtime } => {
                ScriptPlayerState::PrePlayKey {
                    key: key.into(),
                    runtime: Some(runtime),
                }
            },
            ScriptPlayerState::Stopping { runtime } => {
                ScriptPlayerState::ChangingKey {
                    key: key.into(),
                    runtime,
                }
            },
            _ => ScriptPlayerState::PrePlayKey {
                key: key.into(),
                runtime: None,
            },
        };
    }

    pub fn stop(&mut self) {
        let state = std::mem::replace(
            &mut self.state,
            ScriptPlayerState::Stopped,
        );
        self.state = match state {
            ScriptPlayerState::Playing { runtime } => {
                ScriptPlayerState::Stopping { runtime }
            },
            _ => ScriptPlayerState::Stopped,
        }
    }

    pub fn clear_slots(&mut self) {
        let timing = ScriptActionTiming::UnknownTick;
        match &mut self.state {
            ScriptPlayerState::Playing { runtime } => {
                runtime.tracker.clear_slots(timing);
            },
            ScriptPlayerState::Starting { runtime } => {
                runtime.tracker.clear_slots(timing);
            },
            ScriptPlayerState::Stopping { runtime } => {
                runtime.tracker.clear_slots(timing);
            },
            ScriptPlayerState::PrePlayHandle {
                runtime: Some(runtime),
                ..
            } => {
                runtime.tracker.clear_slots(timing);
            },
            ScriptPlayerState::PrePlayKey {
                runtime: Some(runtime),
                ..
            } => {
                runtime.tracker.clear_slots(timing);
            },
            ScriptPlayerState::ChangingHandle { runtime, .. } => {
                runtime.tracker.clear_slots(timing);
            },
            ScriptPlayerState::ChangingKey { runtime, .. } => {
                runtime.tracker.clear_slots(timing);
            },
            _ => {},
        }
    }

    pub fn set_slot(&mut self, slot: &str, state: bool) {
        let timing = ScriptActionTiming::UnknownTick;
        match &mut self.state {
            ScriptPlayerState::Playing { runtime } => {
                runtime.tracker.set_slot(timing, slot, state);
            },
            ScriptPlayerState::Starting { runtime } => {
                runtime.tracker.set_slot(timing, slot, state);
            },
            ScriptPlayerState::Stopping { runtime } => {
                runtime.tracker.set_slot(timing, slot, state);
            },
            ScriptPlayerState::PrePlayHandle {
                runtime: Some(runtime),
                ..
            } => {
                runtime.tracker.set_slot(timing, slot, state);
            },
            ScriptPlayerState::PrePlayKey {
                runtime: Some(runtime),
                ..
            } => {
                runtime.tracker.set_slot(timing, slot, state);
            },
            ScriptPlayerState::ChangingHandle { runtime, .. } => {
                runtime.tracker.set_slot(timing, slot, state);
            },
            ScriptPlayerState::ChangingKey { runtime, .. } => {
                runtime.tracker.set_slot(timing, slot, state);
            },
            _ => {},
        }
    }

    pub fn has_slot(&self, slot: &str) -> bool {
        match &self.state {
            ScriptPlayerState::Playing { runtime } => {
                runtime.tracker.has_slot(slot)
            },
            ScriptPlayerState::Starting { runtime } => {
                runtime.tracker.has_slot(slot)
            },
            ScriptPlayerState::Stopping { runtime } => {
                runtime.tracker.has_slot(slot)
            },
            ScriptPlayerState::PrePlayHandle {
                runtime: Some(runtime),
                ..
            } => runtime.tracker.has_slot(slot),
            ScriptPlayerState::PrePlayKey {
                runtime: Some(runtime),
                ..
            } => runtime.tracker.has_slot(slot),
            ScriptPlayerState::ChangingHandle { runtime, .. } => {
                runtime.tracker.has_slot(slot)
            },
            ScriptPlayerState::ChangingKey { runtime, .. } => {
                runtime.tracker.has_slot(slot)
            },
            _ => false,
        }
    }

    /// Toggles the value of a slot and returns the new value
    pub fn toggle_slot(&mut self, slot: &str) -> bool {
        if self.has_slot(slot) {
            self.set_slot(slot, false);
            false
        } else {
            self.set_slot(slot, true);
            true
        }
    }

    pub fn config_value(&self, name: &str) -> Option<DynamicConfigValue> {
        match &self.state {
            ScriptPlayerState::Playing { runtime } => {
                runtime.config.0.get(name).cloned()
            },
            ScriptPlayerState::Starting { runtime } => {
                runtime.config.0.get(name).cloned()
            },
            ScriptPlayerState::Stopping { runtime } => {
                runtime.config.0.get(name).cloned()
            },
            ScriptPlayerState::PrePlayHandle {
                runtime: Some(runtime),
                ..
            } => runtime.config.0.get(name).cloned(),
            ScriptPlayerState::PrePlayKey {
                runtime: Some(runtime),
                ..
            } => runtime.config.0.get(name).cloned(),
            ScriptPlayerState::ChangingHandle { runtime, .. } => {
                runtime.config.0.get(name).cloned()
            },
            ScriptPlayerState::ChangingKey { runtime, .. } => {
                runtime.config.0.get(name).cloned()
            },
            _ => None,
        }
    }

    /// Get the asset key of the script that is currently playing, if known.
    ///
    /// Note: since it is possible to play assets using handles, the key
    /// might not be known/available, even if something is currently playing.
    pub fn current_key(&self) -> Option<&str> {
        match &self.state {
            ScriptPlayerState::Stopped => None,
            ScriptPlayerState::PrePlayHandle { .. } => None,
            ScriptPlayerState::PrePlayKey { key, .. } => Some(&key),
            ScriptPlayerState::Starting { runtime } => {
                runtime.key.as_ref().map(|x| x.as_str())
            },
            ScriptPlayerState::Playing { runtime } => {
                runtime.key.as_ref().map(|x| x.as_str())
            },
            ScriptPlayerState::Stopping { runtime } => {
                runtime.key.as_ref().map(|x| x.as_str())
            },
            ScriptPlayerState::ChangingHandle { .. } => None,
            ScriptPlayerState::ChangingKey { key, .. } => Some(&key),
        }
    }
}
