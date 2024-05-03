use bevy::ecs::system::lifetimeless::*;

use super::*;
use crate::assets::script::*;
use crate::data::OneOrMany;
use crate::script::label::EntityLabels;

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
    q_extra: Vec<ActionId>,
    q_delayed: Vec<(u64, ActionId)>,
    old_key: Option<String>,
    runcount: u32,
}

impl CommonScriptTracker {
    pub fn new_with_offset(start_tick: u64, start_time: Duration) -> Self {
        Self {
            start_tick,
            start_time,
            ..Default::default()
        }
    }
}

impl ScriptTracker for CommonScriptTracker {
    type InitParam = (
        SRes<Time>,
        SRes<GameTime>,
        Option<SRes<LevelLoadTime>>,
        SQuery<&'static TimeBase>,
        SQuery<&'static ScriptTickQuant>,
    );
    type RunIf = CommonScriptRunIf;
    type Settings = CommonScriptSettings;
    type UpdateParam = (SRes<Time>, SRes<GameTime>);
    type ActionParams = CommonScriptParams;

    fn init<'w>(
        &mut self,
        entity: Entity,
        settings: &Self::Settings,
        metadata: &ScriptMetadata,
        (time, gametime, leveltime, query_tb, query_quant): &mut <Self::InitParam as SystemParam>::Item<'w, '_>,
    ) {
        let time_base = query_tb.get(entity).unwrap_or(&settings.time_base);
        let tick_quant = query_quant
            .get(entity)
            .ok()
            .or_else(|| settings.tick_quant.as_ref());
        match time_base {
            TimeBase::Relative => {
                self.start_tick = gametime.tick();
                self.start_time = time.elapsed();
            },
            TimeBase::Level => {
                if let Some(leveltime) = leveltime {
                    self.start_tick = leveltime.tick;
                    self.start_time = leveltime.time;
                } else {
                    error!("Script with time base 'Level' wants to run, but level start time is unknown! (are we in-game?)");
                    self.start_tick = 0;
                    self.start_time = Duration::new(0, 0);
                }
            },
            TimeBase::Startup => {
                self.start_tick = 0;
                self.start_time = Duration::new(0, 0);
            },
        }
        if let Some(ScriptTickQuant(quant)) = tick_quant {
            self.start_tick = quant.apply(self.start_tick as i64) as u64;
        }
        self.old_key = metadata.key_previous.clone();
        self.runcount = metadata.runcount;
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
                match tick {
                    OneOrMany::Single(tick) => {
                        self.tick_actions.push((*tick, action_id));
                    }
                    OneOrMany::Many(ticks) => {
                        for tick in ticks.iter() {
                            self.tick_actions.push((*tick, action_id));
                        }
                    }
                }
            },
            CommonScriptRunIf::TickQuant(quant) => {
                self.tickquant_actions.push((*quant, action_id));
            },
            CommonScriptRunIf::Millis(millis) => {
                match millis {
                    OneOrMany::Single(millis) => {
                        self.time_actions.push((
                            Duration::from_millis(*millis),
                            action_id,
                        ));
                    }
                    OneOrMany::Many(millis) => {
                        for millis in millis.iter() {
                            self.time_actions.push((
                                Duration::from_millis(*millis),
                                action_id,
                            ));
                        }
                    }
                }
            },
            CommonScriptRunIf::Time(timespec) => {
                match timespec {
                    OneOrMany::Single(timespec) => {
                        self.time_actions.push((Duration::from(*timespec), action_id));
                    }
                    OneOrMany::Many(timespecs) => {
                        for timespec in timespecs.iter() {
                            self.time_actions.push((Duration::from(*timespec), action_id));
                        }
                    }
                }
            },
            CommonScriptRunIf::SlotEnable(slot) => {
                if let Some(entry) = self.slot_enable_actions.get_mut(slot.as_str()) {
                    entry.push(action_id);
                } else {
                    self.slot_enable_actions.insert(slot.clone(), vec![action_id]);
                }
            }
            CommonScriptRunIf::SlotDisable(slot) => {
                if let Some(entry) = self.slot_disable_actions.get_mut(slot.as_str()) {
                    entry.push(action_id);
                } else {
                    self.slot_disable_actions.insert(slot.clone(), vec![action_id]);
                }
            }
            CommonScriptRunIf::PlaybackControl(PlaybackControl::Start) => {
                self.start_actions.push(action_id);
            }
            CommonScriptRunIf::PlaybackControl(PlaybackControl::Stop) => {
                self.stop_actions.push(action_id);
            }
        }
    }

    fn finalize(&mut self) {
        self.tick_actions.sort_by_key(|(tick, _)| *tick);
        self.time_actions.sort_by_key(|(duration, _)| *duration);
    }

    fn update<'w>(
        &mut self,
        _entity: Entity,
        _settings: &Self::Settings,
        (time, game_time): &mut <Self::UpdateParam as SystemParam>::Item<'w, '_>,
        queue: &mut Vec<ActionId>,
    ) -> ScriptUpdateResult {
        // any delayed actions
        // we don't remove them here, only trigger them to run
        // they will manage themselves in/out of `q_delayed` when they run
        for (tick, action_id) in self.q_delayed.iter() {
            if game_time.tick() >= *tick {
                queue.push(*action_id);
            }
        }

        // check any time actions
        while self.next_time_id < self.time_actions.len() {
            let next = &self.time_actions[self.next_time_id];
            if time.elapsed() - self.start_time > next.0 {
                queue.push(next.1);
                self.next_time_id += 1;
            } else {
                break;
            }
        }
        // check any tick actions
        while self.next_tick_id < self.tick_actions.len() {
            let next = &self.tick_actions[self.next_tick_id];
            if game_time.tick() - self.start_tick > next.0 {
                queue.push(next.1);
                self.next_tick_id += 1;
            } else {
                break;
            }
        }
        // check any tickquant actions
        for (quant, action_id) in &self.tickquant_actions {
            if quant.check(game_time.tick() as i64) {
                queue.push(*action_id);
            }
        }
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
        queue: &mut Vec<ActionId>,
    ) {
        queue.append(&mut self.q_extra);
    }

    fn do_start<'w>(
        &mut self,
        _entity: Entity,
        _settings: &Self::Settings,
        _param: &mut <Self::UpdateParam as SystemParam>::Item<'w, '_>,
        queue: &mut Vec<ActionId>,
    ) {
        queue.append(&mut self.start_actions);
    }

    fn do_stop<'w>(
        &mut self,
        _entity: Entity,
        _settings: &Self::Settings,
        _param: &mut <Self::UpdateParam as SystemParam>::Item<'w, '_>,
        queue: &mut Vec<ActionId>,
    ) {
        queue.append(&mut self.stop_actions);
    }

    fn set_slot(&mut self, slot: &str, state: bool) {
        if state {
            if !self.slots_enabled.contains(slot) {
                self.slots_enabled.insert(slot.to_owned());
                if let Some(actions) = self.slot_enable_actions.get(slot) {
                    self.q_extra.extend_from_slice(&actions);
                }
            }
        } else {
            if self.slots_enabled.contains(slot) {
                self.slots_enabled.remove(slot);
                if let Some(actions) = self.slot_disable_actions.get(slot) {
                    self.q_extra.extend_from_slice(&actions);
                }
            }
        }
    }

    fn has_slot(&self, slot: &str) -> bool {
        self.slots_enabled.contains(slot)
    }

    fn take_slots(&mut self) -> HashSet<String> {
        for slot in self.slots_enabled.iter() {
            if let Some(actions) = self.slot_disable_actions.get(slot) {
                self.q_extra.extend_from_slice(&actions);
            }
        }
        std::mem::take(&mut self.slots_enabled)
    }

    fn clear_slots(&mut self) {
        for slot in self.slots_enabled.iter() {
            if let Some(actions) = self.slot_disable_actions.get(slot) {
                self.q_extra.extend_from_slice(&actions);
            }
        }
        self.slots_enabled.clear()
    }
}

impl ScriptRunIf for CommonScriptRunIf {
    type Tracker = CommonScriptTracker;
}

impl ScriptActionParams for CommonScriptParams {
    type Tracker = CommonScriptTracker;
    type ShouldRunParam = (SRes<Time>, SRes<GameTime>);

    fn should_run<'w>(
        &self,
        _entity: Entity,
        tracker: &mut Self::Tracker,
        action_id: ActionId,
        (_time, game_time): &mut <Self::ShouldRunParam as SystemParam>::Item<'w, '_>,
    ) -> Result<(), ScriptUpdateResult> {
        if let Some(i_delayed) = tracker.q_delayed.iter()
            .position(|(tick, aid)| *tick == game_time.tick() && *aid == action_id)
        {
            tracker.q_delayed.remove(i_delayed);
        } else if let Some(delay_ticks) = self.delay_ticks {
            tracker.q_delayed.push((game_time.tick() + delay_ticks as u64, action_id));
            return Err(ScriptUpdateResult::NormalRun);
        }
        if let Some(lt) = self.if_runcount_lt {
            if !(tracker.runcount < lt) {
                return Err(ScriptUpdateResult::NormalRun);
            }
        }
        if let Some(le) = self.if_runcount_le {
            if !(tracker.runcount <= le) {
                return Err(ScriptUpdateResult::NormalRun);
            }
        }
        if let Some(gt) = self.if_runcount_gt {
            if !(tracker.runcount > gt) {
                return Err(ScriptUpdateResult::NormalRun);
            }
        }
        if let Some(ge) = self.if_runcount_ge {
            if !(tracker.runcount >= ge) {
                return Err(ScriptUpdateResult::NormalRun);
            }
        }
        if let Some(quant) = self.if_runcount_quant {
            if !quant.check(tracker.runcount as i64) {
                return Err(ScriptUpdateResult::NormalRun);
            }
        }
        if let Some(eq) = &self.if_runcount_is {
            let b = match eq {
                OneOrMany::Single(x) => *x == tracker.runcount,
                OneOrMany::Many(x) => x.iter().any(|x| *x == tracker.runcount),
            };
            if !b {
                return Err(ScriptUpdateResult::NormalRun);
            }
        }
        match (&self.if_previous_script_key, &tracker.old_key) {
            (None, _) => {}
            (Some(req), Some(old)) if req == old => {},
            _ => return Err(ScriptUpdateResult::NormalRun),
        }
        if !self.forbid_slots_any.is_empty() &&
            self.forbid_slots_any.iter().any(|s| tracker.slots_enabled.contains(s))
        {
            return Err(ScriptUpdateResult::NormalRun);
        }
        if !self.forbid_slots_all.is_empty() &&
            self.forbid_slots_all.iter().all(|s| tracker.slots_enabled.contains(s))
        {
            return Err(ScriptUpdateResult::NormalRun);
        }
        if !self.require_slots_all.is_empty() &&
           !self.require_slots_all.iter().all(|s| tracker.slots_enabled.contains(s))
        {
            return Err(ScriptUpdateResult::NormalRun);
        }
        if !self.require_slots_any.is_empty() &&
           !self.require_slots_any.iter().any(|s| tracker.slots_enabled.contains(s))
        {
            return Err(ScriptUpdateResult::NormalRun);
        }
        if let Some(rng_pct) = &self.rng_pct {
            let mut rng = thread_rng();
            if !rng.gen_bool((*rng_pct as f64 / 100.0).clamp(0.0, 1.0)) {
                return Err(ScriptUpdateResult::NormalRun);
            }
        }
        Ok(())
    }
}

impl ScriptAction for CommonScriptAction {
    type ActionParams = CommonScriptParams;
    type Param = (SRes<EntityLabels>, SCommands);
    type Tracker = CommonScriptTracker;

    fn run<'w>(
        &self,
        entity: Entity,
        _actionparams: &Self::ActionParams,
        tracker: &mut Self::Tracker,
        (ref elabels, ref mut commands): &mut <Self::Param as SystemParam>::Item<'w, '_>,
    ) -> ScriptUpdateResult {
        match self {
            CommonScriptAction::RunCli { cli } => {
                for cli in cli.iter() {
                    commands.run_clicommand(cli);
                }
                ScriptUpdateResult::NormalRun
            },
            CommonScriptAction::DespawnEntity { label } => {
                if let Some(label) = label {
                    for e in elabels.iter_label_entities(label) {
                        commands.entity(*e).despawn_recursive();
                    }
                } else {
                    commands.entity(entity).despawn_recursive();
                    return ScriptUpdateResult::Terminated;
                }
                ScriptUpdateResult::NormalRun
            },
            CommonScriptAction::SpawnScene {
                asset_key,
                as_child,
                parent_label,
            } => ScriptUpdateResult::NormalRun,
            CommonScriptAction::SpawnScript { asset_key } => {
                let mut player = ScriptPlayer::new();
                player.play_key(asset_key.as_str());
                commands.spawn(ScriptBundle {
                    player,
                });
                ScriptUpdateResult::NormalRun
            },
            CommonScriptAction::SlotEnable { slot } => {
                tracker.set_slot(slot, true);
                ScriptUpdateResult::NormalRun
            }
            CommonScriptAction::SlotDisable { slot } => {
                tracker.set_slot(slot, false);
                ScriptUpdateResult::NormalRun
            }
            CommonScriptAction::SlotToggle { slot } => {
                if tracker.has_slot(slot) {
                    tracker.set_slot(slot, false);
                } else {
                    tracker.set_slot(slot, true);
                }
                ScriptUpdateResult::NormalRun
            }
        }
    }
}

#[derive(Default)]
pub struct ExtendedScriptTracker<T: ScriptTracker> {
    pub extended: T,
    pub common: CommonScriptTracker,
}

impl<T: ScriptTracker> ScriptTracker for ExtendedScriptTracker<T> {
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
    type ActionParams = ExtendedScriptParams<T::ActionParams>;

    fn init<'w>(
        &mut self,
        entity: Entity,
        settings: &Self::Settings,
        metadata: &ScriptMetadata,
        param: &mut <Self::InitParam as SystemParam>::Item<'w, '_>,
    ) {
        self.extended.init(entity, &settings.extended, metadata, &mut param.0);
        self.common.init(entity, &settings.common, metadata, &mut param.1);
    }

    fn transfer_progress(&mut self, other: &Self) {
        self.extended.transfer_progress(&other.extended);
        self.common.transfer_progress(&other.common);
    }

    fn track_action(&mut self, run_if: &Self::RunIf, params: &Self::ActionParams, action_id: ActionId) {
        match run_if {
            ExtendedScriptRunIf::Extended(run_if) => {
                self.extended.track_action(run_if, &params.extended, action_id);
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

    fn update<'w>(
        &mut self,
        entity: Entity,
        settings: &Self::Settings,
        param: &mut <Self::UpdateParam as SystemParam>::Item<'w, '_>,
        queue: &mut Vec<ActionId>,
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
            (ScriptUpdateResult::Terminated, _) | (_, ScriptUpdateResult::Terminated) => {
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
        queue: &mut Vec<ActionId>,
    ) {
        self.extended.queue_extra_actions(&settings.extended, queue);
        self.common.queue_extra_actions(&settings.common, queue);
    }

    fn do_start<'w>(
        &mut self,
        entity: Entity,
        settings: &Self::Settings,
        param: &mut <Self::UpdateParam as SystemParam>::Item<'w, '_>,
        queue: &mut Vec<ActionId>,
    ) {
        self.extended.do_start(entity, &settings.extended, &mut param.0, queue);
        self.common.do_start(entity, &settings.common, &mut param.1, queue);
    }

    fn do_stop<'w>(
        &mut self,
        entity: Entity,
        settings: &Self::Settings,
        param: &mut <Self::UpdateParam as SystemParam>::Item<'w, '_>,
        queue: &mut Vec<ActionId>,
    ) {
        self.extended.do_stop(entity, &settings.extended, &mut param.0, queue);
        self.common.do_stop(entity, &settings.common, &mut param.1, queue);
    }

    fn set_slot(&mut self, slot: &str, state: bool) {
        self.common.set_slot(slot, state);
        self.extended.set_slot(slot, state);
    }

    fn has_slot(&self, slot: &str) -> bool {
        self.common.has_slot(slot) || self.extended.has_slot(slot)
    }

    fn take_slots(&mut self) -> HashSet<String> {
        let mut r = self.common.take_slots();
        r.extend(self.extended.take_slots());
        r
    }

    fn clear_slots(&mut self) {
        self.common.clear_slots();
        self.extended.clear_slots();
    }
}

impl<T: ScriptRunIf> ScriptRunIf for ExtendedScriptRunIf<T> {
    type Tracker = ExtendedScriptTracker<T::Tracker>;
}

impl<T: ScriptActionParams> ScriptActionParams for ExtendedScriptParams<T> {
    type Tracker = ExtendedScriptTracker<T::Tracker>;
    type ShouldRunParam = (
        T::ShouldRunParam,
        <CommonScriptParams as ScriptActionParams>::ShouldRunParam,
    );
    fn should_run<'w>(
        &self,
        entity: Entity,
        tracker: &mut Self::Tracker,
        action_id: ActionId,
        (param_ext, param_common): &mut <Self::ShouldRunParam as SystemParam>::Item<'w, '_>,
    ) -> Result<(), ScriptUpdateResult> {
        if let Err(r) = self.extended.should_run(entity, &mut tracker.extended, action_id, param_ext) {
            Err(r)
        } else {
            self.common.should_run(entity, &mut tracker.common, action_id, param_common)
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

    fn run<'w>(
        &self,
        entity: Entity,
        actionparams: &Self::ActionParams,
        tracker: &mut Self::Tracker,
        (param_ext, param_common): &mut <Self::Param as SystemParam>::Item<'w, '_>,
    ) -> ScriptUpdateResult {
        match self {
            ExtendedScriptAction::Extended(action) => {
                action.run(
                    entity,
                    &actionparams.extended,
                    &mut tracker.extended,
                    param_ext,
                )
            },
            ExtendedScriptAction::Common(action) => {
                action.run(
                    entity,
                    &actionparams.common,
                    &mut tracker.common,
                    param_common,
                )
            },
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

    fn into_settings(&self) -> Self::Settings {
        self.settings.clone().unwrap_or_default()
    }

    fn build<'w>(
        &self,
        mut builder: ScriptRuntimeBuilder<Self>,
        _entity: Entity,
        _param: &mut <Self::BuildParam as SystemParam>::Item<'w, '_>,
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
