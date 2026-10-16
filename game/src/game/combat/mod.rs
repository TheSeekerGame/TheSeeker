pub mod blood;
pub mod damage_pipeline;
pub mod damage_source;
pub mod effects;
pub mod sparks;

pub use damage_source::{
    DamageSource, DespawnOnOwnerInvalid, DownwardAttack, Hit, Pushback,
    SelfPushback, Stealthed,
};
use crate::prelude::*;

pub const MAX_HEALTH: u32 = 120;

#[derive(Resource, Debug, Default, Deref, DerefMut)]
pub struct KillCount(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlanetImmunityLevel(u8);

impl PlanetImmunityLevel {
    pub const MIN: u8 = 1;
    pub const MAX: u8 = 8;
    const KILLS_PER_LEVEL: u32 = 3;

    pub fn from_kills(kills: u32) -> Self {
        let level_offset = (kills / Self::KILLS_PER_LEVEL)
            .min(u32::from(Self::MAX - 1));
        let capped_offset =
            u8::try_from(level_offset).unwrap_or(Self::MAX - 1);
        Self(Self::MIN + capped_offset)
    }

    pub const fn get(self) -> u8 {
        self.0
    }

    pub const fn atlas_index(self) -> usize {
        (self.0 - Self::MIN) as usize
    }
}

#[derive(Component)]
pub struct Health {
    pub current: u32,
    pub max: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InvulnerabilitySource {
    Defense,
    PopulationProbe,
}

impl InvulnerabilitySource {
    const fn bit(self) -> u8 {
        match self {
            Self::Defense => 1 << 0,
            Self::PopulationProbe => 1 << 1,
        }
    }
}

#[derive(Component, Debug, Clone, Copy)]
#[component(storage = "SparseSet")]
pub struct Invulnerable {
    sources: u8,
}

impl Invulnerable {
    pub const fn from_source(source: InvulnerabilitySource) -> Self {
        Self {
            sources: source.bit(),
        }
    }

    pub fn add_source(&mut self, source: InvulnerabilitySource) {
        self.sources |= source.bit();
    }

    pub fn remove_source(&mut self, source: InvulnerabilitySource) {
        self.sources &= !source.bit();
    }

    pub const fn is_empty(self) -> bool {
        self.sources == 0
    }
}

/// Optional damage gate; when present, damage only applies if `open` is true.
#[derive(Component, Debug)]
pub struct DamageWindow {
    pub open: bool,
}

impl Default for DamageWindow {
    fn default() -> Self {
        Self { open: true }
    }
}

#[derive(Message, Clone, Copy, PartialEq)]
pub struct DamageInfo {
    pub owner: Entity,
    pub source: Entity,
    pub target: Entity,
    pub amount: f32,
    pub crit: bool,
    pub stealthed: bool,
    /// True if this damage instance qualified as a backstab (from behind)
    pub backstab: bool,
}

pub struct CombatPlugin;

impl Plugin for CombatPlugin {
    fn build(&self, app: &mut App) {
        app.add_gametick_event::<DamageInfo>();
        app.init_resource::<KillCount>();
        app.add_plugins((
            blood::BloodFxPlugin,
            damage_pipeline::DamagePipelinePlugin,
            effects::EffectsPlugin,
            sparks::DamageSparksPlugin,
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invulnerability_sources_can_be_removed() {
        let mut invulnerable =
            Invulnerable::from_source(InvulnerabilitySource::Defense);

        invulnerable.remove_source(InvulnerabilitySource::Defense);

        assert!(invulnerable.is_empty());
        invulnerable.add_source(InvulnerabilitySource::PopulationProbe);
        invulnerable.add_source(InvulnerabilitySource::Defense);
        invulnerable.remove_source(InvulnerabilitySource::Defense);
        assert!(!invulnerable.is_empty());
        invulnerable.remove_source(InvulnerabilitySource::PopulationProbe);
        assert!(invulnerable.is_empty());
    }

    #[test]
    fn planet_immunity_advances_every_three_kills_and_caps_at_eight() {
        let checkpoints = [
            (0, 1),
            (2, 1),
            (3, 2),
            (20, 7),
            (21, 8),
            (u32::MAX, 8),
        ];

        for (kills, expected_level) in checkpoints {
            let level = PlanetImmunityLevel::from_kills(kills);
            assert_eq!(level.get(), expected_level);
            assert_eq!(level.atlas_index(), usize::from(expected_level - 1));
        }
    }
}
