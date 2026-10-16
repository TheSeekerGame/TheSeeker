use std::collections::HashMap;
use std::marker::PhantomData;

use bevy::asset::Asset;
use bevy::ecs::system::{StaticSystemParam, SystemParam};

use crate::assets::script::ScriptConfig;
use crate::prelude::*;
use crate::profile::TickProfiler;

pub mod common;

pub struct ScriptPlugin;

impl Plugin for ScriptPlugin {
    fn build(&self, app: &mut App) {
        app.configure_sets(
            GameTickUpdate,
            (
                ScriptSet::Init,
                ScriptSet::Run.after(ScriptSet::Init),
            ),
        );
        app.add_plugins(self::common::CommonScriptPlugin);
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
    pub execution: u64,
    pub section: Option<String>,
    pub key: Option<String>,
    pub key_previous: Option<String>,
    pub runcount: u32,
}

pub type ActionId = usize;

pub trait ScriptAsset: Asset + Sized + Send + Sync + 'static {
    type Settings: Sized + Send + Sync + 'static;
    type RunIf: ScriptRunIf<Tracker = Self::Tracker>;
    type Action: ScriptAction<
        Tracker = Self::Tracker,
        ActionParams = Self::ActionParams,
    >;
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

    fn settings(&self) -> Self::Settings;
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
    fn flush_events(
        &mut self,
        _entity: Entity,
        _param: &mut <Self::UpdateParam as SystemParam>::Item<'_, '_>,
    ) {}
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
        HashSet::default()
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
            counts: HashMap::default(),
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
                config: ScriptConfig(HashMap::default()),
                settings,
                actions: vec![],
                tracker,
            },
        }
    }

    pub fn asset_key(&self) -> Option<&str> {
        self.runtime.key.as_deref()
    }

    pub fn replace_config(&mut self, config: &ScriptConfig) {
        self.runtime.config = config.clone();
    }

    #[must_use]
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
    mut profiler: TickProfiler,
) {
    let _profile = profiler.scope("script_changeover");
    for (e, player) in &mut q_script {
        script_changeover_player(
            e,
            player.into_inner(),
            &mut params,
            &mut action_queue.0,
        );
    }
}

fn script_changeover_player<T: ScriptAsset>(
    entity: Entity,
    player: &mut ScriptPlayer<T>,
    params: &mut ParamSet<(
        StaticSystemParam<<T::Tracker as ScriptTracker>::UpdateParam>,
        StaticSystemParam<<T::Action as ScriptAction>::Param>,
        StaticSystemParam<
            <T::ActionParams as ScriptActionParams>::ShouldRunParam,
        >,
    )>,
    action_queue: &mut Vec<QueuedAction>,
) {
    let mut old_state =
        std::mem::replace(&mut player.state, ScriptPlayerState::Stopped);
    let Some(script_rt) = changing_runtime_mut(&mut old_state) else {
        player.state = old_state;
        return;
    };

    stop_script_runtime(entity, script_rt, params, action_queue);
    run_changeover_action_queue(entity, script_rt, params, action_queue);
    script_rt.tracker.flush_events(entity, &mut params.p0().into_inner());
    player.state = changeover_next_state(old_state);
}

fn changing_runtime_mut<T: ScriptAsset>(
    state: &mut ScriptPlayerState<T>,
) -> Option<&mut ScriptRuntime<T>> {
    match state {
        ScriptPlayerState::ChangingKey {
            ref mut old_runtime,
            ..
        }
        | ScriptPlayerState::ChangingHandle {
            ref mut old_runtime,
            ..
        } => Some(old_runtime),
        _ => None,
    }
}

fn start_script_runtime<T: ScriptAsset>(
    entity: Entity,
    runtime: &mut ScriptRuntime<T>,
    params: &mut ParamSet<(
        StaticSystemParam<<T::Tracker as ScriptTracker>::UpdateParam>,
        StaticSystemParam<<T::Action as ScriptAction>::Param>,
        StaticSystemParam<
            <T::ActionParams as ScriptActionParams>::ShouldRunParam,
        >,
    )>,
    action_queue: &mut Vec<QueuedAction>,
) {
    let mut tracker_param = params.p0().into_inner();
    runtime.tracker.do_start(
        entity,
        &runtime.settings,
        &mut tracker_param,
        action_queue,
    );
}

fn stop_script_runtime<T: ScriptAsset>(
    entity: Entity,
    runtime: &mut ScriptRuntime<T>,
    params: &mut ParamSet<(
        StaticSystemParam<<T::Tracker as ScriptTracker>::UpdateParam>,
        StaticSystemParam<<T::Action as ScriptAction>::Param>,
        StaticSystemParam<
            <T::ActionParams as ScriptActionParams>::ShouldRunParam,
        >,
    )>,
    action_queue: &mut Vec<QueuedAction>,
) {
    let mut tracker_param = params.p0().into_inner();
    runtime.tracker.do_stop(
        entity,
        &runtime.settings,
        &mut tracker_param,
        action_queue,
    );
}

fn update_script_runtime<T: ScriptAsset>(
    entity: Entity,
    runtime: &mut ScriptRuntime<T>,
    params: &mut ParamSet<(
        StaticSystemParam<<T::Tracker as ScriptTracker>::UpdateParam>,
        StaticSystemParam<<T::Action as ScriptAction>::Param>,
        StaticSystemParam<
            <T::ActionParams as ScriptActionParams>::ShouldRunParam,
        >,
    )>,
    action_queue: &mut Vec<QueuedAction>,
) -> ScriptUpdateResult {
    let mut tracker_param = params.p0().into_inner();
    runtime.tracker.update(
        entity,
        &runtime.settings,
        &mut tracker_param,
        action_queue,
    )
}

fn run_changeover_action_queue<T: ScriptAsset>(
    entity: Entity,
    script_rt: &mut ScriptRuntime<T>,
    params: &mut ParamSet<(
        StaticSystemParam<<T::Tracker as ScriptTracker>::UpdateParam>,
        StaticSystemParam<<T::Action as ScriptAction>::Param>,
        StaticSystemParam<
            <T::ActionParams as ScriptActionParams>::ShouldRunParam,
        >,
    )>,
    action_queue: &mut Vec<QueuedAction>,
) {
    // Need to sort to ensure actions run in the order they were
    // originally defined in the script asset.
    action_queue.sort_unstable_by_key(|qa| qa.action);
    for queued_action in action_queue.drain(..) {
        run_queued_script_action(entity, script_rt, params, queued_action);
    }
}

fn changeover_next_state<T: ScriptAsset>(
    old_state: ScriptPlayerState<T>,
) -> ScriptPlayerState<T> {
    match old_state {
        ScriptPlayerState::ChangingHandle {
            handle,
            old_runtime,
        } => ScriptPlayerState::PrePlayHandle {
            handle,
            old_runtime: Some(old_runtime),
        },
        ScriptPlayerState::ChangingKey { key, old_runtime } => {
            ScriptPlayerState::PrePlayKey {
                key,
                old_runtime: Some(old_runtime),
            }
        },
        _ => old_state,
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
    mut profiler: TickProfiler,
) {
    let _profile = profiler.scope("script_driver");
    for (e, player) in &mut q_script {
        script_driver_player(
            e,
            player.into_inner(),
            &mut params,
            &mut action_queue.0,
        );
    }
}

fn script_driver_player<T: ScriptAsset>(
    entity: Entity,
    player: &mut ScriptPlayer<T>,
    params: &mut ParamSet<(
        StaticSystemParam<<T::Tracker as ScriptTracker>::UpdateParam>,
        StaticSystemParam<<T::Action as ScriptAction>::Param>,
        StaticSystemParam<
            <T::ActionParams as ScriptActionParams>::ShouldRunParam,
        >,
    )>,
    action_queue: &mut Vec<QueuedAction>,
) {
    if player.paused {
        return;
    }

    let mut is_loop = true;
    let mut is_end = false;
    let mut keep_running = true;
    while is_loop && keep_running {
        is_loop = false;
        keep_running = run_script_driver_iteration(
            entity,
            player,
            params,
            action_queue,
            &mut is_loop,
            &mut is_end,
        );
    }
}

fn run_script_driver_iteration<T: ScriptAsset>(
    entity: Entity,
    player: &mut ScriptPlayer<T>,
    params: &mut ParamSet<(
        StaticSystemParam<<T::Tracker as ScriptTracker>::UpdateParam>,
        StaticSystemParam<<T::Action as ScriptAction>::Param>,
        StaticSystemParam<
            <T::ActionParams as ScriptActionParams>::ShouldRunParam,
        >,
    )>,
    action_queue: &mut Vec<QueuedAction>,
    is_loop: &mut bool,
    is_end: &mut bool,
) -> bool {
    let mut old_state =
        std::mem::replace(&mut player.state, ScriptPlayerState::Stopped);
    let Some(script_rt) = enqueue_driver_state_actions(
        entity,
        &mut old_state,
        params,
        action_queue,
        is_loop,
        is_end,
    ) else {
        player.state = old_state;
        return false;
    };

    run_script_action_queue(
        entity,
        script_rt,
        params,
        action_queue,
        is_loop,
        is_end,
    );
    script_rt.tracker.flush_events(entity, &mut params.p0().into_inner());
    player.state = driver_next_state(old_state);
    if *is_end && !*is_loop {
        player.stop();
    }

    true
}

fn enqueue_driver_state_actions<'a, T: ScriptAsset>(
    entity: Entity,
    state: &'a mut ScriptPlayerState<T>,
    params: &mut ParamSet<(
        StaticSystemParam<<T::Tracker as ScriptTracker>::UpdateParam>,
        StaticSystemParam<<T::Action as ScriptAction>::Param>,
        StaticSystemParam<
            <T::ActionParams as ScriptActionParams>::ShouldRunParam,
        >,
    )>,
    action_queue: &mut Vec<QueuedAction>,
    is_loop: &mut bool,
    is_end: &mut bool,
) -> Option<&'a mut ScriptRuntime<T>> {
    match state {
        ScriptPlayerState::Starting { runtime } => {
            start_script_runtime(entity, runtime, params, action_queue);
            Some(runtime)
        },
        ScriptPlayerState::Playing { runtime } => {
            let result =
                update_script_runtime(entity, runtime, params, action_queue);
            *is_loop |= result.is_loop();
            *is_end |= result.is_end();
            Some(runtime)
        },
        ScriptPlayerState::Stopping { runtime } => {
            stop_script_runtime(entity, runtime, params, action_queue);
            Some(runtime)
        },
        _ => None,
    }
}

fn driver_next_state<T: ScriptAsset>(
    old_state: ScriptPlayerState<T>,
) -> ScriptPlayerState<T> {
    match old_state {
        ScriptPlayerState::Starting { runtime }
        | ScriptPlayerState::Playing { runtime } => {
            ScriptPlayerState::Playing { runtime }
        },
        ScriptPlayerState::Stopping { runtime: _ } => ScriptPlayerState::Stopped,
        _ => old_state,
    }
}

fn run_script_action_queue<T: ScriptAsset>(
    entity: Entity,
    script_rt: &mut ScriptRuntime<T>,
    params: &mut ParamSet<(
        StaticSystemParam<<T::Tracker as ScriptTracker>::UpdateParam>,
        StaticSystemParam<<T::Action as ScriptAction>::Param>,
        StaticSystemParam<
            <T::ActionParams as ScriptActionParams>::ShouldRunParam,
        >,
    )>,
    action_queue: &mut Vec<QueuedAction>,
    is_loop: &mut bool,
    is_end: &mut bool,
) {
    loop {
        script_rt
            .tracker
            .queue_extra_actions(&script_rt.settings, action_queue);
        if action_queue.is_empty() {
            break;
        }

        action_queue.sort_unstable_by_key(|qa| qa.action);
        for qa in action_queue.drain(..) {
            let result =
                run_queued_script_action(entity, script_rt, params, qa);
            *is_loop |= result.is_loop();
            *is_end |= result.is_end();
        }
    }
}

fn run_queued_script_action<T: ScriptAsset>(
    entity: Entity,
    script_rt: &mut ScriptRuntime<T>,
    params: &mut ParamSet<(
        StaticSystemParam<<T::Tracker as ScriptTracker>::UpdateParam>,
        StaticSystemParam<<T::Action as ScriptAction>::Param>,
        StaticSystemParam<
            <T::ActionParams as ScriptActionParams>::ShouldRunParam,
        >,
    )>,
    queued_action: QueuedAction,
) -> ScriptUpdateResult {
    let action = &script_rt.actions[queued_action.action];
    {
        let mut shouldrun_param = params.p2().into_inner();
        action.0.should_run(
            entity,
            &mut script_rt.tracker,
            queued_action.action,
            &mut shouldrun_param,
        )
    }
    .err()
    .unwrap_or_else(|| {
        let mut action_param = params.p1().into_inner();
        script_rt.actions[queued_action.action].1.run(
            entity,
            queued_action.timing,
            &script_rt.actions[queued_action.action].0,
            &mut script_rt.tracker,
            &mut action_param,
        )
    })
}

fn pending_script_handle<T: ScriptAsset>(
    state: &ScriptPlayerState<T>,
    preloaded: &PreloadedAssets,
) -> Option<Handle<T>> {
    match state {
        ScriptPlayerState::PrePlayHandle { handle, .. } => Some(handle.clone()),
        ScriptPlayerState::PrePlayKey { key, .. } => {
            preloaded.get_single_asset(key)
        },
        _ => None,
    }
}

fn init_script_player<T: ScriptAsset>(
    entity: Entity,
    player: &mut ScriptPlayer<T>,
    gt: &GameTime,
    preloaded: &PreloadedAssets,
    ass_script: &Assets<T>,
    runcounts: &mut ScriptRunCounts<T>,
    params: &mut ParamSet<(
        StaticSystemParam<<T::Tracker as ScriptTracker>::InitParam>,
        StaticSystemParam<T::BuildParam>,
        StaticSystemParam<<T::Tracker as ScriptTracker>::CarryoverParam>,
    )>,
) {
    let Some(handle) = pending_script_handle(&player.state, preloaded) else {
        return;
    };
    let snapshot = player.snapshot.clone();
    let Some(script) = snapshot.as_deref().or_else(|| ass_script.get(&handle)) else {
        return;
    };

    let old_state =
        std::mem::replace(&mut player.state, ScriptPlayerState::Stopped);
    let mut metadata = ScriptMetadata { execution: player.execution, section: player.section.clone(), ..Default::default() };
    let old_runtime = match old_state {
        ScriptPlayerState::PrePlayHandle { old_runtime, .. } => old_runtime,
        ScriptPlayerState::PrePlayKey {
            old_runtime, key, ..
        } => {
            metadata.key = Some(key);
            old_runtime
        },
        _ => None,
    };
    let carryover = {
        let mut carryover_param = params.p2().into_inner();
        old_runtime
            .as_ref()
            .map(|rt| rt.tracker.produce_carryover(entity, &mut carryover_param))
            .unwrap_or_default()
    };
    metadata.key_previous = old_runtime.as_ref().and_then(|rt| rt.key.clone());
    metadata.runcount =
        if let Some(count) = runcounts.counts.get_mut(&handle.id()) {
            *count += 1;
            *count
        } else {
            runcounts.counts.insert(handle.id(), 1);
            1
        };
    let settings = script.settings();
    let builder = {
        let mut tracker_init_param = params.p0().into_inner();
        ScriptRuntimeBuilder::new(
            entity,
            settings,
            &metadata,
            carryover,
            &mut tracker_init_param,
        )
    };
    let builder = {
        let mut build_param = params.p1().into_inner();
        script.build(builder, entity, &mut build_param)
    };
    let mut runtime = builder.build();
    let timing = ScriptActionTiming::Tick(gt.tick());
    let carried_slots = old_runtime
        .map(|mut rt| rt.tracker.take_slots(timing))
        .unwrap_or_default();
    apply_carried_slots(&mut runtime, timing, &carried_slots);
    apply_pending_slots(&mut runtime, timing, &mut player.pending_slots);
    player.state = ScriptPlayerState::Starting { runtime };
}

fn apply_carried_slots<T: ScriptAsset>(
    runtime: &mut ScriptRuntime<T>,
    timing: ScriptActionTiming,
    slots: &HashSet<String>,
) {
    for slot in slots {
        runtime.tracker.set_slot(timing, slot, true);
    }
}

fn apply_pending_slots<T: ScriptAsset>(
    runtime: &mut ScriptRuntime<T>,
    timing: ScriptActionTiming,
    pending_slots: &mut HashMap<String, bool>,
) {
    if pending_slots.is_empty() {
        return;
    }

    for (slot, enabled) in std::mem::take(pending_slots) {
        runtime.tracker.set_slot(timing, &slot, enabled);
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
    mut profiler: TickProfiler,
) {
    let _profile = profiler.scope("script_init");
    for (e, mut player) in &mut q_script {
        init_script_player(
            e,
            player.as_mut(),
            &gt,
            &preloaded,
            &ass_script,
            &mut runcounts,
            &mut params,
        );
    }
}

#[derive(Component)]
pub struct ScriptPlayer<T: ScriptAsset> {
    execution: u64,
    section: Option<String>,
    snapshot: Option<std::sync::Arc<T>>,
    background_key: Option<String>,
    background_snapshot: Option<std::sync::Arc<T>>,
    foreground_owner: Option<u64>,
    presentation_slots: HashMap<String, bool>,
    state: ScriptPlayerState<T>,
    paused: bool,
    // Slots set before a runtime exists; applied when runtime starts
    pending_slots: HashMap<String, bool>,
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
            execution: 0,
            section: None,
            snapshot: None,
            background_key: None,
            background_snapshot: None,
            foreground_owner: None,
            presentation_slots: HashMap::new(),
            state: ScriptPlayerState::Stopped,
            paused: false,
            pending_slots: HashMap::new(),
        }
    }
}

impl<T: ScriptAsset> ScriptPlayer<T> {
    pub fn new() -> Self {
        Self {
            execution: 0,
            section: None,
            snapshot: None,
            background_key: None,
            background_snapshot: None,
            foreground_owner: None,
            presentation_slots: HashMap::new(),
            state: ScriptPlayerState::Stopped,
            paused: false,
            pending_slots: HashMap::new(),
        }
    }

    pub fn execution_id(&self) -> u64 {
        self.execution
    }

    pub fn set_paused(&mut self, paused: bool) {
        self.paused = paused;
    }

    pub fn play_handle(&mut self, script: Handle<T>) {
        self.execution = self.execution.wrapping_add(1);
        self.section = None;
        self.snapshot = None;
        let old_state = std::mem::replace(
            &mut self.state,
            ScriptPlayerState::Stopped,
        );
        self.state = match old_state {
            ScriptPlayerState::Starting { runtime } => {
                ScriptPlayerState::PrePlayHandle {
                    handle: script,
                    old_runtime: Some(runtime),
                }
            },
            ScriptPlayerState::Playing { runtime }
            | ScriptPlayerState::Stopping { runtime } => {
                ScriptPlayerState::ChangingHandle {
                    handle: script,
                    old_runtime: runtime,
                }
            },
            ScriptPlayerState::ChangingHandle { old_runtime, .. }
            | ScriptPlayerState::ChangingKey { old_runtime, .. } => ScriptPlayerState::ChangingHandle {
                handle: script, old_runtime,
            },
            ScriptPlayerState::PrePlayHandle { old_runtime, .. }
            | ScriptPlayerState::PrePlayKey { old_runtime, .. } => ScriptPlayerState::PrePlayHandle {
                handle: script, old_runtime,
            },
            ScriptPlayerState::Stopped => ScriptPlayerState::PrePlayHandle {
                handle: script,
                old_runtime: None,
            },
        };
    }

    pub fn play_key(&mut self, key: &str) {
        self.execution = self.execution.wrapping_add(1);
        self.section = None;
        self.snapshot = None;
        let old_state = std::mem::replace(
            &mut self.state,
            ScriptPlayerState::Stopped,
        );
        self.state = match old_state {
            ScriptPlayerState::Starting { runtime } => {
                ScriptPlayerState::PrePlayKey {
                    key: key.into(),
                    old_runtime: Some(runtime),
                }
            },
            ScriptPlayerState::Playing { runtime }
            | ScriptPlayerState::Stopping { runtime } => {
                ScriptPlayerState::ChangingKey {
                    key: key.into(),
                    old_runtime: runtime,
                }
            },
            ScriptPlayerState::ChangingHandle { old_runtime, .. }
            | ScriptPlayerState::ChangingKey { old_runtime, .. } => ScriptPlayerState::ChangingKey {
                key: key.into(), old_runtime,
            },
            ScriptPlayerState::PrePlayHandle { old_runtime, .. }
            | ScriptPlayerState::PrePlayKey { old_runtime, .. } => ScriptPlayerState::PrePlayKey {
                key: key.into(), old_runtime,
            },
            ScriptPlayerState::Stopped => ScriptPlayerState::PrePlayKey {
                key: key.into(),
                old_runtime: None,
            },
        };
    }

    /// Retains the current locomotion request while a foreground presentation owns playback.
    pub fn request_background(&mut self, key: &str) {
        self.background_key = Some(key.to_owned());
        self.background_snapshot = None;
        if self.foreground_owner.is_none() {
            self.play_key(key);
        }
    }

    pub fn request_background_snapshot(&mut self, key: &str, snapshot: std::sync::Arc<T>) {
        self.background_key = Some(key.to_owned());
        self.background_snapshot = Some(snapshot.clone());
        if self.foreground_owner.is_none() { self.play_snapshot(key, snapshot, None); }
    }

    pub fn acquire_presentation(&mut self, owner: u64) {
        if self.foreground_owner != Some(owner) { self.release_presentation_slots(); }
        self.foreground_owner = Some(owner);
    }

    /// Owns a slot until this presentation releases it; background writes remain pending.
    pub fn set_presentation_slot(&mut self, owner: u64, slot: &str, enabled: bool) {
        if self.foreground_owner != Some(owner) { return; }
        let background = self.has_slot(slot);
        self.presentation_slots.entry(slot.to_owned()).or_insert(background);
        self.write_slot(slot, enabled);
    }

    fn release_presentation_slots(&mut self) {
        for (slot, background) in std::mem::take(&mut self.presentation_slots) {
            self.write_slot(&slot, background);
        }
    }

    pub fn revoke_presentation(&mut self, owner: u64) -> bool {
        if self.foreground_owner != Some(owner) { return false; }
        self.foreground_owner = None;
        self.release_presentation_slots();
        true
    }

    pub fn release_presentation(&mut self, owner: u64) -> bool {
        if self.foreground_owner != Some(owner) { return false; }
        self.foreground_owner = None;
        self.release_presentation_slots();
        if let Some(key) = self.background_key.clone() {
            let snapshot = self.background_snapshot.clone();
            self.play_key(&key);
            self.snapshot = snapshot;
        }
        true
    }

    pub fn play_snapshot(&mut self, key: &str, snapshot: std::sync::Arc<T>, section: Option<&str>) {
        self.play_key(key);
        self.snapshot = Some(snapshot);
        self.section = section.map(str::to_owned);
    }

    pub fn stop(&mut self) {
        let old_state = std::mem::replace(
            &mut self.state,
            ScriptPlayerState::Stopped,
        );
        self.state = match old_state {
            ScriptPlayerState::Playing { runtime }
            | ScriptPlayerState::Starting { runtime }
            | ScriptPlayerState::Stopping { runtime } => ScriptPlayerState::Stopping { runtime },
            ScriptPlayerState::ChangingHandle { old_runtime, .. }
            | ScriptPlayerState::ChangingKey { old_runtime, .. }
            | ScriptPlayerState::PrePlayHandle { old_runtime: Some(old_runtime), .. }
            | ScriptPlayerState::PrePlayKey { old_runtime: Some(old_runtime), .. } => ScriptPlayerState::Stopping { runtime: old_runtime },
            _ => ScriptPlayerState::Stopped,
        }
    }

    pub fn clear_slots(&mut self) {
        let timing = ScriptActionTiming::UnknownTick;
        match &mut self.state {
            // Active runtimes we can mutate directly
            ScriptPlayerState::Playing { ref mut runtime }
            | ScriptPlayerState::Starting { ref mut runtime }
            | ScriptPlayerState::Stopping { ref mut runtime } => {
                runtime.tracker.clear_slots(timing);
            },

            // Runtimes stored inside an Option while we are pre-playing
            // Runtimes stored while pre-playing or changing over
            ScriptPlayerState::PrePlayHandle {
                old_runtime: Some(old_runtime),
                ..
            }
            | ScriptPlayerState::PrePlayKey {
                old_runtime: Some(old_runtime),
                ..
            }
            | ScriptPlayerState::ChangingHandle { old_runtime, .. }
            | ScriptPlayerState::ChangingKey { old_runtime, .. } => {
                old_runtime.tracker.clear_slots(timing);
            },
            _ => {
                self.pending_slots.clear();
            },
        }
    }

    pub fn set_slot(&mut self, slot: &str, state: bool) {
        if let Some(background) = self.presentation_slots.get_mut(slot) {
            *background = state;
            return;
        }
        self.write_slot(slot, state);
    }

    fn write_slot(&mut self, slot: &str, state: bool) {
        let timing = ScriptActionTiming::UnknownTick;
        match &mut self.state {
            ScriptPlayerState::Playing { ref mut runtime }
            | ScriptPlayerState::Starting { ref mut runtime }
            | ScriptPlayerState::Stopping { ref mut runtime } => {
                runtime.tracker.set_slot(timing, slot, state);
            },
            ScriptPlayerState::PrePlayHandle {
                old_runtime: Some(old_runtime),
                ..
            }
            | ScriptPlayerState::PrePlayKey {
                old_runtime: Some(old_runtime),
                ..
            }
            | ScriptPlayerState::ChangingHandle { old_runtime, .. }
            | ScriptPlayerState::ChangingKey { old_runtime, .. } => {
                old_runtime.tracker.set_slot(timing, slot, state);
            },
            _ => {
                // No runtime yet; remember desired slot state to apply on init
                self.pending_slots.insert(slot.to_string(), state);
            },
        }
    }

    pub fn has_slot(&self, slot: &str) -> bool {
        match &self.state {
            ScriptPlayerState::Playing { runtime }
            | ScriptPlayerState::Starting { runtime }
            | ScriptPlayerState::Stopping { runtime } => {
                runtime.tracker.has_slot(slot)
            },
            ScriptPlayerState::PrePlayHandle {
                old_runtime: Some(old_runtime),
                ..
            }
            | ScriptPlayerState::PrePlayKey {
                old_runtime: Some(old_runtime),
                ..
            }
            | ScriptPlayerState::ChangingHandle { old_runtime, .. }
            | ScriptPlayerState::ChangingKey { old_runtime, .. } => {
                old_runtime.tracker.has_slot(slot)
            },
            _ => self.pending_slots.get(slot).copied().unwrap_or(false),
        }
    }

    /// Get the asset key of the script that is currently playing, if known.
    ///
    /// Note: since it is possible to play assets using handles, the key
    /// might not be known/available, even if something is currently playing.
    pub fn current_key(&self) -> Option<&str> {
        match &self.state {
            // Nothing playing (or handle-based variants where we don't know the key)
            ScriptPlayerState::Stopped
            | ScriptPlayerState::PrePlayHandle { .. }
            | ScriptPlayerState::ChangingHandle { .. } => None,

            // Key known directly on the enum variant
            ScriptPlayerState::PrePlayKey { key, .. }
            | ScriptPlayerState::ChangingKey { key, .. } => Some(key.as_str()),

            // Key stored inside the runtime
            ScriptPlayerState::Starting { runtime }
            | ScriptPlayerState::Playing { runtime }
            | ScriptPlayerState::Stopping { runtime } => runtime.key.as_deref(),
        }
    }
}

#[cfg(test)]
mod ownership_tests;
