use crate::game::player::material_inventory::CraftMaterial;
use crate::game::player::weapon::PlayerMeleeWeapon;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SkillId {
    Attack,
    BowAttack,
    Dash,
    DashStrike,
    Whirl,
    Stealth,
    BurningDash,
    FlickerStrike,
    AmplifiedBell,
    Spinner,
    ExplosiveMine,
    IceNova,
    QuarkPylon,
    Spit,
    RedBerry,
    BlueBerry,
    MossBerry,
    PortableTent,
    TetherNode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CooldownMode {
    /// Tick down using current CDR each tick (affected by mid-cooldown changes)
    RateBased,
    /// Snapshot duration at start; tick down by 1 per tick
    #[expect(
        dead_code,
        reason = "Snapshot cooldowns are supported by the skill data model even when no active skill selects them."
    )]
    SnapshotBased,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CooldownSpec {
    pub min_ticks: u32,
    pub max_ticks: u32,
    pub mode: CooldownMode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DashMetadata {
    pub duration_ticks: u32,
    pub cooldown: CooldownSpec,
    pub animation_key: &'static str,
}

pub(crate) use super::dash::DASH_METADATA;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WhirlMetadata {
    pub max_energy: f32,
    pub min_energy_to_start: f32,
    pub drain_per_second: f32,
    pub regen_per_second: f32,
}

pub fn whirl_metadata() -> &'static WhirlMetadata {
    &super::whirl::WHIRL_METADATA
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BurningDashMetadata {
    pub min_health_to_start: u32,
    pub delayed_cooldown: CooldownSpec,
    pub animation_key: &'static str,
}

pub fn burning_dash_metadata() -> &'static BurningDashMetadata {
    &super::burning_dash::BURNING_DASH_METADATA
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SpinnerMetadata {
    pub cooldown: CooldownSpec,
}

pub fn spinner_metadata() -> &'static SpinnerMetadata {
    &super::spinner::SPINNER_METADATA
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AmplifiedBellMetadata {
    pub cooldown: CooldownSpec,
}

pub fn amplified_bell_metadata() -> &'static AmplifiedBellMetadata {
    &super::amplified_bell::AMPLIFIED_BELL_METADATA
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IceNovaMetadata {
    pub cooldown: CooldownSpec,
}

pub fn ice_nova_metadata() -> &'static IceNovaMetadata {
    &super::ice_nova::ICE_NOVA_METADATA
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QuarkPylonMetadata {
    pub cooldown: CooldownSpec,
    pub lifetime_ticks: u32,
    pub pulse_interval_ticks: u32,
}

pub fn quark_pylon_metadata() -> &'static QuarkPylonMetadata {
    &super::quark_pylon::QUARK_PYLON_METADATA
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StealthMetadata {
    pub cooldown: CooldownSpec,
}

pub fn stealth_metadata() -> &'static StealthMetadata {
    &super::stealth::STEALTH_METADATA
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ExplosiveMineMetadata {
    pub cooldown: CooldownSpec,
}

pub fn explosive_mine_metadata() -> &'static ExplosiveMineMetadata {
    &super::explosive_mine::EXPLOSIVE_MINE_METADATA
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FlickerStrikeMetadata {
    pub max_energy: f32,
    pub chunk_cost: f32,
    pub regen_per_second: f32,
    pub range: f32,
}

pub fn flicker_strike_metadata() -> &'static FlickerStrikeMetadata {
    &super::flicker_strike::FLICKER_STRIKE_METADATA
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ConsumableEffect {
    Regeneration {
        total_heal: u32,
        duration_ticks: u32,
    },
    CooldownRateMultiplier {
        multiplier: f32,
        duration_ticks: u32,
    },
    DamageMultiplier {
        multiplier: f32,
        duration_ticks: u32,
    },
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ConsumableMetadata {
    pub material: CraftMaterial,
    pub effect: ConsumableEffect,
    pub cooldown: CooldownSpec,
}

pub fn consumable_metadata(skill: SkillId) -> Option<&'static ConsumableMetadata> {
    match skill {
        SkillId::RedBerry => Some(&super::consumables::RED_BERRY_METADATA),
        SkillId::BlueBerry => Some(&super::consumables::BLUE_BERRY_METADATA),
        SkillId::MossBerry => Some(&super::consumables::MOSS_BERRY_METADATA),
        _ => None,
    }
}

struct SkillDefinition {
    name: &'static str,
    description: &'static str,
    icon_path: &'static str,
}

impl SkillId {
    fn definition(self) -> SkillDefinition {
        let (name, description, icon_path) = match self {
            Self::Attack => ("Basic Attack",
                "Strike with your melee weapon.",
                "ui/game/AttackSkillIcon.png"),
            Self::BowAttack => ("Bow Attack",
                "Loose an arrow from your bow.",
                "ui/game/BowAttackSkillIcon.png"),
            Self::Dash => ("Dash",
                "Slip past danger in a burst of speed.",
                "ui/game/DashSkillIcon.png"),
            Self::DashStrike => ("Dash Strike",
                "Dive down through foes and burst on landing.",
                "ui/game/DashStrike.png"),
            Self::Whirl => ("Whirling Strike",
                "Spin, striking everything around you.",
                "ui/game/WhirlSkillIcon.png"),
            Self::Stealth => ("Stealth",
                "Vanish from sight. Striking from hiding heals 20% health.",
                "ui/game/StealthSkillIcon.png"),
            Self::BurningDash => ("Burning Dash",
                "Dash aflame through foes, burning health while active.",
                "ui/game/BurningDashSkillIcon.png"),
            Self::FlickerStrike => ("Flicker Strike",
                "Flicker from foe to foe, draining energy while active.",
                "ui/game/FlickerStrikeSkillIcon.png"),
            Self::AmplifiedBell => ("Amplified Bell",
                "Plant a bell. Strike it to send out shocks.",
                "ui/game/AmplifiedBellSkillIcon.png"),
            Self::Spinner => ("Spinner",
                "Launch a drone that darts out and returns.",
                "ui/game/SpinnerSkillIcon.png"),
            Self::ExplosiveMine => ("Explosive Mine",
                "Plant a mine that bursts when foes draw near.",
                "ui/game/MineSkillIcon.png"),
            Self::IceNova => ("Frost Bomb",
                "A blast that freezes nearby foes solid.",
                "ui/game/IceNovaSkillIcon.png"),
            Self::QuarkPylon => ("Quark Pylon",
                "Drop a pylon. A second one completes the pair.",
                "ui/game/PylonSkillIcon.png"),
            Self::Spit => ("Spit",
                "Take spider form and spit venom at the nearest foe.",
                "ui/game/SpitSkillIcon.png"),
            Self::RedBerry => ("Red Berry",
                "Heal 12 over 1s.",
                "ui/game/RedBerrySkillIcon.png"),
            Self::BlueBerry => ("Blue Berry",
                "x1.33 cooldown speed for 2s.",
                "ui/game/BlueBerrySkillIcon.png"),
            Self::MossBerry => ("Moss Berry",
                "x1.5 damage for 2s.",
                "ui/game/MossBerrySkillIcon.png"),
            Self::PortableTent => ("Portable Tent",
                "Hold to aim, release on solid ground to pitch a shelter.",
                "ui/game/TentSkillIcon.png"),
            Self::TetherNode => ("Tether Node",
                "Hold to aim, release on solid ground to extend the network.",
                "ui/game/TetherSkillIcon.png"),
        };
        SkillDefinition { name, description, icon_path }
    }

    /// Skills that can be triggered instantly without interrupting core state machines.
    pub fn activation_mode(self) -> super::activation::ActivationMode {
        if matches!(
            self,
            SkillId::Stealth
                | SkillId::Spinner
                | SkillId::IceNova
                | SkillId::ExplosiveMine
                | SkillId::AmplifiedBell
                | SkillId::QuarkPylon
                | SkillId::RedBerry
                | SkillId::BlueBerry
                | SkillId::MossBerry
                | SkillId::PortableTent
                | SkillId::TetherNode
        ) { super::activation::ActivationMode::Instant } else { super::activation::ActivationMode::Foreground }
    }

    pub fn is_instant(self) -> bool { self.activation_mode() == super::activation::ActivationMode::Instant }

    pub fn is_consumable(self) -> bool {
        matches!(
            self,
            SkillId::RedBerry | SkillId::BlueBerry | SkillId::MossBerry
        )
    }

    pub fn is_stackable(self) -> bool {
        self.stackable_material().is_some()
    }

    pub fn stackable_material(self) -> Option<CraftMaterial> {
        match self {
            SkillId::RedBerry => Some(CraftMaterial::RedBerry),
            SkillId::BlueBerry => Some(CraftMaterial::BlueBerry),
            SkillId::MossBerry => Some(CraftMaterial::MossBerry),
            SkillId::PortableTent => Some(CraftMaterial::Tent),
            SkillId::TetherNode => Some(CraftMaterial::Tether),
            _ => None,
        }
    }

    /// The material this skill spends as stock, as bound in the material table.
    pub fn stock_material(self) -> Option<CraftMaterial> {
        CraftMaterial::SKILL_STOCK_MATERIALS
            .into_iter()
            .find(|material| material.stock_skill() == Some(self))
    }

    /// Human-readable display name for the skill.
    pub fn name(self) -> &'static str { self.definition().name }

    /// Gameplay description for the skill (shown in UI tooltips).
    pub fn description(self) -> &'static str { self.definition().description }

    /// Asset path for the skill's icon image.
    pub fn icon_path(self) -> &'static str { self.definition().icon_path }

    pub fn icon_path_with_melee_weapon(
        self,
        melee_weapon: PlayerMeleeWeapon,
    ) -> &'static str {
        match (self, melee_weapon) {
            (SkillId::Attack, PlayerMeleeWeapon::Hammer) => {
                "ui/game/HammerAttackSkillIcon.png"
            },
            (SkillId::Whirl, PlayerMeleeWeapon::Hammer) => {
                "ui/game/HammerWhirlSkillIcon.png"
            },
            _ => self.icon_path(),
        }
    }
}
