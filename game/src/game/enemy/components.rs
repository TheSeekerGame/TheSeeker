use bevy::prelude::*;

#[derive(Component, Debug, Clone)]
pub struct Movement { pub walk_speed: f32 }

#[derive(Component, Debug, Clone)]
pub struct Patrol { pub idle: u32, pub walk_min: u32, pub walk_max: u32 }

#[derive(Component, Debug, Clone)]
pub struct RangedAttack { pub damage: f32, pub range: f32 }

#[derive(Component, Debug, Clone, Default)]
pub struct TargetSensor { pub vision: f32, pub target: Option<Entity> }

#[derive(Component, Debug, Clone, Default)]
pub struct GroundSensor { pub on_ground: bool }

#[derive(Component, Debug, Clone, Default)]
pub struct RangeSensor { pub in_ranged: bool }
