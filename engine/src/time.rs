use core::fmt;
use std::num::{ParseFloatError, ParseIntError};
use std::str::FromStr;

use bevy::ecs::schedule::ScheduleLabel;

use crate::prelude::*;

pub struct GameTimePlugin;

impl Plugin for GameTimePlugin {
    fn build(&self, app: &mut App) {
        app.init_schedule(GameTickUpdate);
        app.init_resource::<GameTime>();
        app.add_systems(
            Update,
            (
                update_gametime,
                run_gametickupdate_schedule.after(update_gametime),
            ),
        );
        app.configure_set(
            Update,
            GameTickSet::Pre
                .before(run_gametickupdate_schedule)
                .after(update_gametime),
        );
        app.configure_set(
            Update,
            GameTickSet::Post.after(run_gametickupdate_schedule),
        );
        app.add_systems(
            GameTickUpdate,
            apply_deferred.in_set(GameTickMidFlush),
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

/// Within `GameTickUpdate`, let's have a flush point to order things around.
/// Ofc, we can add more if we need them, but let's try to reuse this one.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct GameTickMidFlush;

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

#[derive(Debug, Default, Clone, Copy, PartialEq)]
#[derive(SerializeDisplay, DeserializeFromStr)]
pub struct TimeSpec {
    hours: u32,
    mins: u32,
    secs: u32,
    fract: f64,
}

impl From<TimeSpec> for Duration {
    fn from(value: TimeSpec) -> Self {
        Duration::from_secs(value.secs as u64 + value.mins as u64 * 60 + value.hours as u64 * 3600)
            + Duration::from_secs_f64(value.fract.abs())
    }
}

#[derive(Debug, Error)]
pub enum ParseTimeSpecError {
    #[error("Wrong number of ':'/'.' separators")]
    InvalidComponents,
    #[error("Invalid Hours")]
    Hours(ParseIntError),
    #[error("Invalid Minutes")]
    Mins(ParseIntError),
    #[error("Invalid Seconds")]
    Secs(ParseIntError),
    #[error("Invalid fractional part")]
    Fract(ParseFloatError),
}

impl FromStr for TimeSpec {
    type Err = ParseTimeSpecError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut r = TimeSpec::default();

        let mut dotsplit = s.split('.');

        // require a value
        let Some(whole) = dotsplit.next() else {
            return Err(ParseTimeSpecError::InvalidComponents);
        };

        // parse the optional fract part (if any) into a float
        if let Some(fract) = dotsplit.next() {
            if !fract.is_empty() {
                // PERF: obviously inefficient ;)
                let tmp = format!("0.{}", fract);
                r.fract = tmp.parse().map_err(ParseTimeSpecError::Fract)?;

                // if there is a fract, the whole is optional
                if whole.is_empty() {
                    return Ok(r);
                }
            }
        }

        // disallow more than one dot
        if !dotsplit.next().is_none() {
            return Err(ParseTimeSpecError::InvalidComponents);
        }

        // split the whole into up to 3 parts, require at least one
        let mut colonsplit = whole.split(':');
        let Some(part1) = colonsplit.next() else {
            return Err(ParseTimeSpecError::InvalidComponents);
        };
        let part2 = colonsplit.next();
        let part3 = colonsplit.next();

        // disallow more than 3 whole parts
        if !colonsplit.next().is_none() {
            return Err(ParseTimeSpecError::InvalidComponents);
        }

        match (part1, part2, part3) {
            (part1, None, None) => {
                r.secs = part1.parse().map_err(ParseTimeSpecError::Secs)?;
            },
            (part1, Some(part2), None) => {
                r.mins = part1.parse().map_err(ParseTimeSpecError::Mins)?;
                r.secs = part2.parse().map_err(ParseTimeSpecError::Secs)?;
            },
            (part1, Some(part2), Some(part3)) => {
                r.hours = part1.parse().map_err(ParseTimeSpecError::Hours)?;
                r.mins = part2.parse().map_err(ParseTimeSpecError::Mins)?;
                r.secs = part3.parse().map_err(ParseTimeSpecError::Secs)?;
            },
            (_, None, Some(_)) => unreachable!(),
        }

        Ok(r)
    }
}

impl fmt::Display for TimeSpec {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        if self.hours > 0 {
            write!(
                f,
                "{}:{:02}:{:02}",
                self.hours, self.mins, self.secs
            )
        } else if self.mins > 0 {
            write!(f, "{}:{:02}", self.mins, self.secs)
        } else {
            write!(f, "{}", self.secs)
        }?;

        if self.fract != 0.0 {
            // PERF: obviously inefficient ;)
            let tmp = format!("{}", self.fract);
            let mut split = tmp.split('.');
            if let Some(substr) = split.nth(1) {
                write!(f, ".{}", substr)?;
            }
        }

        Ok(())
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

    /// Check if a tick number matches
    pub fn check(self, tick: u64) -> bool {
        (tick + self.offset as u64) % self.n as u64 == 0
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
    use super::{TickQuant, TimeSpec};
    #[test]
    fn display_framequant() {
        let x = TickQuant { n: 0, offset: 0 };
        assert_eq!(x.to_string(), "0");
        let x = TickQuant { n: 3, offset: 0 };
        assert_eq!(x.to_string(), "3");
        let x = TickQuant { n: 0, offset: 4 };
        assert_eq!(x.to_string(), "0+4");
        let x = TickQuant { n: 8, offset: 2 };
        assert_eq!(x.to_string(), "8+2");
    }
    #[test]
    fn parse_framequant() {
        let x = "13".parse::<TickQuant>();
        assert_eq!(x, Ok(TickQuant { n: 13, offset: 0 }));
        let x = "  2\n".parse::<TickQuant>();
        assert_eq!(x, Ok(TickQuant { n: 2, offset: 0 }));
        let x = "3+1".parse::<TickQuant>();
        assert_eq!(x, Ok(TickQuant { n: 3, offset: 1 }));
        let x = " 6 + 2  ".parse::<TickQuant>();
        assert_eq!(x, Ok(TickQuant { n: 6, offset: 2 }));
        let x = "garbage".parse::<TickQuant>();
        assert!(x.is_err());
        let x = " 4+garbage".parse::<TickQuant>();
        assert!(x.is_err());
        let x = "garbage + 4".parse::<TickQuant>();
        assert!(x.is_err());
        let x = "gar + bage".parse::<TickQuant>();
        assert!(x.is_err());
    }
    #[test]
    fn display_timespec() {
        let x = TimeSpec {
            hours: 171,
            mins: 3,
            secs: 78,
            fract: 0.25,
        };
        assert_eq!(x.to_string(), "171:03:78.25");
        let x = TimeSpec {
            hours: 3,
            mins: 8,
            secs: 2,
            fract: 0.0,
        };
        assert_eq!(x.to_string(), "3:08:02");
        let x = TimeSpec {
            hours: 17,
            mins: 32,
            secs: 0,
            fract: 0.75,
        };
        assert_eq!(x.to_string(), "17:32:00.75");
        let x = TimeSpec {
            hours: 1,
            mins: 0,
            secs: 0,
            fract: 0.5,
        };
        assert_eq!(x.to_string(), "1:00:00.5");
        let x = TimeSpec {
            hours: 2,
            mins: 0,
            secs: 5,
            fract: 0.0,
        };
        assert_eq!(x.to_string(), "2:00:05");
        let x = TimeSpec {
            hours: 0,
            mins: 0,
            secs: 166,
            fract: 0.125,
        };
        assert_eq!(x.to_string(), "166.125");
        let x = TimeSpec {
            hours: 0,
            mins: 5,
            secs: 0,
            fract: 0.0,
        };
        assert_eq!(x.to_string(), "5:00");
        let x = TimeSpec {
            hours: 0,
            mins: 0,
            secs: 3,
            fract: 0.0,
        };
        assert_eq!(x.to_string(), "3");
    }
    #[test]
    fn parse_timespec() {
        let x = "0".parse::<TimeSpec>().unwrap();
        assert_eq!(
            x,
            TimeSpec {
                hours: 0,
                mins: 0,
                secs: 0,
                fract: 0.0
            }
        );
        let x = "0.0".parse::<TimeSpec>().unwrap();
        assert_eq!(
            x,
            TimeSpec {
                hours: 0,
                mins: 0,
                secs: 0,
                fract: 0.0
            }
        );
        let x = "139".parse::<TimeSpec>().unwrap();
        assert_eq!(
            x,
            TimeSpec {
                hours: 0,
                mins: 0,
                secs: 139,
                fract: 0.0
            }
        );
        let x = "1.125".parse::<TimeSpec>().unwrap();
        assert_eq!(
            x,
            TimeSpec {
                hours: 0,
                mins: 0,
                secs: 1,
                fract: 0.125
            }
        );
        let x = "6:300.75".parse::<TimeSpec>().unwrap();
        assert_eq!(
            x,
            TimeSpec {
                hours: 0,
                mins: 6,
                secs: 300,
                fract: 0.75
            }
        );
        let x = "15:03".parse::<TimeSpec>().unwrap();
        assert_eq!(
            x,
            TimeSpec {
                hours: 0,
                mins: 15,
                secs: 3,
                fract: 0.0
            }
        );
        let x = "16:2".parse::<TimeSpec>().unwrap();
        assert_eq!(
            x,
            TimeSpec {
                hours: 0,
                mins: 16,
                secs: 2,
                fract: 0.0
            }
        );
        let x = "100:200:300".parse::<TimeSpec>().unwrap();
        assert_eq!(
            x,
            TimeSpec {
                hours: 100,
                mins: 200,
                secs: 300,
                fract: 0.0
            }
        );
        let x = "123:0:9.5".parse::<TimeSpec>().unwrap();
        assert_eq!(
            x,
            TimeSpec {
                hours: 123,
                mins: 0,
                secs: 9,
                fract: 0.5
            }
        );
        let x = "01:00:00".parse::<TimeSpec>().unwrap();
        assert_eq!(
            x,
            TimeSpec {
                hours: 1,
                mins: 0,
                secs: 0,
                fract: 0.0
            }
        );
        let x = "1:23:0.75".parse::<TimeSpec>().unwrap();
        assert_eq!(
            x,
            TimeSpec {
                hours: 1,
                mins: 23,
                secs: 0,
                fract: 0.75
            }
        );
        let x = "2:3.".parse::<TimeSpec>().unwrap();
        assert_eq!(
            x,
            TimeSpec {
                hours: 0,
                mins: 2,
                secs: 3,
                fract: 0.0
            }
        );
        let x = ".5".parse::<TimeSpec>().unwrap();
        assert_eq!(
            x,
            TimeSpec {
                hours: 0,
                mins: 0,
                secs: 0,
                fract: 0.5
            }
        );
        let x = "0.75".parse::<TimeSpec>().unwrap();
        assert_eq!(
            x,
            TimeSpec {
                hours: 0,
                mins: 0,
                secs: 0,
                fract: 0.75
            }
        );
        let x = "3.".parse::<TimeSpec>().unwrap();
        assert_eq!(
            x,
            TimeSpec {
                hours: 0,
                mins: 0,
                secs: 3,
                fract: 0.0
            }
        );
        let x = "1:2:3:4".parse::<TimeSpec>();
        assert!(x.is_err());
        let x = ":".parse::<TimeSpec>();
        assert!(x.is_err());
        let x = ".".parse::<TimeSpec>();
        assert!(x.is_err());
        let x = "".parse::<TimeSpec>();
        assert!(x.is_err());
        let x = "abc".parse::<TimeSpec>();
        assert!(x.is_err());
        let x = "0.def".parse::<TimeSpec>();
        assert!(x.is_err());
    }
}
