use core::fmt;
use std::num::ParseIntError;
use std::str::FromStr;

use bevy::ecs::schedule::ScheduleLabel;

use crate::prelude::*;

pub struct GameTimePlugin;

impl Plugin for GameTimePlugin {
    fn build(&self, app: &mut App) {
        app.init_schedule(GameTickUpdate);
        app.init_resource::<GameTime>();
        app.add_system(update_gametime);
        app.add_system(run_gametickupdate_schedule.after(update_gametime));
        app.configure_set(
            GameTickSet::Pre
                .before(run_gametickupdate_schedule)
                .after(update_gametime),
        );
        app.configure_set(GameTickSet::Post.after(run_gametickupdate_schedule));
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

/// Our alternative to `FixedUpdate`
#[derive(ScheduleLabel, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct GameTickUpdate;

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
        world.resource_mut::<GameTime>().tick += 1;
    }
}

/// Special value to indicate that something should happen "every N ticks"
///
/// This can be parsed from a string
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[derive(SerializeDisplay, DeserializeFromStr)]
pub struct TickQuant {
    /// Do the thing every Nth tick ...
    pub n: u32,
    /// ... offsetted by this many ticks
    /// (use this to do things "on the off-beat", in musical terms)
    pub offset: u32,
}

impl TickQuant {
    /// Quantize a tick value
    ///
    /// Takes the raw tick value and returns the last value according
    /// to the quantization parameters.
    pub fn apply(self, tick: u64) -> u64 {
        let tick = tick + self.offset as u64;
        let rem = tick % self.n as u64;
        tick - rem
    }

    /// Get a value in these units
    pub fn convert(self, tick: u64) -> u64 {
        let tick = tick + self.offset as u64;
        tick / self.n as u64
    }
}

impl fmt::Display for TickQuant {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        if self.offset != 0 {
            write!(f, "{}+{}", self.n, self.offset)
        } else {
            write!(f, "{}", self.n)
        }
    }
}

impl FromStr for TickQuant {
    type Err = ParseIntError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut r = TickQuant { n: 0, offset: 0 };

        // look for a "+" sign
        // if there is none, then we expect an integer to use for `n`
        // if there is one, we expect integers on either side to use for `n`+`offset`
        let mut parts_iter = s.split('+');

        if let Some(part0) = parts_iter.next() {
            r.n = part0.trim().parse()?;
        } else {
            r.n = s.trim().parse()?;
        }

        if let Some(part1) = parts_iter.next() {
            r.offset = part1.trim().parse()?;
        }

        Ok(r)
    }
}

/// Run condition to run something "every N ticks"
pub fn at_tick_multiples(quant: TickQuant) -> impl FnMut(Res<GameTime>) -> bool {
    move |gametime: Res<GameTime>| (gametime.tick() + quant.offset as u64) % quant.n as u64 == 0
}

#[cfg(test)]
mod test {
    use super::TickQuant;
    #[test]
    fn display_framequant() {
        let a = TickQuant { n: 0, offset: 0 };
        assert_eq!(a.to_string(), "0");
        let b = TickQuant { n: 3, offset: 0 };
        assert_eq!(b.to_string(), "3");
        let c = TickQuant { n: 0, offset: 4 };
        assert_eq!(c.to_string(), "0+4");
        let d = TickQuant { n: 8, offset: 2 };
        assert_eq!(d.to_string(), "8+2");
    }
    #[test]
    fn parse_framequant() {
        let a = "13".parse::<TickQuant>();
        assert_eq!(a, Ok(TickQuant { n: 13, offset: 0 }));
        let b = "  2\n".parse::<TickQuant>();
        assert_eq!(b, Ok(TickQuant { n: 2, offset: 0 }));
        let c = "3+1".parse::<TickQuant>();
        assert_eq!(c, Ok(TickQuant { n: 3, offset: 1 }));
        let d = " 6 + 2  ".parse::<TickQuant>();
        assert_eq!(d, Ok(TickQuant { n: 6, offset: 2 }));
        let e = "garbage".parse::<TickQuant>();
        assert!(e.is_err());
        let f = " 4+garbage".parse::<TickQuant>();
        assert!(f.is_err());
        let g = "garbage + 4".parse::<TickQuant>();
        assert!(g.is_err());
        let h = "gar + bage".parse::<TickQuant>();
        assert!(h.is_err());
    }
}
