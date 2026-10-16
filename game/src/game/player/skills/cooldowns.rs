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

impl CooldownEntry {
    /// Advances one tick and returns how much of the cooldown elapsed: rate-based
    /// cooldowns speed up with live cooldown reduction, snapshot cooldowns count one per tick.
    fn tick(&mut self, cdr: f32) -> f32 {
        let before = self.remaining;
        let decrement = match self.mode {
            CooldownMode::RateBased => cdr,
            CooldownMode::SnapshotBased => 1.0,
        };
        self.remaining = (self.remaining - decrement).max(0.0);
        (before - self.remaining).max(0.0)
    }
}

#[derive(Resource, Default)]
pub struct Cooldowns {
    // Keyed by (entity, skill)
    map: HashMap<(Entity, SkillId), CooldownEntry>,
    /// How much each cooldown elapsed in the last tick (Frenzied Attack extends it).
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
        let Some(entry) = self.map.get(&(entity, skill)) else {
            return true;
        };

        let elapsed = now_tick.saturating_sub(entry.started_at_tick) as u32;
        if elapsed < entry.min_ticks {
            return false;
        }
        if elapsed >= entry.max_ticks {
            return true;
        }

        entry.remaining <= 0.0
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
        for (_, entry) in self
            .map
            .iter_mut()
            .filter(|(key, _)| key.0 == entity)
        {
            entry.remaining = (entry.remaining - delta_ticks).max(0.0);
        }
    }

    /// Set all cooldowns for `entity` to ready.
    pub fn reset_all(&mut self, entity: Entity) {
        for (_, entry) in self
            .map
            .iter_mut()
            .filter(|(key, _)| key.0 == entity)
        {
            entry.remaining = 0.0;
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
}

/// Tick cooldowns each game tick. RateBased uses live CDR; SnapshotBased ignores CDR after start.
pub fn tick_cooldowns(
    mut cooldowns: ResMut<Cooldowns>,
    query: Query<
        (Entity, Option<&crate::game::player::PlayerStatMod>),
        With<crate::game::player::Player>,
    >,
) {
    let cooldowns: &mut Cooldowns = &mut cooldowns;
    cooldowns.tick_deltas.clear();
    for (entity, stat_mod) in &query {
        let cdr = stat_mod.map_or(1.0, |stat_mod| stat_mod.cdr);
        for (&key, entry) in cooldowns.map.iter_mut().filter(|((owner, _), _)| *owner == entity) {
            cooldowns.tick_deltas.insert(key, entry.tick(cdr));
        }
    }
}
