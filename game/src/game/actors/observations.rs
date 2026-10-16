use bevy::prelude::*;

#[derive(Component, Default)]
#[component(storage = "SparseSet")]
pub struct AiPaused;

#[derive(Component, Default)]
pub struct TargetSensor {
    pub entity: Option<Entity>,
    pub dist2: f32,
}

#[derive(Component, Default)]
pub struct GroundSensor {
    pub on: bool,
}

#[derive(Component, Default)]
pub struct RangeSensor {
    pub in_melee: bool,
    pub in_aggro: bool,
}

#[derive(Component)]
#[derive(Default)]
pub struct TurnCooldown {
    pub timer: u16,
}

#[derive(Component, Default)]
pub struct HealthSensor {
    pub zero: bool,
}

#[derive(Component)]
pub struct SensorHistory {
    pub target_history: [(Option<Entity>, f32); 32],
    pub ground_history: [bool; 32],
    pub range_history: [(bool, bool); 32], // (in_melee, in_aggro)

    pub write_index: u8,
    pub reaction_time: u8,
}

impl SensorHistory {
    pub fn new(reaction_time: u8) -> Self {
        Self {
            target_history: [(None, f32::MAX); 32],
            ground_history: [false; 32],
            range_history: [(false, false); 32],
            write_index: 0,
            reaction_time: reaction_time.min(32),
        }
    }

    pub fn get_read_index(&self) -> usize {
        let delay = self.reaction_time.min(32) as usize;
        (self.write_index as usize + 32 - delay) % 32
    }
}

#[derive(Component, Default)]
pub struct PerceivedTargetSensor {
    pub entity: Option<Entity>,
    pub dist2: f32,
}

#[derive(Component, Default)]
pub struct PerceivedGroundSensor {
    pub on: bool,
}

#[derive(Component, Default)]
pub struct PerceivedRangeSensor {
    pub in_melee: bool,
    pub in_aggro: bool,
}



#[derive(Component, Default, Clone, Copy)]
pub struct DistanceJitter {
    pub melee: f32,
    pub vision: f32,
}

#[derive(Component)]
pub struct PatrolTimeScale(pub f32);
