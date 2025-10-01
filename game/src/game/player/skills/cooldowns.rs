use bevy::prelude::*;
use std::collections::HashMap;

use super::types::{CooldownMode, CooldownSpec, SkillId};

#[derive(Debug, Clone, Copy)]
pub struct CooldownEntry {
    pub remaining: f32,
    pub initial: f32,
    pub started_at_tick: u64,
    pub min_ticks: u32,
    pub max_ticks: u32,
    pub mode: CooldownMode,
}

#[derive(Resource, Default)]
pub struct Cooldowns {
    // Keyed by (entity, skill)
    map: HashMap<(Entity, SkillId), CooldownEntry>,
    // Natural tick-based reduction for each cooldown during the last tick_cooldowns pass
    pub(crate) tick_deltas: HashMap<(Entity, SkillId), f32>,
}

impl Cooldowns {
    pub fn get(
        &self,
        entity: Entity,
        skill: SkillId,
    ) -> Option<&CooldownEntry> {
        self.map.get(&(entity, skill))
    }
    pub fn is_ready(
        &self,
        entity: Entity,
        skill: SkillId,
        now_tick: u64,
    ) -> bool {
        if let Some(entry) = self.map.get(&(entity, skill)) {
            let elapsed = now_tick.saturating_sub(entry.started_at_tick) as u32;
            if elapsed < entry.min_ticks {
                return false;
            }
            if elapsed >= entry.max_ticks {
                return true;
            }
            entry.remaining <= 0.0
        } else {
            true
        }
    }

    /// Start or restart a cooldown for `skill` on `entity` using the provided spec.
    /// For SnapshotBased mode the duration is divided by `cdr_snapshot` at start.
    pub fn start(
        &mut self,
        entity: Entity,
        skill: SkillId,
        spec: CooldownSpec,
        cdr_snapshot: f32,
        now_tick: u64,
    ) {
        let base = spec.max_ticks as f32;
        let effective = match spec.mode {
            CooldownMode::RateBased => base, // rate-based ignores snapshot for length; ticking uses cdr
            CooldownMode::SnapshotBased => base / cdr_snapshot.max(0.001),
        };
        self.map.insert(
            (entity, skill),
            CooldownEntry {
                remaining: effective,
                initial: effective,
                started_at_tick: now_tick,
                min_ticks: spec.min_ticks,
                max_ticks: spec.max_ticks,
                mode: spec.mode,
            },
        );
    }

    /// Reduce all cooldowns for `entity` by a number of ticks.
    pub fn reduce_all(&mut self, entity: Entity, delta_ticks: f32) {
        for ((e, _), entry) in self.map.iter_mut() {
            if *e == entity {
                entry.remaining = (entry.remaining - delta_ticks).max(0.0);
            }
        }
    }

    /// Set all cooldowns for `entity` to ready.
    pub fn reset_all(&mut self, entity: Entity) {
        for ((e, _), entry) in self.map.iter_mut() {
            if *e == entity {
                entry.remaining = 0.0;
            }
        }
    }

    /// Reduce a specific cooldown by a number of ticks (post-processing helpers)
    pub fn reduce_specific(
        &mut self,
        entity: Entity,
        skill: SkillId,
        delta_ticks: f32,
    ) {
        if let Some(entry) = self.map.get_mut(&(entity, skill)) {
            entry.remaining = (entry.remaining - delta_ticks).max(0.0);
        }
    }

    /// Get the natural tick-based delta recorded for this entity+skill in the last pass
    pub fn get_tick_delta(&self, entity: Entity, skill: SkillId) -> f32 {
        self.tick_deltas
            .get(&(entity, skill))
            .copied()
            .unwrap_or(0.0)
    }
}

/// Tick cooldowns each game tick. RateBased uses live CDR; SnapshotBased ignores CDR after start.
pub fn tick_cooldowns(
    mut cooldowns: ResMut<Cooldowns>,
    query: Query<
        (
            Entity,
            Option<&crate::game::player::PlayerStatMod>,
        ),
        With<crate::game::player::Player>,
    >,
    _time: Res<theseeker_engine::time::GameTime>,
) {
    let dt_ticks = 1.0; // one tick per GameTickUpdate
                        // Clear last tick deltas at the start of processing
    cooldowns.tick_deltas.clear();
    for (entity, stat_mod) in query.iter() {
        let cdr = stat_mod.map(|s| s.cdr).unwrap_or(1.0);
        // Iterate entries we own and record natural delta per-entry
        // Note: collect keys to avoid borrow issues while mutating
        let keys: Vec<(Entity, SkillId)> = cooldowns
            .map
            .keys()
            .filter(|(e, _)| *e == entity)
            .copied()
            .collect();
        for (e, skill) in keys {
            if let Some(entry) = cooldowns.map.get_mut(&(e, skill)) {
                let before = entry.remaining;
                let decrement = match entry.mode {
                    CooldownMode::RateBased => cdr * dt_ticks,
                    CooldownMode::SnapshotBased => dt_ticks,
                };
                entry.remaining = (entry.remaining - decrement).max(0.0);
                let actual = (before - entry.remaining).max(0.0);
                cooldowns.tick_deltas.insert((e, skill), actual);
            }
        }
    }
}
