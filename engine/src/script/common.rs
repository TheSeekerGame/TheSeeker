use bevy::ecs::system::lifetimeless::*;

use super::*;
use crate::assets::script::*;
use crate::prelude::*;
use crate::script::label::EntityLabels;

pub struct CommonScriptPlugin;

impl Plugin for CommonScriptPlugin {
    fn build(&self, app: &mut App) {
        app.add_script_runtime::<Script>();
    }
}

#[derive(Default)]
pub struct CommonScriptTracker {
    next_tick_id: usize,
    tick: Vec<(u64, ActionId)>,
    tickquant: Vec<(TickQuant, ActionId)>,
    next_time_id: usize,
    time: Vec<(Duration, ActionId)>,
}

impl ScriptTracker for CommonScriptTracker {
    type Param = (SRes<Time>, SRes<GameTime>);
    type RunIf = CommonScriptRunIf;

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
        (time, game_time): &mut <Self::Param as SystemParam>::Item<'w, '_>,
        queue: &mut Vec<ActionId>,
    ) -> ScriptUpdateResult {
        // check any time actions
        while self.next_time_id < self.time.len() {
            let next = &self.time[self.next_time_id];
            if time.elapsed() > next.0 {
                queue.push(next.1);
                self.next_time_id += 1;
            } else {
                break;
            }
        }
        // check any tick actions
        while self.next_tick_id < self.tick.len() {
            let next = &self.tick[self.next_tick_id];
            if game_time.tick() > next.0 {
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

    fn run<'w>(
        &self,
        entity: Entity,
        (ref elabels, ref mut commands): &mut <Self::Param as SystemParam>::Item<'w, '_>,
    ) {
        match self {
            CommonScriptAction::RunCli { cli } => {
                for cli in cli.iter() {
                    commands.run_clicommand(cli);
                }
            },
            CommonScriptAction::DespawnEntity { label } => {
                if let Some(label) = label {
                    for e in elabels.iter_label_entities(label) {
                        commands.entity(*e).despawn_recursive();
                    }
                } else {
                    commands.entity(entity).despawn_recursive();
                }
            },
            CommonScriptAction::SpawnScene {
                scene_asset_key,
                as_child,
                parent_label,
            } => {},
        }
    }
}

#[derive(Default)]
pub struct ExtendedScriptTracker<T: ScriptTracker> {
    extended: T,
    common: CommonScriptTracker,
}

impl<T: ScriptTracker> ScriptTracker for ExtendedScriptTracker<T> {
    type Param = (
        T::Param,
        <<CommonScriptRunIf as ScriptRunIf>::Tracker as ScriptTracker>::Param,
    );
    type RunIf = ExtendedScriptRunIf<T::RunIf>;

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
        param: &mut <Self::Param as SystemParam>::Item<'w, '_>,
        queue: &mut Vec<ActionId>,
    ) -> ScriptUpdateResult {
        let r_extended = self.extended.update(entity, &mut param.0, queue);
        let r_common = self.common.update(entity, &mut param.1, queue);
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

    fn run<'w>(
        &self,
        entity: Entity,
        (param_ext, param_common): &mut <Self::Param as SystemParam>::Item<'w, '_>,
    ) {
        match self {
            ExtendedScriptAction::Extended(action) => action.run(entity, param_ext),
            ExtendedScriptAction::Common(action) => action.run(entity, param_common),
        }
    }
}

impl ScriptAsset for Script {
    type Action = CommonScriptAction;
    type RunIf = CommonScriptRunIf;
    type Tracker = CommonScriptTracker;

    fn init(&self) -> ScriptRuntime<Self> {
        let mut builder = ScriptRuntimeBuilder::new();
        for action in self.script.iter() {
            builder = builder.add_action(&action.run_if, &action.action);
        }
        builder.build()
    }
}
