use bevy::prelude::*;

pub mod amplified_bell;
pub mod attack;
pub mod burning_dash;
pub mod channeled;
pub mod cooldowns;
pub mod dash;
pub mod dash_strike;
pub mod explosive_mine;
pub mod flicker_strike;
pub mod ice_nova;
pub mod spinner;
pub mod stealth;
pub mod types;
pub mod whirl;
pub mod registry;

/// Optional plugin to initialize skills resources (currently only `Cooldowns`).
pub struct SkillsPlugin;

impl Plugin for SkillsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<cooldowns::Cooldowns>();
    }
}
