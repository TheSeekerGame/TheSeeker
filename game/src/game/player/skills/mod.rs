pub(super) mod placement_preview;
pub(crate) mod instant;
use bevy::prelude::*;

pub mod activation;
pub mod amplified_bell;
pub mod attack;
pub mod burning_dash;
pub mod channeled;
pub mod consumables;
pub mod cooldowns;
pub mod dash;
pub mod dash_strike;
pub mod explosive_mine;
pub mod flicker_strike;
pub mod ice_nova;
pub mod item_use;
pub mod portable_tent;
pub mod quark_pylon;
pub mod registry;
pub mod spinner;
pub mod spit;
pub mod stealth;
pub mod tether_node;
pub mod types;
pub mod whirl;

/// Skill admission, cooldown resources, and instant or foreground item use.
pub struct SkillsPlugin;

impl Plugin for SkillsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<cooldowns::Cooldowns>();
        app.add_plugins((activation::SkillActivationPlugin, item_use::ItemUsePlugin));
        app.init_resource::<tether_node::TetherConnectionStatus>();
    }
}
