use bevy::prelude::*;
use std::collections::HashMap;

use super::PlayerGfx;

// Movement tuning constants – per-tick displacements at 96 Hz
const MAX_MOVE_VEL: f32 = 0.68;
const MOVE_ACCEL: f32 = 0.00217;
const MOVE_ACCEL_INIT: f32 = 0.00217;

#[derive(Clone, Copy, Eq, PartialEq, Hash, Debug)]
pub enum StatType {
    MoveVelMax,
    MoveAccelInit,
    MoveAccel,
}

#[derive(Component, Clone, Debug)]
pub struct StatusModifier {
    pub(super) status_types: Vec<StatType>,
    pub(super) scalar: Vec<f32>,
    pub(super) delta: Vec<f32>,
    pub(super) effect_col: Color,
    pub(super) time_remaining: f32,
}

impl StatusModifier {
    pub fn basic_ice_spider() -> Self {
        Self {
            status_types: vec![
                StatType::MoveVelMax,
                StatType::MoveAccel,
                StatType::MoveAccelInit,
            ],
            scalar: vec![0.5],
            delta: vec![],
            effect_col: Srgba::hex("7aa7ff").unwrap().into(),
            time_remaining: 2.0,
        }
    }
}

#[derive(Component)]
pub struct PlayerStats {
    pub base_stats: HashMap<StatType, f32>,
    pub effective_stats: HashMap<StatType, f32>,
}

impl PlayerStats {
    pub fn init_from_config() -> Self {
        let stats = HashMap::from_iter(vec![
            (StatType::MoveVelMax, MAX_MOVE_VEL),
            (StatType::MoveAccel, MOVE_ACCEL),
            (StatType::MoveAccelInit, MOVE_ACCEL_INIT),
        ]);
        Self {
            base_stats: stats.clone(),
            effective_stats: stats,
        }
    }
    pub fn get(&self, stat: StatType) -> f32 {
        self.effective_stats[&stat]
    }
    pub fn set(&mut self, stat: StatType, value: f32) {
        if let Some(e) = self.effective_stats.get_mut(&stat) {
            *e = value;
        }
    }
    pub fn reset_stat(&mut self, stat: StatType) {
        if let Some(e) = self.effective_stats.get_mut(&stat) {
            *e = self.base_stats[&stat];
        }
    }
    pub fn reset_stats(&mut self) {
        self.effective_stats = self.base_stats.clone();
    }
    pub fn update_stats(&mut self, modifier: &StatusModifier) {
        self.effective_stats.clear();
        // Allow broadcasting a single scalar/delta to all status_types when provided length == 1
        let base_scalar = match modifier.scalar.len() {
            0 => Some(1.0),
            1 => Some(modifier.scalar[0]),
            _ => None,
        };
        let base_delta = match modifier.delta.len() {
            0 => Some(0.0),
            1 => Some(modifier.delta[0]),
            _ => None,
        };
        for (i, stat) in modifier.status_types.iter().enumerate() {
            let val = self.base_stats[stat]
                * base_scalar.unwrap_or_else(|| modifier.scalar[i])
                + base_delta.unwrap_or_else(|| modifier.delta[i]);
            self.effective_stats.insert(*stat, val);
        }
    }
}

pub fn load_player_stats(mut stat_q: Query<&mut PlayerStats>) {
    stat_q.iter_mut().for_each(|mut stats| {
        *stats = PlayerStats::init_from_config();
    });
}

pub fn player_update_stats_mod(
    mut query: Query<(
        Entity,
        &mut StatusModifier,
        &mut PlayerStats,
    )>,
    mut gfx_query: Query<(&PlayerGfx, &mut Sprite)>,
    time: Res<Time<Virtual>>,
    mut commands: Commands,
) {
    for (p_gfx, mut sprite) in gfx_query.iter_mut() {
        let Ok((entity, mut modifier, mut player_stats)) =
            query.get_mut(p_gfx.e_gent)
        else {
            return;
        };
        if modifier.is_changed() {
            player_stats.update_stats(&modifier);
        }
        sprite.color = modifier.effect_col;
        modifier.time_remaining -= time.delta_secs();
        if modifier.time_remaining < 0.0 {
            commands.entity(entity).remove::<StatusModifier>();
            player_stats.reset_stats();
            sprite.color = Color::WHITE;
        }
    }
}

#[derive(Component, Deref, DerefMut)]
pub struct EnemiesNearby(pub u32);

#[derive(Component, Debug)]
pub struct PlayerStatMod {
    pub damage: f32,
    pub defense: f32,
    pub speed: f32,
    pub cdr: f32,
    pub sharpshooter_multiplier: f32,
    pub extra_jumps: u8,
}

impl PlayerStatMod {
    pub fn new() -> PlayerStatMod {
        PlayerStatMod {
            damage: 1.0,
            defense: 1.0,
            speed: 1.0,
            cdr: 1.0,
            sharpshooter_multiplier: 1.0,
            extra_jumps: 0,
        }
    }
}
