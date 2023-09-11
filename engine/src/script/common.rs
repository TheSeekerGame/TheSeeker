use bevy::ecs::system::lifetimeless::*;

use super::*;
use crate::assets::script::*;
use crate::script::label::EntityLabels;

pub struct CommonScriptPlugin;

impl Plugin for CommonScriptPlugin {
    fn build(&self, app: &mut App) {
        app.add_script_runtime::<Script>();
    }
}

#[derive(Bundle)]
pub struct ScriptBundle {
    pub key: AssetKey<Script>,
}

#[derive(Default)]
pub struct CommonScriptTracker {
    start_tick: u64,
    next_tick_id: usize,
    tick: Vec<(u64, ActionId)>,
    tickquant: Vec<(TickQuant, ActionId)>,
    start_time: Duration,
    next_time_id: usize,
    time: Vec<(Duration, ActionId)>,
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

    fn init<'w>(
        &mut self,
        entity: Entity,
        settings: &Self::Settings,
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
            self.start_tick = quant.apply(self.start_tick);
        }
    }

    fn transfer_progress(&mut self, other: &Self) {
        self.start_tick = other.start_tick;
        self.start_time = other.start_time;
    }

    fn track_action(&mut self, run_if: &Self::RunIf, action_id: ActionId) {
        match run_if {
            CommonScriptRunIf::Tick(tick) => {
                self.tick.push((*tick, action_id));
            },
            CommonScriptRunIf::TickQuant(quant) => {
                self.tickquant.push((*quant, action_id));
            },
            CommonScriptRunIf::Millis(millis) => {
                self.time.push((
                    Duration::from_millis(*millis),
                    action_id,
                ));
            },
            CommonScriptRunIf::Time(timespec) => {
                self.time.push((Duration::from(*timespec), action_id));
            },
        }
    }

    fn finalize(&mut self) {
        self.tick.sort_unstable_by_key(|(tick, _)| *tick);
        self.time.sort_unstable_by_key(|(duration, _)| *duration);
    }

    fn update<'w>(
        &mut self,
        _entity: Entity,
        _settings: &Self::Settings,
        (time, game_time): &mut <Self::UpdateParam as SystemParam>::Item<'w, '_>,
        queue: &mut Vec<ActionId>,
    ) -> ScriptUpdateResult {
        // check any time actions
        while self.next_time_id < self.time.len() {
            let next = &self.time[self.next_time_id];
            if time.elapsed() - self.start_time > next.0 {
                queue.push(next.1);
                self.next_time_id += 1;
            } else {
                break;
            }
        }
        // check any tick actions
        while self.next_tick_id < self.tick.len() {
            let next = &self.tick[self.next_tick_id];
            if game_time.tick() - self.start_tick > next.0 {
                queue.push(next.1);
                self.next_tick_id += 1;
            } else {
                break;
            }
        }
        // check any tickquant actions
        for (quant, action_id) in &self.tickquant {
            if quant.check(game_time.tick()) {
                queue.push(*action_id);
            }
        }
        if self.next_time_id >= self.time.len()
            && self.next_tick_id >= self.tick.len()
            && self.tickquant.is_empty()
        {
            ScriptUpdateResult::Finished
        } else {
            ScriptUpdateResult::NormalRun
        }
    }
}

impl ScriptRunIf for CommonScriptRunIf {
    type Tracker = CommonScriptTracker;
}

impl ScriptAction for CommonScriptAction {
    type Param = (SRes<EntityLabels>, SCommands);
    type Tracker = CommonScriptTracker;

    fn run<'w>(
        &self,
        entity: Entity,
        _tracker: &mut Self::Tracker,
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
                commands.spawn(ScriptBundle {
                    key: asset_key.into(),
                });
                ScriptUpdateResult::NormalRun
            }
        }
    }
}

#[derive(Default)]
pub struct ExtendedScriptTracker<T: ScriptTracker> {
    extended: T,
    common: CommonScriptTracker,
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

    fn init<'w>(
        &mut self,
        entity: Entity,
        settings: &Self::Settings,
        param: &mut <Self::InitParam as SystemParam>::Item<'w, '_>,
    ) {
        self.extended.init(entity, &settings.extended, &mut param.0);
        self.common.init(entity, &settings.common, &mut param.1);
    }

    fn transfer_progress(&mut self, other: &Self) {
        self.extended.transfer_progress(&other.extended);
        self.common.transfer_progress(&other.common);
    }

    fn track_action(&mut self, run_if: &Self::RunIf, action_id: ActionId) {
        match run_if {
            ExtendedScriptRunIf::Extended(run_if) => {
                self.extended.track_action(run_if, action_id);
            },
            ExtendedScriptRunIf::Common(run_if) => {
                self.common.track_action(run_if, action_id);
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
}

impl<T: ScriptRunIf> ScriptRunIf for ExtendedScriptRunIf<T> {
    type Tracker = ExtendedScriptTracker<T::Tracker>;
}

impl<T: ScriptAction> ScriptAction for ExtendedScriptAction<T> {
    type Param = (
        T::Param,
        <CommonScriptAction as ScriptAction>::Param,
    );
    type Tracker = ExtendedScriptTracker<T::Tracker>;

    fn run<'w>(
        &self,
        entity: Entity,
        tracker: &mut Self::Tracker,
        (param_ext, param_common): &mut <Self::Param as SystemParam>::Item<'w, '_>,
    ) -> ScriptUpdateResult {
        match self {
            ExtendedScriptAction::Extended(action) => {
                action.run(entity, &mut tracker.extended, param_ext)
            },
            ExtendedScriptAction::Common(action) => {
                action.run(
                    entity,
                    &mut tracker.common,
                    param_common,
                )
            },
        }
    }
}

impl ScriptAsset for Script {
    type Action = CommonScriptAction;
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
        for action in self.script.iter() {
            builder = builder.add_action(&action.run_if, &action.action);
        }
        builder
    }
}
