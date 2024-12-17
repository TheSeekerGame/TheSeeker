use bevy::ecs::schedule::{ExecutorKind, ScheduleLabel};

use crate::prelude::*;

pub struct GameTimePlugin;

impl Plugin for GameTimePlugin {
    fn build(&self, app: &mut App) {
        app.init_schedule(GameTickUpdate);
        app.init_schedule(GameTickPost);
        app.edit_schedule(GameTickUpdate, |s| {
            s.set_executor_kind(ExecutorKind::SingleThreaded);
        });
        app.edit_schedule(GameTickPost, |s| {
            s.set_executor_kind(ExecutorKind::SingleThreaded);
        });
        app.init_resource::<GameTime>();
        app.add_systems(
            Update,
            (
                update_gametime,
                run_gametickupdate_schedule.after(update_gametime),
            ),
        );
        app.configure_sets(
            Update,
            GameTickSet::Pre
                .before(run_gametickupdate_schedule)
                .after(update_gametime),
        );
        app.configure_sets(
            Update,
            GameTickSet::Post.after(run_gametickupdate_schedule),
        );
    }
}

/// Apply this to anything that relies on `GameTime`
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GameTickSet {
    /// Runs after `GameTime` is updated, but before the `GameTickUpdate` schedule
    Pre,
    /// Runs after the `GameTickUpdate` schedule
    Post,
}

/// This is when old "game tick events" are cleared (in `GameTickUpdate` schedule)
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct GameTickEventClearSet;

pub trait GameTimeAppExt {
    fn add_gametick_event<T: Event>(&mut self) -> &mut Self;
}

impl GameTimeAppExt for App {
    fn add_gametick_event<T: Event>(&mut self) -> &mut Self {
        if !self.world.contains_resource::<Events<T>>() {
            self.init_resource::<Events<T>>();
            self.add_systems(
                GameTickUpdate,
                minimal_event_update_system::<T>.in_set(GameTickEventClearSet),
            );
        } else {
            warn!("Attempted to add a Game Tick event type that had already been added as an event before!");
        }
        self
    }
}

fn minimal_event_update_system<T: Event>(mut events: ResMut<Events<T>>) {
    events.update();
}

/// Our alternative to `FixedUpdate`
#[derive(ScheduleLabel, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct GameTickUpdate;

/// Run after `GameTickUpdate`
/// used for running systems which update input state between GameTickUpdate runs
#[derive(ScheduleLabel, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct GameTickPost;

#[derive(Resource, Debug)]
pub struct GameTime {
    /// The time base rate
    pub hz: f64,
    tick: u64,
    new_ticks: u64,
    total_ticks: u64,
    overstep: f64,
    last_update: Duration,
}

impl Default for GameTime {
    fn default() -> Self {
        Self {
            hz: 96.0,
            tick: 0,
            new_ticks: 0,
            total_ticks: 0,
            overstep: 0.0,
            last_update: Duration::new(0, 0),
        }
    }
}

impl GameTime {
    /// Create with a non-default tick rate
    pub fn new(hz: f64) -> Self {
        Self {
            hz,
            ..Default::default()
        }
    }

    /// Get the current tick number to be simulated
    ///
    /// Increments with every run of the `GameTickUpdate` schedule, until it reaches
    /// the `total_ticks` target value.
    pub fn tick(&self) -> u64 {
        self.tick
    }

    /// Get the number of new ticks to be simulated this Bevy frame update
    pub fn new_ticks(&self) -> u64 {
        self.new_ticks
    }

    /// Get the total target number of ticks as of this Bevy frame update
    pub fn total_ticks(&self) -> u64 {
        self.total_ticks
    }

    /// Get the leftover partial tick to be carried over to the next Bevy frame update
    pub fn overstep(&self) -> f64 {
        self.overstep
    }

    /// Get the "elapsed" time when we last updated
    pub fn last_update(&self) -> Duration {
        self.last_update
    }

    /// Reset tick counters to zero, set the last update to now, keep the `hz` value
    pub fn reset(&mut self, now: Duration) {
        *self = Self {
            hz: self.hz,
            last_update: now,
            ..Default::default()
        };
    }

    /// Every Bevy frame, this gets called to advance the tick counters
    pub fn update(&mut self, time: &Time) {
        let now = time.elapsed();
        let delta = now - self.last_update;
        self.last_update = now;

        let delta_f64 = delta.as_secs_f64();
        let new_ticks = delta_f64 * self.hz + self.overstep;
        self.total_ticks += new_ticks as u64;
        self.overstep = new_ticks.fract();
        self.new_ticks = self.total_ticks - self.tick;
    }

    /// returns the amount of time since start; ie: ticks * tick length
    pub fn time_in_seconds(&self) -> f64 {
        self.tick() as f64 * self.seconds_per_tick()
    }

    /// Convenience function to save you the math
    pub fn seconds_per_tick(&self) -> f64 {
        1.0 / self.hz
    }
}

/// Update `GameTime` every frame
pub fn update_gametime(time: Res<Time>, mut gametime: ResMut<GameTime>) {
    gametime.update(&time);
}

/// Our alternative to Bevy's fixed timestep, based on `GameTime`
pub fn run_gametickupdate_schedule(world: &mut World) {
    loop {
        let gametime = world.resource::<GameTime>();
        if gametime.tick >= gametime.total_ticks {
            break;
        }
        world.run_schedule(GameTickUpdate);
        world.run_schedule(GameTickPost);
        world.resource_mut::<GameTime>().tick += 1;
    }
}

/// Run condition to run something "every N ticks"
pub fn at_tick_multiples(quant: Quant) -> impl FnMut(Res<GameTime>) -> bool {
    move |gametime: Res<GameTime>| {
        (gametime.tick() + quant.offset as u64) % quant.n as u64 == 0
    }
}
