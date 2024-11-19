// plugin for demonstrating bevy v0.13 system stepping using egui interface
// Released under the MIT License

use std::collections::HashMap;

use bevy::{
    app::MainScheduleOrder,
    ecs::schedule::{InternedScheduleLabel, NodeId, ScheduleLabel, Stepping},
    prelude::*,
};
use bevy_egui::{egui, EguiContexts};

#[derive(Default)]
pub struct SteppingEguiPlugin {
    schedule_labels: Vec<InternedScheduleLabel>,
}

impl SteppingEguiPlugin {
    /// add a schedule to be stepped when stepping is enabled
    pub fn add_schedule(mut self, label: impl ScheduleLabel) -> SteppingEguiPlugin {
        self.schedule_labels.push(label.intern());
        self
    }
}

impl Plugin for SteppingEguiPlugin {
    fn build(&self, app: &mut App) {
        // app.add_plugins(EguiPlugin);

        // create & insert dedicated stepping schedule
        app.init_schedule(SteppingSchedule);
        let mut order = app.world.resource_mut::<MainScheduleOrder>();
        order.insert_after(PreUpdate, SteppingSchedule);

        // create & configure the stepping resource
        let mut stepping = Stepping::new();
        for label in &self.schedule_labels {
            stepping.add_schedule(*label);
        }
        app.insert_resource(stepping);
        app.insert_resource(SteppingEguiState::default());

        app.add_systems(
            SteppingSchedule,
            (draw_window, handle_input),
        );
    }
}

/// Independent [`Schedule`] for stepping systems.
///
/// The stepping systems must run in their own schedule to be able to inspect
/// all the other schedules in the [`App`].  This is because the currently
/// executing schedule is removed from the [`Schedules`] resource while it is
/// being run.
#[derive(Debug, Hash, PartialEq, Eq, Clone, ScheduleLabel)]
struct SteppingSchedule;

/// copied from ecs::schedule::stepping; need to add methods to interrogate
/// system behavior state
#[derive(Debug, Copy, Clone)]
enum SystemBehavior {
    /// System will always run regardless of stepping action
    AlwaysRun,

    /// System will never run while stepping is enabled
    NeverRun,

    /// When [`Action::Waiting`] this system will not be run
    /// When [`Action::Step`] this system will be stepped
    /// When [`Action::Continue`] system execution will stop before executing
    /// this system unless its the first system run when continuing
    Break,

    /// When [`Action::Waiting`] this system will not be run
    /// When [`Action::Step`] this system will be stepped
    /// When [`Action::Continue`] this system will be run
    Continue,
}

#[derive(Debug, Resource, Default)]
struct SteppingEguiState {
    // vector of schedule/nodeid -> text index offset
    systems: HashMap<(InternedScheduleLabel, NodeId), SystemBehavior>,
}

fn draw_window(
    mut stepping: ResMut<Stepping>,
    mut state: ResMut<SteppingEguiState>,
    mut contexts: EguiContexts,
    schedules: Res<Schedules>,
) {
    egui::Window::new("system stepping").show(contexts.ctx_mut(), |ui| {
        egui::Grid::new("stepping_controls")
            .num_columns(3)
            .show(ui, |ui| {
                let mut enabled = stepping.is_enabled();
                if ui.checkbox(&mut enabled, "enable").clicked() {
                    if enabled {
                        stepping.enable();
                    } else {
                        stepping.disable();
                    }
                }
                if enabled {
                    if ui.button("step").clicked() {
                        stepping.step_frame();
                    }
                    if ui.button("continue").clicked() {
                        stepping.continue_frame();
                    }
                }
                ui.end_row();
            });
        ui.separator();

        // schedules will not be populated until after stepping has been
        // enabled.
        let Ok(schedule_order) = stepping.schedules() else {
            ui.label("Enable stepping to continue");
            return;
        };

        let mut behavior_updates = Vec::new();

        for label in schedule_order {
            let Some(schedule) = schedules.get(*label) else {
                ui.label(format!("error: Schedule not found: {:?}", label));
                continue;
            };
            ui.heading(format!("{:?}", label));

            let Ok(systems) = schedule.systems() else {
                ui.label(format!("error: {:?} has no systems", label));
                continue;
            };

            egui::Grid::new(format!("stepping_schedule_{:?}", label))
                // cursor, disabled/enabled/always, breakpoint
                .num_columns(5)
                .striped(true)
                .spacing([5.0, 5.0])
                .show(ui, |ui| {
                    ui.label("cursor").on_hover_text("Stepping cursor position");
                    ui.label("enable").on_hover_text("System enabled");
                    ui.label("always").on_hover_text("System ignored by stepping; system will always run regardless of stepping state.");
                    ui.label("break").on_hover_text("Breakpoint; when set stepping will stop before executing this system when continuing.");
                    ui.label("system name");
                    ui.end_row();

                    for (node_id, system) in systems {
                        if let Some((cursor_label, cursor_node_id)) = stepping.cursor() {
                            if cursor_label == *label && cursor_node_id == node_id {
                                ui.label("👉");
                            } else {
                                ui.label(" ");
                            }
                        } else {
                            ui.label(" ");
                        }

                        let (mut enabled, mut ignore, mut breakpoint) =
                            match state.systems.get(&(*label, node_id)) {
                                Some(SystemBehavior::AlwaysRun) => (true, true, false),
                                Some(SystemBehavior::NeverRun) => (false, false, false),
                                Some(SystemBehavior::Break) => (true, false, true),
                                Some(SystemBehavior::Continue) => (true, false, false),
                                None => (true, false, false),
                            };

                        let mut update = None;

                        ui.add_enabled_ui(!ignore, |ui| {
                            if ui.checkbox(&mut enabled, "").clicked() {
                                if enabled {
                                    update = Some(SystemBehavior::Continue);
                                } else {
                                    update = Some(SystemBehavior::NeverRun);
                                }
                            }
                        });
                        if ui.checkbox(&mut ignore, "").clicked() {
                            if ignore {
                                update = Some(SystemBehavior::AlwaysRun);
                            } else {
                                update = Some(SystemBehavior::Continue);
                            }
                        }
                        if ui.checkbox(&mut breakpoint, "").clicked() {
                            if breakpoint {
                                update = Some(SystemBehavior::Break);
                            } else {
                                update = Some(SystemBehavior::Continue);
                            }
                        };

                        ui.label(system.name());
                        ui.end_row();

                        if let Some(behavior) = update {
                            behavior_updates.push((*label, node_id, behavior));
                        }
                    }
                });

            ui.separator();
        }

        // apply any behavior updates (should only be one)
        for (schedule, node, behavior) in behavior_updates {
            match behavior {
                SystemBehavior::AlwaysRun => stepping.always_run_node(schedule, node),
                SystemBehavior::NeverRun => stepping.never_run_node(schedule, node),
                SystemBehavior::Break => stepping.set_breakpoint_node(schedule, node),
                SystemBehavior::Continue => stepping.clear_node(schedule, node),
            };
            state.systems.insert((schedule, node), behavior);
        }
    });
}

fn handle_input(keyboard_input: Res<ButtonInput<KeyCode>>, mut stepping: ResMut<Stepping>) {
    // grave key to toggle stepping mode for the FixedUpdate schedule
    if keyboard_input.just_pressed(KeyCode::Backquote) {
        if stepping.is_enabled() {
            stepping.disable();
        } else {
            stepping.enable();
        }
    }

    if !stepping.is_enabled() {
        return;
    }

    // space key will step the remainder of this frame
    if keyboard_input.just_pressed(KeyCode::Space) {
        stepping.continue_frame();
    } else if keyboard_input.just_pressed(KeyCode::KeyS) {
        stepping.step_frame();

    // hold enter to continue as fast as possible
    } else if keyboard_input.pressed(KeyCode::Enter) {
        stepping.continue_frame();
    }
}
