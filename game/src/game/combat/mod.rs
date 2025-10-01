pub mod damage_pipeline;
pub mod damage_source;
pub mod effects;
pub mod sparks;

pub use damage_source::{
    DamageSource, DownwardAttack, Hit, Pushback, SelfPushback, Stealthed,
};
pub use sparks::SparkSource;

use crate::prelude::*;

pub const MAX_HEALTH: u32 = 120;

#[derive(Resource, Debug, Default, Deref, DerefMut)]
pub struct KillCount(pub u32);

#[derive(Component)]
pub struct Health {
    pub current: u32,
    pub max: u32,
}

#[derive(Event, Clone, Copy, PartialEq)]
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
            damage_pipeline::DamagePipelinePlugin,
            effects::EffectsPlugin,
            sparks::DamageSparksPlugin,
        ));
    }
}
