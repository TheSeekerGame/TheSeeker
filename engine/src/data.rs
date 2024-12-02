use core::fmt;
use std::num::{ParseFloatError, ParseIntError};
use std::str::FromStr;

use serde::{Deserializer, Serializer};

use crate::prelude::*;

#[derive(Debug, Clone, Copy)]
#[derive(Serialize, Deserialize)]
#[serde(untagged)]
pub enum ColorRepr {
    Lch([f32; 3]),
    Lcha([f32; 4]),
    #[serde(deserialize_with = "deserialize_color_rgbhex")]
    #[serde(serialize_with = "serialize_color_rgbhex")]
    RGBHex(Color),
}

pub fn deserialize_color_rgbhex<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Color, D::Error> {
    let s: String = Deserialize::deserialize(deserializer)?;
    match Color::hex(&s) {
        Ok(color) => Ok(color),
        Err(e) => {
            error!(
                "Color must be specified as RGBA Hex syntax. {:?} is invalid: {}",
                s, e
            );
            Ok(Color::WHITE)
        },
    }
}

pub fn serialize_color_rgbhex<S: Serializer>(
    value: &Color,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    let [r, g, b, a] = value.as_rgba_u8();
    let s = if a != 255 {
        format!("#{:02x}{:02x}{:02x}{:02x}", r, g, b, a)
    } else {
        format!("#{:02x}{:02x}{:02x}", r, g, b)
    };
    serializer.serialize_str(s.as_str())
}

impl From<Color> for ColorRepr {
    fn from(value: Color) -> Self {
        if let Color::Lcha {
            lightness,
            chroma,
            hue,
            alpha,
        } = value
        {
            if alpha != 1.0 {
                ColorRepr::Lcha([lightness, chroma, hue, alpha])
            } else {
                ColorRepr::Lch([lightness, chroma, hue])
            }
        } else {
            ColorRepr::RGBHex(value)
        }
    }
}

impl From<ColorRepr> for Color {
    fn from(value: ColorRepr) -> Self {
        match value {
            ColorRepr::Lcha([l, c, h, a]) => {
                Color::Lcha {
                    lightness: l,
                    chroma: c,
                    hue: h,
                    alpha: a,
                }
            },
            ColorRepr::Lch([l, c, h]) => {
                Color::Lcha {
                    lightness: l,
                    chroma: c,
                    hue: h,
                    alpha: 1.0,
                }
            },
            ColorRepr::RGBHex(color) => color,
        }
    }
}

/// Represent a fractional value, parsed from either a fraction or decimal syntax
#[derive(Debug, Default, Clone, Copy, PartialEq)]
#[derive(SerializeDisplay, DeserializeFromStr)]
pub struct Frac(pub f32);

impl fmt::Display for Frac {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f32::fmt(&self.0, f)
    }
}

impl FromStr for Frac {
    type Err = ParseFloatError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut split = s.split('/');
        if let (Some(num), Some(denum)) = (split.next(), split.next()) {
            let num = num.trim().parse::<f32>()?;
            let denum = denum.trim().parse::<f32>()?;
            Ok(Frac(num / denum))
        } else {
            let float = s.trim().parse::<f32>()?;
            Ok(Frac(float))
        }
    }
}

impl From<Frac> for f32 {
    fn from(value: Frac) -> Self {
        value.0
    }
}
impl From<f32> for Frac {
    fn from(value: f32) -> Self {
        Frac(value)
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
        Duration::from_secs(
            value.secs as u64
                + value.mins as u64 * 60
                + value.hours as u64 * 3600,
        ) + Duration::from_secs_f64(value.fract.abs())
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

/// Special value to indicate that something should happen periodically.
///
/// ("every N ticks", "every N frames", etc.)
///
/// This can be parsed from a string
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[derive(SerializeDisplay, DeserializeFromStr)]
pub struct Quant {
    /// Do the thing every Nth time ...
    pub n: u64,
    /// ... offsetted by this many
    /// (use this to do things "on the off-beat", in musical terms)
    pub offset: i64,
}

impl Quant {
    /// Quantize a value
    ///
    /// Takes the raw value and returns the last value according
    /// to the quantization parameters.
    pub fn apply(self, value: i64) -> i64 {
        let rem = (value - self.offset) % self.n as i64;
        value - rem
    }

    /// Get a value in these units
    pub fn convert(self, value: i64) -> i64 {
        (value - self.offset) / self.n as i64
    }

    /// Check if a value matches
    pub fn check(self, value: i64) -> bool {
        (value - self.offset) % self.n as i64 == 0
    }
}

impl fmt::Display for Quant {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        if self.offset > 0 {
            write!(f, "{}+{}", self.n, self.offset)
        } else if self.offset < 0 {
            write!(f, "{}-{}", self.n, self.offset)
        } else {
            write!(f, "{}", self.n)
        }
    }
}

impl FromStr for Quant {
    type Err = ParseIntError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut r = Quant { n: 0, offset: 0 };

        // look for a + or - sign
        // if there is none, then we expect an integer to use for `n`
        // if there is one, we expect integers on either side to use for `n`+`offset`
        if let Some(i) = s.find(&['+', '-']) {
            let part0 = &s[..i];
            let part1 = &s[i..];
            r.n = part0.trim().parse()?;
            r.offset = part1.trim().parse()?;
        } else {
            r.n = s.trim().parse()?;
        }

        Ok(r)
    }
}

#[derive(Debug, Clone)]
#[derive(Serialize, Deserialize)]
#[serde(untagged)]
pub enum OneOrMany<T> {
    Single(T),
    Many(Vec<T>),
}

#[cfg(test)]
mod test {
    use super::{Quant, TimeSpec};
    #[test]
    fn display_framequant() {
        let x = Quant { n: 0, offset: 0 };
        assert_eq!(x.to_string(), "0");
        let x = Quant { n: 3, offset: 0 };
        assert_eq!(x.to_string(), "3");
        let x = Quant { n: 0, offset: 4 };
        assert_eq!(x.to_string(), "0+4");
        let x = Quant { n: 8, offset: 2 };
        assert_eq!(x.to_string(), "8+2");
    }
    #[test]
    fn parse_framequant() {
        let x = "13".parse::<Quant>();
        assert_eq!(x, Ok(Quant { n: 13, offset: 0 }));
        let x = "  2\n".parse::<Quant>();
        assert_eq!(x, Ok(Quant { n: 2, offset: 0 }));
        let x = "3+1".parse::<Quant>();
        assert_eq!(x, Ok(Quant { n: 3, offset: 1 }));
        let x = " 6 + 2  ".parse::<Quant>();
        assert_eq!(x, Ok(Quant { n: 6, offset: 2 }));
        let x = "garbage".parse::<Quant>();
        assert!(x.is_err());
        let x = " 4+garbage".parse::<Quant>();
        assert!(x.is_err());
        let x = "garbage + 4".parse::<Quant>();
        assert!(x.is_err());
        let x = "gar + bage".parse::<Quant>();
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
