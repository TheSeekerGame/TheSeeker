use strum_macros::EnumIter;
use theseeker_engine::audio::transport::TICK_RATE_HZ;

#[derive(Debug, Eq, PartialEq, Hash, EnumIter, Clone, Copy)]
pub enum Passive {
    Bloodstone,
    BloodShards,
    FlamingHeart,
    IceDagger,
    GlowingShard,
    ObsidianNecklace,
    HeavyBoots,
    SerpentRing,
    FrenziedAttack,
    PackKiller,
    DeadlyFeather,
    Sharpshooter,
    ProtectiveSpirit,
    RabbitsFoot,
    CriticalRegeneration,
    VitalityOverclock,
    DeathShroud,
    Prism,
    KineticOrb,
    BatteryPack,
    ShadowCloak,
    PrimeSygil,
    PulseEcho,
    FireCrystal,
    IceCrystal,
    PulseDrive,
    Permutator,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RollDirection {
    HigherBetter,
    LowerBetter,
}

/// How a rolled value reads in its passive's description.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatUnit {
    Flat,
    /// A fraction, read as a percentage.
    Percent,
    /// A factor, read as `x2`.
    Multiplier,
    Seconds,
    /// An amount per tick, read per second.
    PerTick,
    /// A tick interval, read as occurrences per second.
    Interval,
}

impl StatUnit {
    pub fn format(self, value: f32) -> String {
        let hz = TICK_RATE_HZ as f32;
        match self {
            Self::Flat => decimal(value),
            Self::Percent => format!("{:.0}%", value * 100.0),
            Self::Multiplier => format!("x{}", decimal(value)),
            Self::Seconds => format!("{}s", decimal(value)),
            Self::PerTick => decimal(value * hz),
            Self::Interval => decimal(hz / value.max(1.0)),
        }
    }
}

/// At most two decimals, without trailing zeros.
fn decimal(value: f32) -> String {
    format!("{value:.2}").trim_end_matches('0').trim_end_matches('.').to_owned()
}

#[derive(Debug, Clone, Copy)]
pub struct PassiveRollSpec {
    pub unit: StatUnit,
    pub min: f32,
    pub max: f32,
    pub default: f32,
    pub direction: RollDirection,
}

impl PassiveRollSpec {
    pub fn best_value(self) -> f32 {
        match self.direction {
            RollDirection::HigherBetter => self.max,
            RollDirection::LowerBetter => self.min,
        }
    }
}

pub struct PassiveDefinition {
    pub name: &'static str,
    /// The rolled value, if any, replaces `{}`.
    pub description: &'static str,
    pub icon_path: &'static str,
    pub roll: Option<PassiveRollSpec>,
    pub priority: i32,
}

fn roll(unit: StatUnit, min: f32, max: f32, default: f32, direction: RollDirection) -> Option<PassiveRollSpec> {
    Some(PassiveRollSpec { unit, min, max, default, direction })
}

impl Passive {
    pub fn mutually_exclusive_with(&self) -> Option<Passive> {
        match self {
            Self::FireCrystal => Some(Self::IceCrystal),
            Self::IceCrystal => Some(Self::FireCrystal),
            _ => None,
        }
    }

    pub fn name(&self) -> &'static str { self.definition().name }
    pub fn icon_path(&self) -> &'static str { self.definition().icon_path }
    pub fn roll_spec(&self) -> Option<PassiveRollSpec> { self.definition().roll }

    /// The description showing `value`, or the default roll, in its unit.
    pub fn describe(self, value: Option<f32>) -> String {
        let PassiveDefinition { description, roll, .. } = self.definition();
        roll.map_or_else(|| description.to_owned(), |spec| {
            description.replacen("{}", &spec.unit.format(value.unwrap_or(spec.default)), 1)
        })
    }

    #[expect(clippy::too_many_lines, reason = "Each passive's immutable presentation and roll definition is kept together in one catalog.")]
    pub fn definition(self) -> PassiveDefinition {
        use RollDirection::*;
        use StatUnit::*;
        match self {
            Self::Bloodstone => PassiveDefinition {
                name: "Bloodstone",
                description: "Experience orbs heal {}.",
                icon_path: "items/passives/Bloodstone.png",
                roll: roll(Flat, 1.0, 6.0, 2.0, HigherBetter),
                priority: 0,
            },
            Self::BloodShards => PassiveDefinition {
                name: "Blood Shards",
                description: "Overheal looses a homing shard dealing {} the excess.",
                icon_path: "items/passives/BloodShards.png",
                roll: roll(Multiplier, 0.8, 2.2, 1.5, HigherBetter),
                priority: 5,
            },
            Self::FlamingHeart => PassiveDefinition {
                name: "Flaming Heart",
                description: "Below {} health, every 2nd and 3rd hit crits.",
                icon_path: "items/passives/FlamingHeart.png",
                roll: roll(Percent, 0.15, 0.45, 0.25, HigherBetter),
                priority: 10,
            },
            Self::IceDagger => PassiveDefinition {
                name: "Ice Dagger",
                description: "Backstabs deal {} damage. Backstab kills reset cooldowns.",
                icon_path: "items/passives/IceDagger.png",
                roll: roll(Multiplier, 1.5, 3.0, 2.0, HigherBetter),
                priority: 10,
            },
            Self::GlowingShard => PassiveDefinition {
                name: "Glowing Shard",
                description: "+{} defense per nearby enemy.",
                icon_path: "items/passives/GlowingShard.png",
                roll: roll(Percent, 0.4, 1.25, 1.0, HigherBetter),
                priority: 5,
            },
            Self::ObsidianNecklace => PassiveDefinition {
                name: "Obsidian Necklace",
                description: "+11% crit chance. Crits cut {} off every cooldown.",
                icon_path: "items/passives/ObsidianNecklace.png",
                roll: roll(Seconds, 0.2, 1.2, 0.5, HigherBetter),
                priority: 10,
            },
            Self::HeavyBoots => PassiveDefinition {
                name: "Heavy Boots",
                description: "Standing still: x2 damage and defense. Moving: {}.",
                icon_path: "items/passives/HeavyBoots.png",
                roll: roll(Multiplier, 0.35, 0.8, 0.5, HigherBetter),
                priority: 5,
            },
            Self::SerpentRing => PassiveDefinition {
                name: "Serpent Ring",
                description: "x1.2 speed and x1.33 cooldown speed, but max health falls to {}.",
                icon_path: "items/passives/SerpentRing.png",
                roll: roll(Percent, 0.35, 0.70, 0.50, HigherBetter),
                priority: 10,
            },
            Self::FrenziedAttack => PassiveDefinition {
                name: "Frenzied Attack",
                description: "Cooldowns recharge x2 as fast, each bleeding {} health per second.",
                icon_path: "items/passives/FrenziedAttack.png",
                roll: roll(PerTick, 0.04, 0.16, 0.10, LowerBetter),
                priority: 5,
            },
            Self::PackKiller => PassiveDefinition {
                name: "Pack Killer",
                description: "+{} damage per enemy near your target. Lone targets take half.",
                icon_path: "items/passives/PackKiller.png",
                roll: roll(Percent, 0.15, 0.40, 0.30, HigherBetter),
                priority: 5,
            },
            Self::DeadlyFeather => PassiveDefinition {
                name: "Deadly Feather",
                description: "Airborne: +{} cooldown speed, +42% crit chance. Grounded: -50% defense.",
                icon_path: "items/passives/DeadlyFeather.png",
                roll: roll(Percent, 0.15, 0.45, 0.30, HigherBetter),
                priority: 5,
            },
            Self::Sharpshooter => PassiveDefinition {
                name: "Sharpshooter",
                description: "Damage grows with distance, up to {}.",
                icon_path: "items/passives/Sharpshooter.png",
                roll: roll(Multiplier, 2.0, 5.0, 4.0, HigherBetter),
                priority: 5,
            },
            Self::ProtectiveSpirit => PassiveDefinition {
                name: "Protective Spirit",
                description: "No hit takes more than {} of your max health.",
                icon_path: "items/passives/ProtectiveSpirit.png",
                roll: roll(Percent, 0.10, 0.40, 1.0 / 3.0, LowerBetter),
                priority: 20,
            },
            Self::RabbitsFoot => PassiveDefinition {
                name: "Elastic Accelerator",
                description: "+1 jump, +{} speed, +9% crit chance.",
                icon_path: "items/passives/RabbitsFoot.png",
                roll: roll(Percent, 0.10, 0.35, 0.20, HigherBetter),
                priority: 5,
            },
            Self::CriticalRegeneration => PassiveDefinition {
                name: "Critical Regeneration",
                description: "Crits heal {}.",
                icon_path: "items/passives/CriticalRegeneration.png",
                roll: roll(Flat, 8.0, 40.0, 24.0, HigherBetter),
                priority: 10,
            },
            Self::VitalityOverclock => PassiveDefinition {
                name: "Vitality Overclock",
                description: "Up to {} damage near full health, but health drains 4.8 per second.",
                icon_path: "items/passives/VitalityOverclock.png",
                roll: roll(Multiplier, 1.8, 3.8, 3.0, HigherBetter),
                priority: 5,
            },
            Self::DeathShroud => PassiveDefinition {
                name: "Death Shroud",
                description: "Below half health, regenerate {} health per second.",
                icon_path: "items/passives/DeathShroud.png",
                roll: roll(Interval, 4.0, 16.0, 8.0, LowerBetter),
                priority: 0,
            },
            Self::Prism => PassiveDefinition {
                name: "Prism",
                description: "Projectiles split in three, {} degrees apart.",
                icon_path: "items/passives/Prism.png",
                roll: roll(Flat, 2.0, 14.0, 5.0, LowerBetter),
                priority: 5,
            },
            Self::KineticOrb => PassiveDefinition {
                name: "Kinetic Orb",
                description: "Three orbs circle you, dealing {} damage. They quicken as you do.",
                icon_path: "items/passives/KineticOrb.png",
                roll: roll(Flat, 8.0, 20.0, 12.0, HigherBetter),
                priority: 0,
            },
            Self::BatteryPack => PassiveDefinition {
                name: "Battery Pack",
                description: "A fifth skill slot.",
                icon_path: "items/passives/BatteryPack.png",
                roll: None,
                priority: 0,
            },
            Self::ShadowCloak => PassiveDefinition {
                name: "Shadow Cloak",
                description: "Striking no longer breaks stealth, but forfeits its rewards.",
                icon_path: "items/passives/ShadowCloak.png",
                roll: None,
                priority: 0,
            },
            Self::PrimeSygil => PassiveDefinition {
                name: "Prime Sygil",
                description: "x2 damage to enemies above {} health.",
                icon_path: "items/passives/PrimeSygil.png",
                roll: roll(Percent, 0.60, 0.95, 0.80, LowerBetter),
                priority: 5,
            },
            Self::PulseEcho => PassiveDefinition {
                name: "Pulse Echo",
                description: "Melee attacks send out fading pulses dealing {} damage.",
                icon_path: "items/passives/PulseEcho.png",
                roll: roll(Flat, 6.0, 12.0, 9.0, HigherBetter),
                priority: 5,
            },
            Self::FireCrystal => PassiveDefinition {
                name: "Fire Crystal",
                description: "All damage becomes Fire. Excludes Ice Crystal.",
                icon_path: "items/passives/FireCrystal.png",
                roll: None,
                priority: 0,
            },
            Self::IceCrystal => PassiveDefinition {
                name: "Ice Crystal",
                description: "All damage becomes Ice. Excludes Fire Crystal.",
                icon_path: "items/passives/IceCrystal.png",
                roll: None,
                priority: 0,
            },
            Self::PulseDrive => PassiveDefinition {
                name: "Pulse Drive",
                description: "Dash Strike crits have a {} chance to trigger every instant skill.",
                icon_path: "items/passives/PulseDrive.png",
                roll: roll(Percent, 0.70, 1.0, 1.0, HigherBetter),
                priority: 5,
            },
            Self::Permutator => PassiveDefinition {
                name: "Permutator",
                description: "Alternating skills grants {} cooldown speed.",
                icon_path: "items/passives/Permutator.png",
                roll: roll(Multiplier, 1.35, 2.4, 2.0, HigherBetter),
                priority: 5,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use strum::IntoEnumIterator;

    use super::*;

    #[test]
    fn rolled_passives_show_their_value_in_the_description() {
        for passive in Passive::iter() {
            let PassiveDefinition { description, roll, .. } = passive.definition();
            assert_eq!(description.matches("{}").count(), usize::from(roll.is_some()), "{passive:?}");
        }
        assert_eq!(Passive::IceDagger.describe(Some(2.5)), "Backstabs deal x2.5 damage. Backstab kills reset cooldowns.");
        assert_eq!(Passive::DeathShroud.describe(None), "Below half health, regenerate 12 health per second.");
        assert_eq!(Passive::ProtectiveSpirit.describe(None), "No hit takes more than 33% of your max health.");
    }
}
