use bevy::audio::AudioSource;
use bevy::ecs::system::lifetimeless::*;

use super::*;
use crate::assets::script::*;
use crate::audio::{
    AudioEngine, AudioInstanceId, AudioPlaybackConfig, AudioStartTime,
};
use crate::data::{OneOrMany, TimeSpec};
use crate::time::ClockTime;

pub struct CommonScriptPlugin;

impl Plugin for CommonScriptPlugin {
    fn build(&self, app: &mut App) {
        app.add_script_runtime::<Script>();
    }
}

#[derive(Bundle, Default)]
pub struct ScriptBundle {
    pub player: ScriptPlayer<Script>,
}

impl ScriptBundle {
    pub fn new_play_handle(handle: Handle<Script>) -> Self {
        let mut player = ScriptPlayer::default();
        player.play_handle(handle);
        Self { player }
    }
    pub fn new_play_key(key: &str) -> Self {
        let mut player = ScriptPlayer::default();
        player.play_key(key);
        Self { player }
    }
}

#[derive(Default)]
pub struct CommonScriptTracker {
    tick_actions: Vec<(u64, ActionId)>,
    time_actions: Vec<(Duration, ActionId)>,
    tickquant_actions: Vec<(Quant, ActionId)>,
    slot_enable_actions: HashMap<String, Vec<ActionId>>,
    slot_disable_actions: HashMap<String, Vec<ActionId>>,
    start_actions: Vec<ActionId>,
    stop_actions: Vec<ActionId>,
    start_tick: u64,
    next_tick_id: usize,
    start_time: Duration,
    next_time_id: usize,
    slots_enabled: HashSet<String>,
    q_extra: Vec<QueuedAction>,
    q_delayed: Vec<(u64, ActionId)>,
    old_key: Option<String>,
    runcount: u32,
    my_sounds: Vec<AudioInstanceId>,
    my_background_sounds: Vec<AudioInstanceId>,
}

#[derive(Default)]
pub struct CommonScriptCarryover;

impl CommonScriptTracker {
}

fn init_level_time(
    tracker: &mut CommonScriptTracker,
    leveltime: Option<&LevelLoadTime>,
) {
    let Some(leveltime) = leveltime else {
        error!("Script with time base 'Level' wants to run, but level start time is unknown! (are we in-game?)");
        tracker.start_tick = 0;
        tracker.start_time = Duration::new(0, 0);
        return;
    };

    tracker.start_tick = leveltime.tick;
    tracker.start_time = leveltime.time;
}

fn track_tick_actions(
    tick_actions: &mut Vec<(u64, ActionId)>,
    ticks: &OneOrMany<u64>,
    action_id: ActionId,
) {
    match ticks {
        OneOrMany::Single(tick) => tick_actions.push((*tick, action_id)),
        OneOrMany::Many(ticks) => {
            tick_actions.extend(ticks.iter().map(|&tick| (tick, action_id)));
        },
    }
}

fn track_millis_actions(
    time_actions: &mut Vec<(Duration, ActionId)>,
    millis: &OneOrMany<u64>,
    action_id: ActionId,
) {
    match millis {
        OneOrMany::Single(millis) => {
            time_actions.push((Duration::from_millis(*millis), action_id));
        },
        OneOrMany::Many(millis) => {
            time_actions.extend(
                millis
                    .iter()
                    .map(|&millis| (Duration::from_millis(millis), action_id)),
            );
        },
    }
}

fn track_time_spec_actions(
    time_actions: &mut Vec<(Duration, ActionId)>,
    timespecs: &OneOrMany<TimeSpec>,
    action_id: ActionId,
) {
    match timespecs {
        OneOrMany::Single(timespec) => {
            time_actions.push((Duration::from(*timespec), action_id));
        },
        OneOrMany::Many(timespecs) => {
            time_actions.extend(
                timespecs
                    .iter()
                    .map(|&timespec| (Duration::from(timespec), action_id)),
            );
        },
    }
}

fn queue_ready_delayed_actions(
    delayed: &[(u64, ActionId)],
    tick: u64,
    queue: &mut Vec<QueuedAction>,
) {
    for (action_tick, action_id) in delayed {
        if tick < *action_tick {
            continue;
        }

        queue.push(QueuedAction {
            timing: ScriptActionTiming::Tick(*action_tick),
            action: *action_id,
        });
    }
}

fn queue_ready_time_actions(
    tracker: &mut CommonScriptTracker,
    elapsed: Duration,
    queue: &mut Vec<QueuedAction>,
) {
    while tracker.next_time_id < tracker.time_actions.len() {
        let next = &tracker.time_actions[tracker.next_time_id];
        if elapsed - tracker.start_time <= next.0 {
            break;
        }

        queue.push(QueuedAction {
            timing: ScriptActionTiming::Time(tracker.start_time + next.0),
            action: next.1,
        });
        tracker.next_time_id += 1;
    }
}

fn queue_ready_tick_actions(
    tracker: &mut CommonScriptTracker,
    tick: u64,
    queue: &mut Vec<QueuedAction>,
) {
    while tracker.next_tick_id < tracker.tick_actions.len() {
        let next = &tracker.tick_actions[tracker.next_tick_id];
        if tick - tracker.start_tick <= next.0 {
            break;
        }

        queue.push(QueuedAction {
            timing: ScriptActionTiming::Tick(tracker.start_tick + next.0),
            action: next.1,
        });
        tracker.next_tick_id += 1;
    }
}

fn queue_ready_tickquant_actions(
    tickquant_actions: &[(Quant, ActionId)],
    tick: u64,
    queue: &mut Vec<QueuedAction>,
) {
    for (quant, action_id) in tickquant_actions {
        if !quant.check(tick as i64) {
            continue;
        }

        queue.push(QueuedAction {
            timing: ScriptActionTiming::Tick(tick),
            action: *action_id,
        });
    }
}

fn enable_script_slot(
    tracker: &mut CommonScriptTracker,
    timing: ScriptActionTiming,
    slot: &str,
) {
    if tracker.slots_enabled.contains(slot) {
        return;
    }

    tracker.slots_enabled.insert(slot.to_owned());
    queue_slot_actions(
        &mut tracker.q_extra,
        &tracker.slot_enable_actions,
        timing,
        slot,
    );
}

fn disable_script_slot(
    tracker: &mut CommonScriptTracker,
    timing: ScriptActionTiming,
    slot: &str,
) {
    if !tracker.slots_enabled.remove(slot) {
        return;
    }

    queue_slot_actions(
        &mut tracker.q_extra,
        &tracker.slot_disable_actions,
        timing,
        slot,
    );
}

fn queue_slot_actions(
    queue: &mut Vec<QueuedAction>,
    slot_actions: &HashMap<String, Vec<ActionId>>,
    timing: ScriptActionTiming,
    slot: &str,
) {
    let Some(actions) = slot_actions.get(slot) else {
        return;
    };

    queue.extend(
        actions
            .iter()
            .map(|&action| QueuedAction { timing, action }),
    );
}

impl ScriptTracker for CommonScriptTracker {
    type ActionParams = CommonScriptParams;
    type Carryover = CommonScriptCarryover;
    type CarryoverParam = ();
    type InitParam = (
        SRes<ClockTime>,
        SRes<GameTime>,
        Option<SRes<LevelLoadTime>>,
        SQuery<&'static TimeBase>,
        SQuery<&'static ScriptTickQuant>,
    );
    type RunIf = CommonScriptRunIf;
    type Settings = CommonScriptSettings;
    type UpdateParam = (
        SRes<ClockTime>,
        SRes<GameTime>,
        SRes<AudioEngine>,
    );

    fn init(
        &mut self,
        entity: Entity,
        settings: &Self::Settings,
        metadata: &ScriptMetadata,
        _carryover: Self::Carryover,
        (time, gametime, leveltime, query_tb, query_quant): &mut <Self::InitParam as SystemParam>::Item<'_, '_>,
    ) {
        let time_base = query_tb.get(entity).unwrap_or(&settings.time_base);
        let tick_quant = query_quant
            .get(entity)
            .ok()
            .or(settings.tick_quant.as_ref());
        match time_base {
            TimeBase::Relative => {
                self.start_tick = gametime.tick();
                self.start_time = time.elapsed();
            },
            TimeBase::Level => {
                init_level_time(self, leveltime.as_deref());
            },
            TimeBase::Startup => {
                self.start_tick = 0;
                self.start_time = Duration::new(0, 0);
            },
        }
        if let Some(ScriptTickQuant(quant)) = tick_quant {
            self.start_tick = quant.apply(self.start_tick as i64) as u64;
        }
        self.old_key.clone_from(&metadata.key_previous);
        self.runcount = metadata.runcount;
    }

    fn produce_carryover(
        &self,
        _entity: Entity,
        (): &mut <Self::CarryoverParam as SystemParam>::Item<'_, '_>,
    ) -> Self::Carryover {
        CommonScriptCarryover
    }

    fn transfer_progress(&mut self, other: &Self) {
        self.start_tick = other.start_tick;
        self.start_time = other.start_time;
    }

    fn track_action(
        &mut self,
        run_if: &Self::RunIf,
        _params: &Self::ActionParams,
        action_id: ActionId,
    ) {
        match run_if {
            CommonScriptRunIf::Tick(tick) => {
                track_tick_actions(&mut self.tick_actions, tick, action_id);
            },
            CommonScriptRunIf::TickQuant(quant) => {
                self.tickquant_actions.push((*quant, action_id));
            },
            CommonScriptRunIf::Millis(millis) => {
                track_millis_actions(&mut self.time_actions, millis, action_id);
            },
            CommonScriptRunIf::Time(timespec) => {
                track_time_spec_actions(&mut self.time_actions, timespec, action_id);
            },
            CommonScriptRunIf::SlotEnable(slot) => {
                self.slot_enable_actions
                    .entry(slot.clone())
                    .or_default()
                    .push(action_id);
            },
            CommonScriptRunIf::SlotDisable(slot) => {
                self.slot_disable_actions
                    .entry(slot.clone())
                    .or_default()
                    .push(action_id);
            },
            CommonScriptRunIf::PlaybackControl(PlaybackControl::Start) => {
                self.start_actions.push(action_id);
            },
            CommonScriptRunIf::PlaybackControl(PlaybackControl::Stop) => {
                self.stop_actions.push(action_id);
            },
        }
    }

    fn finalize(&mut self) {
        self.tick_actions.sort_by_key(|(tick, _)| *tick);
        self.time_actions.sort_by_key(|(duration, _)| *duration);
    }

    fn update(
        &mut self,
        _entity: Entity,
        _settings: &Self::Settings,
        (time, game_time, audio_engine): &mut <Self::UpdateParam as SystemParam>::Item<
            '_,
            '_,
        >,
        queue: &mut Vec<QueuedAction>,
    ) -> ScriptUpdateResult {
        // any delayed actions
        // we don't remove them here, only trigger them to run
        // they will manage themselves in/out of `q_delayed` when they run
        queue_ready_delayed_actions(&self.q_delayed, game_time.tick(), queue);

        // check any time actions
        queue_ready_time_actions(
            self,
            time.elapsed(),
            queue,
        );
        // check any tick actions
        queue_ready_tick_actions(self, game_time.tick(), queue);
        // check any tickquant actions
        queue_ready_tickquant_actions(
            &self.tickquant_actions,
            game_time.tick(),
            queue,
        );
        audio_engine.cleanup_ids(&mut self.my_sounds);
        audio_engine.cleanup_ids(&mut self.my_background_sounds);
        if self.next_time_id >= self.time_actions.len()
            && self.next_tick_id >= self.tick_actions.len()
            && self.tickquant_actions.is_empty()
        {
            ScriptUpdateResult::Finished
        } else {
            ScriptUpdateResult::NormalRun
        }
    }

    fn queue_extra_actions(
        &mut self,
        _settings: &Self::Settings,
        queue: &mut Vec<QueuedAction>,
    ) {
        queue.append(&mut self.q_extra);
    }

    fn do_start(
        &mut self,
        _entity: Entity,
        _settings: &Self::Settings,
        (_time, game_time, _audio_engine): &mut <Self::UpdateParam as SystemParam>::Item<
            '_,
            '_,
        >,
        queue: &mut Vec<QueuedAction>,
    ) {
        queue.extend(
            self.start_actions.drain(..).map(|action| QueuedAction {
                timing: ScriptActionTiming::Tick(game_time.tick()),
                action,
            }),
        );
    }

    fn do_stop(
        &mut self,
        _entity: Entity,
        _settings: &Self::Settings,
        (_time, game_time, _audio_engine): &mut <Self::UpdateParam as SystemParam>::Item<
            '_,
            '_,
        >,
        queue: &mut Vec<QueuedAction>,
    ) {
        queue.extend(
            self.stop_actions.drain(..).map(|action| QueuedAction {
                timing: ScriptActionTiming::Tick(game_time.tick()),
                action,
            }),
        );
    }

    fn set_slot(
        &mut self,
        timing: ScriptActionTiming,
        slot: &str,
        state: bool,
    ) {
        if state {
            enable_script_slot(self, timing, slot);
            return;
        }

        disable_script_slot(self, timing, slot);
    }

    fn has_slot(&self, slot: &str) -> bool {
        self.slots_enabled.contains(slot)
    }

    fn take_slots(&mut self, timing: ScriptActionTiming) -> HashSet<String> {
        for slot in self.slots_enabled.iter() {
            queue_slot_actions(
                &mut self.q_extra,
                &self.slot_disable_actions,
                timing,
                slot,
            );
        }
        std::mem::take(&mut self.slots_enabled)
    }

    fn clear_slots(&mut self, timing: ScriptActionTiming) {
        for slot in self.slots_enabled.iter() {
            queue_slot_actions(
                &mut self.q_extra,
                &self.slot_disable_actions,
                timing,
                slot,
            );
        }
        self.slots_enabled.clear()
    }
}

impl ScriptRunIf for CommonScriptRunIf {
    type Tracker = CommonScriptTracker;
}

impl CommonScriptParams {
    fn delay_gate_passes(
        &self,
        tracker: &mut CommonScriptTracker,
        action_id: ActionId,
        game_tick: u64,
    ) -> bool {
        if let Some(i_delayed) = tracker
            .q_delayed
            .iter()
            .position(|(tick, aid)| *tick == game_tick && *aid == action_id)
        {
            tracker.q_delayed.remove(i_delayed);
            return true;
        }

        let Some(delay_ticks) = self.delay_ticks else {
            return true;
        };
        tracker
            .q_delayed
            .push((game_tick + delay_ticks as u64, action_id));
        false
    }

    fn runcount_passes(&self, tracker: &CommonScriptTracker) -> bool {
        let runcount = tracker.runcount;
        if self.if_runcount_lt.is_some_and(|lt| runcount >= lt)
            || self.if_runcount_le.is_some_and(|le| runcount > le)
            || self.if_runcount_gt.is_some_and(|gt| runcount <= gt)
            || self.if_runcount_ge.is_some_and(|ge| runcount < ge)
        {
            return false;
        }

        if self
            .if_runcount_quant
            .is_some_and(|quant| !quant.check(runcount as i64))
        {
            return false;
        }

        if self
            .if_runcount_is
            .as_ref()
            .is_some_and(|eq| !runcount_matches(eq, runcount))
        {
            return false;
        }

        !self
            .if_runcount_is_not
            .as_ref()
            .is_some_and(|eq| runcount_matches(eq, runcount))
    }

    fn previous_key_passes(&self, tracker: &CommonScriptTracker) -> bool {
        match (&self.if_previous_script_key, &tracker.old_key) {
            (None, _) => true,
            (Some(req), Some(old)) if req == old => true,
            _ => false,
        }
    }

    fn slot_requirements_pass(&self, tracker: &CommonScriptTracker) -> bool {
        if !self.forbid_slots_any.is_empty()
            && self
                .forbid_slots_any
                .iter()
                .any(|s| tracker.slots_enabled.contains(s))
        {
            return false;
        }
        if !self.forbid_slots_all.is_empty()
            && self
                .forbid_slots_all
                .iter()
                .all(|s| tracker.slots_enabled.contains(s))
        {
            return false;
        }
        if !self.require_slots_all.is_empty()
            && !self
                .require_slots_all
                .iter()
                .all(|s| tracker.slots_enabled.contains(s))
        {
            return false;
        }
        self.require_slots_any.is_empty()
            || self
                .require_slots_any
                .iter()
                .any(|s| tracker.slots_enabled.contains(s))
    }

    fn rng_passes(&self) -> bool {
        let Some(rng_pct) = self.rng_pct else {
            return true;
        };
        let mut rng = rand::rng();
        rng.random_bool((rng_pct as f64 / 100.0).clamp(0.0, 1.0))
    }
}

fn runcount_matches(expected: &OneOrMany<u32>, runcount: u32) -> bool {
    match expected {
        OneOrMany::Single(x) => *x == runcount,
        OneOrMany::Many(x) => x.contains(&runcount),
    }
}

impl ScriptActionParams for CommonScriptParams {
    type ShouldRunParam = (SRes<ClockTime>, SRes<GameTime>);
    type Tracker = CommonScriptTracker;

    fn should_run(
        &self,
        _entity: Entity,
        tracker: &mut Self::Tracker,
        action_id: ActionId,
        (_time, game_time): &mut <Self::ShouldRunParam as SystemParam>::Item<
            '_,
            '_,
        >,
    ) -> Result<(), ScriptUpdateResult> {
        if !self.delay_gate_passes(tracker, action_id, game_time.tick())
            || !self.runcount_passes(tracker)
            || !self.previous_key_passes(tracker)
            || !self.slot_requirements_pass(tracker)
            || !self.rng_passes()
        {
            return Err(ScriptUpdateResult::NormalRun);
        }

        Ok(())
    }
}

impl ScriptAction for CommonScriptAction {
    type ActionParams = CommonScriptParams;
    type Param = (
        SRes<GameTime>,
        SRes<PreloadedAssets>,
        SRes<AssetServer>,
        SCommands,
        SResMut<AudioEngine>,
    );
    type Tracker = CommonScriptTracker;

    fn run(
        &self,
        entity: Entity,
        timing: ScriptActionTiming,
        _actionparams: &Self::ActionParams,
        tracker: &mut Self::Tracker,
        (
            ref gt,
            ref preloaded,
            ref asset_server,
            ref mut commands,
            ref mut audio_engine,
        ): &mut <Self::Param as SystemParam>::Item<'_, '_>,
    ) -> ScriptUpdateResult {
        if let Some(result) = run_entity_or_slot_action(
            self,
            entity,
            timing,
            tracker,
            commands,
        ) {
            return result;
        }

        audio_engine.set_command_time(script_audio_start_time(timing, gt));
        match self {
            CommonScriptAction::PlayBackgroundAudio {
                asset_key,
                label,
                volume,
                r#loop,
            } => {
                let mut audio = ScriptAudioContext {
                    audio_engine,
                    preloaded,
                    asset_server,
                    game_time: gt,
                };
                play_script_background_audio(
                    tracker,
                    &mut audio,
                    timing,
                    asset_key,
                    label.as_deref(),
                    volume.unwrap_or(1.0),
                    r#loop.unwrap_or(false),
                );
                ScriptUpdateResult::NormalRun
            },
            CommonScriptAction::PlayAudio {
                asset_key,
                label,
                volume,
                pan,
            } => {
                let mut audio = ScriptAudioContext {
                    audio_engine,
                    preloaded,
                    asset_server,
                    game_time: gt,
                };
                play_script_sfx(
                    tracker,
                    &mut audio,
                    timing,
                    asset_key,
                    label.as_deref(),
                    volume.unwrap_or(1.0),
                    pan.unwrap_or(0.0),
                );
                ScriptUpdateResult::NormalRun
            },
            CommonScriptAction::StopAudio {
                current_script_only,
                label,
            } => {
                stop_script_sfx(
                    tracker,
                    audio_engine,
                    current_script_only.unwrap_or(true),
                    label.as_deref(),
                );
                ScriptUpdateResult::NormalRun
            },
            CommonScriptAction::StopBackgroundAudio {
                current_script_only,
                label,
            } => {
                stop_script_background_audio(
                    tracker,
                    audio_engine,
                    current_script_only.unwrap_or(true),
                    label.as_deref(),
                );
                ScriptUpdateResult::NormalRun
            },
            _ => ScriptUpdateResult::NormalRun,
        }
    }
}

fn run_entity_or_slot_action(
    action: &CommonScriptAction,
    entity: Entity,
    timing: ScriptActionTiming,
    tracker: &mut CommonScriptTracker,
    commands: &mut Commands,
) -> Option<ScriptUpdateResult> {
    match action {
        CommonScriptAction::DespawnEntity => {
            commands.entity(entity).despawn();
            Some(ScriptUpdateResult::Terminated)
        },
        CommonScriptAction::SpawnScript { asset_key } => {
            let mut player = ScriptPlayer::new();
            player.play_key(asset_key.as_str());
            commands.spawn(ScriptBundle { player });
            Some(ScriptUpdateResult::NormalRun)
        },
        CommonScriptAction::SlotEnable { slot } => {
            tracker.set_slot(timing, slot, true);
            Some(ScriptUpdateResult::NormalRun)
        },
        CommonScriptAction::SlotDisable { slot } => {
            tracker.set_slot(timing, slot, false);
            Some(ScriptUpdateResult::NormalRun)
        },
        CommonScriptAction::SlotToggle { slot } => {
            tracker.set_slot(timing, slot, !tracker.has_slot(slot));
            Some(ScriptUpdateResult::NormalRun)
        },
        _ => None,
    }
}

struct ScriptAudioContext<'a> {
    audio_engine: &'a mut AudioEngine,
    preloaded: &'a PreloadedAssets,
    asset_server: &'a AssetServer,
    game_time: &'a GameTime,
}

fn script_audio_start_time(
    timing: ScriptActionTiming,
    game_time: &GameTime,
) -> AudioStartTime {
    match timing {
        ScriptActionTiming::Unknown => AudioStartTime::Immediate,
        ScriptActionTiming::UnknownTick => AudioStartTime::Tick(game_time.tick()),
        ScriptActionTiming::Time(time) => AudioStartTime::Time(time),
        ScriptActionTiming::Tick(tick) => AudioStartTime::Tick(tick),
    }
}

fn choose_script_audio(
    preloaded: &PreloadedAssets,
    asset_key: &str,
) -> Option<Handle<AudioSource>> {
    let sounds: Vec<Handle<AudioSource>> = preloaded
        .get_multi_asset(asset_key)
        .unwrap_or(&[])
        .iter()
        .map(|h_untyped| h_untyped.clone().typed::<AudioSource>())
        .collect();
    sounds.choose(&mut rand::rng()).cloned()
}

fn play_script_background_audio(
    tracker: &mut CommonScriptTracker,
    audio: &mut ScriptAudioContext<'_>,
    timing: ScriptActionTiming,
    asset_key: &str,
    label: Option<&str>,
    volume: f32,
    looped: bool,
) {
    let Some(sound) = choose_script_audio(audio.preloaded, asset_key) else {
        return;
    };
    if let Some(id) = audio.audio_engine.play_background(
        label,
        &sound,
        audio.asset_server,
        AudioPlaybackConfig {
            timing: script_audio_start_time(timing, audio.game_time),
            volume,
            looped,
            ..Default::default()
        },
    ) {
        tracker.my_background_sounds.push(id);
    }
}

fn play_script_sfx(
    tracker: &mut CommonScriptTracker,
    audio: &mut ScriptAudioContext<'_>,
    timing: ScriptActionTiming,
    asset_key: &str,
    label: Option<&str>,
    volume: f32,
    pan: f32,
) {
    let Some(sound) = choose_script_audio(audio.preloaded, asset_key) else {
        return;
    };
    if let Some(id) = audio.audio_engine.play_sfx(
        label,
        &sound,
        audio.asset_server,
        AudioPlaybackConfig {
            timing: script_audio_start_time(timing, audio.game_time),
            volume,
            pan,
            ..Default::default()
        },
    ) {
        tracker.my_sounds.push(id);
    }
}

fn stop_script_sfx(
    tracker: &mut CommonScriptTracker,
    audio_engine: &mut AudioEngine,
    current_script_only: bool,
    label: Option<&str>,
) {
    if current_script_only {
        stop_tracked_audio(&mut tracker.my_sounds, audio_engine, label);
    } else if let Some(label) = label {
        audio_engine.stop_sfx_label(label);
    } else {
        audio_engine.stop_sfx_all();
    }
}

fn stop_script_background_audio(
    tracker: &mut CommonScriptTracker,
    audio_engine: &mut AudioEngine,
    current_script_only: bool,
    label: Option<&str>,
) {
    if current_script_only {
        stop_tracked_audio(
            &mut tracker.my_background_sounds,
            audio_engine,
            label,
        );
    } else {
        audio_engine.stop_backgrounds(label);
    }
}

fn stop_tracked_audio(
    tracked: &mut Vec<AudioInstanceId>,
    audio_engine: &mut AudioEngine,
    label: Option<&str>,
) {
    if let Some(label) = label {
        tracked.retain(|id| keep_tracked_audio_id(*id, audio_engine, label));
        return;
    }

    audio_engine.stop_many(tracked);
    tracked.clear();
}

fn keep_tracked_audio_id(
    id: AudioInstanceId,
    audio_engine: &mut AudioEngine,
    label: &str,
) -> bool {
    if !audio_engine.matches_label(id, label) {
        return true;
    }

    audio_engine.stop_one(id);
    false
}

#[derive(Default)]
pub struct ExtendedScriptCarryover<T> {
    pub extended: T,
    pub common: CommonScriptCarryover,
}

#[derive(Default)]
pub struct ExtendedScriptTracker<T: ScriptTracker> {
    pub extended: T,
    pub common: CommonScriptTracker,
}

impl<T: ScriptTracker> ScriptTracker for ExtendedScriptTracker<T> {
    type ActionParams = ExtendedScriptParams<T::ActionParams>;
    type Carryover = ExtendedScriptCarryover<T::Carryover>;
    type CarryoverParam = (
        T::CarryoverParam,
        <<CommonScriptRunIf as ScriptRunIf>::Tracker as ScriptTracker>::CarryoverParam,
    );
    type InitParam = (
        T::InitParam,
        <<CommonScriptRunIf as ScriptRunIf>::Tracker as ScriptTracker>::InitParam,
    );
    type RunIf = ExtendedScriptRunIf<T::RunIf>;
    type Settings = ExtendedScriptSettings<T::Settings>;
    type UpdateParam = (
        T::UpdateParam,
        <<CommonScriptRunIf as ScriptRunIf>::Tracker as ScriptTracker>::UpdateParam,
    );

    fn init(
        &mut self,
        entity: Entity,
        settings: &Self::Settings,
        metadata: &ScriptMetadata,
        carryover: Self::Carryover,
        param: &mut <Self::InitParam as SystemParam>::Item<'_, '_>,
    ) {
        self.extended.init(
            entity,
            &settings.extended,
            metadata,
            carryover.extended,
            &mut param.0,
        );
        self.common.init(
            entity,
            &settings.common,
            metadata,
            carryover.common,
            &mut param.1,
        );
    }

    fn produce_carryover(
        &self,
        entity: Entity,
        param: &mut <Self::CarryoverParam as SystemParam>::Item<'_, '_>,
    ) -> Self::Carryover {
        ExtendedScriptCarryover {
            extended: self.extended.produce_carryover(entity, &mut param.0),
            common: self.common.produce_carryover(entity, &mut param.1),
        }
    }

    fn flush_events(
        &mut self,
        entity: Entity,
        param: &mut <Self::UpdateParam as SystemParam>::Item<'_, '_>,
    ) {
        self.extended.flush_events(entity, &mut param.0);
        self.common.flush_events(entity, &mut param.1);
    }

    fn transfer_progress(&mut self, other: &Self) {
        self.extended.transfer_progress(&other.extended);
        self.common.transfer_progress(&other.common);
    }

    fn track_action(
        &mut self,
        run_if: &Self::RunIf,
        params: &Self::ActionParams,
        action_id: ActionId,
    ) {
        match run_if {
            ExtendedScriptRunIf::Extended(run_if) => {
                self.extended
                    .track_action(run_if, &params.extended, action_id);
            },
            ExtendedScriptRunIf::Common(run_if) => {
                self.common.track_action(run_if, &params.common, action_id);
            },
        }
    }

    fn finalize(&mut self) {
        self.extended.finalize();
        self.common.finalize();
    }

    fn update(
        &mut self,
        entity: Entity,
        settings: &Self::Settings,
        param: &mut <Self::UpdateParam as SystemParam>::Item<'_, '_>,
        queue: &mut Vec<QueuedAction>,
    ) -> ScriptUpdateResult {
        let r_extended = self.extended.update(
            entity,
            &settings.extended,
            &mut param.0,
            queue,
        );
        let r_common = self.common.update(
            entity,
            &settings.common,
            &mut param.1,
            queue,
        );
        match (r_extended, r_common) {
            (ScriptUpdateResult::Terminated, _)
            | (_, ScriptUpdateResult::Terminated) => {
                ScriptUpdateResult::Terminated
            },
            (ScriptUpdateResult::Finished, ScriptUpdateResult::Finished) => {
                ScriptUpdateResult::Finished
            },
            _ => ScriptUpdateResult::NormalRun,
        }
    }

    fn queue_extra_actions(
        &mut self,
        settings: &Self::Settings,
        queue: &mut Vec<QueuedAction>,
    ) {
        self.extended.queue_extra_actions(&settings.extended, queue);
        self.common.queue_extra_actions(&settings.common, queue);
    }

    fn do_start(
        &mut self,
        entity: Entity,
        settings: &Self::Settings,
        param: &mut <Self::UpdateParam as SystemParam>::Item<'_, '_>,
        queue: &mut Vec<QueuedAction>,
    ) {
        self.extended.do_start(
            entity,
            &settings.extended,
            &mut param.0,
            queue,
        );
        self.common.do_start(
            entity,
            &settings.common,
            &mut param.1,
            queue,
        );
    }

    fn do_stop(
        &mut self,
        entity: Entity,
        settings: &Self::Settings,
        param: &mut <Self::UpdateParam as SystemParam>::Item<'_, '_>,
        queue: &mut Vec<QueuedAction>,
    ) {
        self.extended.do_stop(
            entity,
            &settings.extended,
            &mut param.0,
            queue,
        );
        self.common.do_stop(
            entity,
            &settings.common,
            &mut param.1,
            queue,
        );
    }

    fn set_slot(
        &mut self,
        timing: ScriptActionTiming,
        slot: &str,
        state: bool,
    ) {
        self.common.set_slot(timing, slot, state);
        self.extended.set_slot(timing, slot, state);
    }

    fn has_slot(&self, slot: &str) -> bool {
        self.common.has_slot(slot) || self.extended.has_slot(slot)
    }

    fn take_slots(&mut self, timing: ScriptActionTiming) -> HashSet<String> {
        let mut r = self.common.take_slots(timing);
        r.extend(self.extended.take_slots(timing));
        r
    }

    fn clear_slots(&mut self, timing: ScriptActionTiming) {
        self.common.clear_slots(timing);
        self.extended.clear_slots(timing);
    }
}

impl<T: ScriptRunIf> ScriptRunIf for ExtendedScriptRunIf<T> {
    type Tracker = ExtendedScriptTracker<T::Tracker>;
}

impl<T: ScriptActionParams> ScriptActionParams for ExtendedScriptParams<T> {
    type ShouldRunParam = (
        T::ShouldRunParam,
        <CommonScriptParams as ScriptActionParams>::ShouldRunParam,
    );
    type Tracker = ExtendedScriptTracker<T::Tracker>;

    fn should_run(
        &self,
        entity: Entity,
        tracker: &mut Self::Tracker,
        action_id: ActionId,
        (param_ext, param_common): &mut <Self::ShouldRunParam as SystemParam>::Item<'_, '_>,
    ) -> Result<(), ScriptUpdateResult> {
        if let Err(r) = self.extended.should_run(
            entity,
            &mut tracker.extended,
            action_id,
            param_ext,
        ) {
            Err(r)
        } else {
            self.common.should_run(
                entity,
                &mut tracker.common,
                action_id,
                param_common,
            )
        }
    }
}

impl<T> ScriptAction for ExtendedScriptAction<T>
where
    T: ScriptAction,
{
    type ActionParams = ExtendedScriptParams<T::ActionParams>;
    type Param = (
        T::Param,
        <CommonScriptAction as ScriptAction>::Param,
    );
    type Tracker = ExtendedScriptTracker<T::Tracker>;

    fn run(
        &self,
        entity: Entity,
        timing: ScriptActionTiming,
        actionparams: &Self::ActionParams,
        tracker: &mut Self::Tracker,
        (param_ext, param_common): &mut <Self::Param as SystemParam>::Item<
            '_,
            '_,
        >,
    ) -> ScriptUpdateResult {
        match self {
            ExtendedScriptAction::Extended(action) => action.run(
                entity,
                timing,
                &actionparams.extended,
                &mut tracker.extended,
                param_ext,
            ),
            ExtendedScriptAction::Common(action) => action.run(
                entity,
                timing,
                &actionparams.common,
                &mut tracker.common,
                param_common,
            ),
        }
    }
}

impl ScriptAsset for Script {
    type Action = CommonScriptAction;
    type ActionParams = CommonScriptParams;
    type BuildParam = ();
    type RunIf = CommonScriptRunIf;
    type Settings = CommonScriptSettings;
    type Tracker = CommonScriptTracker;

    fn settings(&self) -> Self::Settings {
        self.settings.clone().unwrap_or_default()
    }

    fn build(
        &self,
        mut builder: ScriptRuntimeBuilder<Self>,
        _entity: Entity,
        _param: &mut <Self::BuildParam as SystemParam>::Item<'_, '_>,
    ) -> ScriptRuntimeBuilder<Self> {
        builder.replace_config(&self.config);
        for action in self.script.iter() {
            builder = builder.add_action(
                &action.run_if,
                &action.action,
                &action.params,
            );
        }
        builder
    }
}
