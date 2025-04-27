use bevy::{prelude::*, reflect::TypePath, asset::Asset, utils::HashMap};
use serde::Deserialize;

#[derive(Asset, Deserialize, TypePath, Debug, Clone)]
pub struct EnemyArchetypeAsset {
    pub health:            u32,
    pub movement_speed:    f32,
    pub vision_range:      f32,
    pub ranged_range:      f32,
    pub projectile_damage: f32,
    pub idle_time_ticks:   u32,
    pub walk_time_min:     u32,
    pub walk_time_max:     u32,
    pub state_init: String,
    #[serde(flatten)]
    pub anim: HashMap<String, String>,
}
