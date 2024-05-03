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
                (script_changeover_system::<T>.in_set(ScriptSet::Init),
                script_init_system::<T>.in_set(ScriptSet::Init)).chain(),
                script_driver_system::<T>.in_set(ScriptSet::Run),
            ),
        );
        self
    }
}

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
    type Tracker: ScriptTracker<RunIf = Self::RunIf, Settings = Self::Settings, ActionParams = Self::ActionParams>;
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
    type ActionParams: ScriptActionParams<Tracker = Self>;

    fn init<'w>(
        &mut self,
        entity: Entity,
        settings: &Self::Settings,
        metadata: &ScriptMetadata,
        param: &mut <Self::InitParam as SystemParam>::Item<'w, '_>,
    );
    fn transfer_progress(&mut self, other: &Self);
    fn track_action(
        &mut self,
        run_if: &Self::RunIf,
        params: &Self::ActionParams,
        action_id: ActionId,
    );
    fn finalize(&mut self);
    fn update<'w>(
        &mut self,
        entity: Entity,
        settings: &Self::Settings,
        param: &mut <Self::UpdateParam as SystemParam>::Item<'w, '_>,
        queue: &mut Vec<ActionId>,
    ) -> ScriptUpdateResult;
    fn queue_extra_actions(
        &mut self,
        _settings: &Self::Settings,
        _queue: &mut Vec<ActionId>,
    ) {}
    fn set_slot(&mut self, _slot: &str, _state: bool) {}
    fn has_slot(&self, _slot: &str) -> bool { false }
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
        _entity: Entity,
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
    pub fn new<'w>(
        entity: Entity,
        settings: <T::Tracker as ScriptTracker>::Settings,
        metadata: &ScriptMetadata,
        param: &mut <<T::Tracker as ScriptTracker>::InitParam as SystemParam>::Item<'w, '_>,
    ) -> Self {
        let mut tracker = T::Tracker::default();
        tracker.init(entity, &settings, metadata, param);
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
        // Need to sort to ensure actions run in the order they were
        // originally defined in the script asset.
        action_queue.0.sort_unstable();
        for action_id in action_queue.0.drain(..) {
            let action = &script_rt.actions[action_id];
            {
                let mut shouldrun_param = params.p2().into_inner();
                action.0.should_run(e, &mut script_rt.tracker, action_id, &mut shouldrun_param)
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
            loop {
                script_rt.tracker.queue_extra_actions(&script_rt.settings, &mut action_queue.0);
                if action_queue.0.is_empty() {
                    break;
                }
                // Need to sort to ensure actions run in the order they were
                // originally defined in the script asset.
                action_queue.0.sort_unstable();
                for action_id in action_queue.0.drain(..) {
                    let action = &script_rt.actions[action_id];
                    let r = {
                        let mut shouldrun_param = params.p2().into_inner();
                        action.0.should_run(e, &mut script_rt.tracker, action_id, &mut shouldrun_param)
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
    mut runcounts: ResMut<ScriptRunCounts<T>>,
    mut q_script: Query<(Entity, &mut ScriptPlayer<T>)>,
    mut params: ParamSet<(
        StaticSystemParam<<T::Tracker as ScriptTracker>::InitParam>,
        StaticSystemParam<T::BuildParam>,
    )>,
) {
    for (e, mut player) in &mut q_script {
        let mut metadata = ScriptMetadata {
            runcount: 0,
            key: None,
            key_previous: match &player.state {
                | ScriptPlayerState::ChangingHandle { old_runtime, .. }
                | ScriptPlayerState::ChangingKey { old_runtime, ..  } => old_runtime.key.clone(),
                _ => None,
            },
        };
        let handle = match &player.state {
            ScriptPlayerState::PrePlayHandle { handle, .. } => {
                handle.clone()
            }
            ScriptPlayerState::PrePlayKey { key, .. } => {
                metadata.key = Some(key.clone());
                if let Some(handle) = preloaded.get_single_asset(&key) {
                    handle
                } else {
                    continue;
                }
            },
            _ => continue,
        };
        metadata.runcount = if let Some(count) = runcounts.counts.get_mut(&handle.id()) {
            let r = *count;
            *count += 1;
            r
        } else {
            runcounts.counts.insert(handle.id(), 1);
            0
        };
        if let Some(script) = ass_script.get(&handle) {
            let settings = script.into_settings();
            let builder = {
                let mut tracker_init_param = params.p0().into_inner();
                ScriptRuntimeBuilder::new(e, settings, &metadata, &mut tracker_init_param)
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
    pub fn has_slot(&self, slot: &str) -> bool {
        match &self.state {
            ScriptPlayerState::Playing { ref runtime } => {
                runtime.tracker.has_slot(slot)
            }
            ScriptPlayerState::Starting { ref runtime } => {
                runtime.tracker.has_slot(slot)
            }
            ScriptPlayerState::Stopping { ref runtime } => {
                runtime.tracker.has_slot(slot)
            }
            ScriptPlayerState::PrePlayHandle { old_runtime: Some(ref old_runtime), .. } => {
                old_runtime.tracker.has_slot(slot)
            }
            ScriptPlayerState::PrePlayKey { old_runtime: Some(ref old_runtime), .. } => {
                old_runtime.tracker.has_slot(slot)
            }
            ScriptPlayerState::ChangingHandle { old_runtime, .. } => {
                old_runtime.tracker.has_slot(slot)
            }
            ScriptPlayerState::ChangingKey { old_runtime, .. } => {
                old_runtime.tracker.has_slot(slot)
            }
            _ => { false }
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
            ScriptPlayerState::Playing { ref runtime } => {
                runtime.config.0.get(name).cloned()
            }
            ScriptPlayerState::Starting { ref runtime } => {
                runtime.config.0.get(name).cloned()
            }
            ScriptPlayerState::Stopping { ref runtime } => {
                runtime.config.0.get(name).cloned()
            }
            ScriptPlayerState::PrePlayHandle { old_runtime: Some(ref old_runtime), .. } => {
                old_runtime.config.0.get(name).cloned()
            }
            ScriptPlayerState::PrePlayKey { old_runtime: Some(ref old_runtime), .. } => {
                old_runtime.config.0.get(name).cloned()
            }
            ScriptPlayerState::ChangingHandle { old_runtime, .. } => {
                old_runtime.config.0.get(name).cloned()
            }
            ScriptPlayerState::ChangingKey { old_runtime, .. } => {
                old_runtime.config.0.get(name).cloned()
            }
            _ => { None }
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
            ScriptPlayerState::Starting { runtime } => runtime.key.as_ref().map(|x| x.as_str()),
            ScriptPlayerState::Playing { runtime } => runtime.key.as_ref().map(|x| x.as_str()),
            ScriptPlayerState::Stopping { runtime } => runtime.key.as_ref().map(|x| x.as_str()),
            ScriptPlayerState::ChangingHandle { .. } => None,
            ScriptPlayerState::ChangingKey { key, .. } => Some(&key),
        }
    }
}
