use super::{PassiveContext, PassiveEffect};
use crate::game::player::PlayerStatMod;
use bevy::prelude::*;
use std::collections::HashMap;

// Frenzied Attack design:
// - Convert a fraction of cooldown progress (and inverse-cooldown energy regen) into extra advance each tick
// - Convert that extra advance into HP cost; never kills the player (floored and capped)

pub struct FrenziedAttackEffect;

impl PassiveEffect for FrenziedAttackEffect {
    fn modify_stats(
        &self,
        _stats: &mut PlayerStatMod,
        _context: &PassiveContext,
    ) {
        // No direct CDR modification; FA accelerates via runtime system and HP cost
    }

    fn animation_slots(&self) -> Vec<(&'static str, bool)> {
        vec![("FrenziedAttack", true)]
    }
    fn priority(&self) -> i32 {
        5
    }
    fn name(&self) -> &'static str {
        "FrenziedAttack"
    }
}

// Fraction of progress to convert into HP cost
pub const FA_FRACTION: f32 = 1.0;
// Health cost per cooldown tick-equivalent of extra progress
pub const FA_HP_PER_TICK: f32 = 0.1; // Accumulated and applied with integer health

// Tracks per-entity energy regeneration deltas this tick for inverse-cooldown skills
#[derive(Resource, Default)]
pub struct EnergyRegenDeltas {
    pub whirl: HashMap<Entity, f32>,
    pub flicker: HashMap<Entity, f32>,
    pub mine: HashMap<Entity, f32>,
}

// Accumulates fractional life cost per entity so we can apply whole-health damage without precision loss
#[derive(Resource, Default)]
pub struct LifeDebt {
    pub acc: HashMap<Entity, f32>,
}

/// Runtime system that applies Frenzied Attack effects after cooldowns/energy update.
/// - Reads natural cooldown tick deltas and adds 50% extra progress
/// - Reads energy regen deltas and adds 50% extra regen
/// - Converts the extra progress into health damage without killing the player
pub fn frenzied_attack_runtime(
    mut cooldowns: ResMut<crate::game::player::skills::cooldowns::Cooldowns>,
    mut query: Query<
        (
            Entity,
            &crate::game::player::Passives,
            &mut crate::game::combat::Health,
            Option<&mut crate::game::player::WhirlAbility>,
            Option<&mut crate::game::player::FlickerAbility>,
            Option<&mut crate::game::player::skills::explosive_mine::ExplosiveMineAbility>,
        ),
        With<crate::game::player::Player>,
    >,
    mut energy_deltas: ResMut<EnergyRegenDeltas>,
    mut life_debt: ResMut<LifeDebt>,
    _time: Res<theseeker_engine::time::GameTime>,
) {
    use crate::game::player::skills::types::SkillId;
    use crate::game::player::Passive;

    for (
        entity,
        passives,
        mut health,
        mut maybe_whirl,
        mut maybe_flicker,
        mut maybe_mine,
    ) in query.iter_mut()
    {
        if !passives.contains(&Passive::FrenziedAttack) {
            continue;
        }

        let mut extra_ticks_equivalent: f32 = 0.0;

        // 1) Cooldowns: add 50% of natural tick advancement
        // Collect keys for this entity to avoid borrow conflicts
        let keys: Vec<(Entity, SkillId)> = cooldowns
            .tick_deltas
            .keys()
            .filter(|(e, _)| *e == entity)
            .copied()
            .collect();
        for (e, skill) in keys {
            if let Some(natural_delta) =
                cooldowns.tick_deltas.get(&(e, skill)).copied()
            {
                if natural_delta <= 0.0 {
                    continue;
                }
                let extra = natural_delta * FA_FRACTION;
                cooldowns.reduce_specific(e, skill, extra);
                extra_ticks_equivalent += extra; // natural_delta measured in ticks
            }
        }

        // 2) Inverse cooldowns (energy): add 50% of natural regen, convert to tick-equivalent
        if let Some(regen) = energy_deltas.whirl.get(&entity).copied() {
            if regen > 0.0 {
                let extra = regen * FA_FRACTION;
                if let Some(ref mut whirl) = maybe_whirl {
                    let cap =
                        (crate::game::player::skills::types::whirl_metadata()
                            .max_energy
                            - whirl.energy)
                            .max(0.0);
                    let applied_extra = extra.min(cap);
                    if applied_extra > 0.0 {
                        whirl.energy += applied_extra;
                        // Tick-equivalent: applied_extra / regen == FA_FRACTION of a tick when not truncated
                        extra_ticks_equivalent += applied_extra / regen;
                    }
                }
            }
        }
        if let Some(regen) = energy_deltas.flicker.get(&entity).copied() {
            if regen > 0.0 {
                let extra = regen * FA_FRACTION;
                if let Some(ref mut flicker) = maybe_flicker {
                    let cap = (
                        crate::game::player::skills::types::flicker_strike_metadata()
                            .max_energy
                            - flicker.energy
                    )
                    .max(0.0);
                    let applied_extra = extra.min(cap);
                    if applied_extra > 0.0 {
                        flicker.energy += applied_extra;
                        extra_ticks_equivalent += applied_extra / regen;
                    }
                }
            }
        }
        if let Some(regen) = energy_deltas.mine.get(&entity).copied() {
            if regen > 0.0 {
                let extra = regen * FA_FRACTION;
                if let Some(ref mut mine) = maybe_mine {
                    let cap = (
                        crate::game::player::skills::types::explosive_mine_metadata()
                            .max_energy
                            - mine.energy
                    )
                    .max(0.0);
                    let applied_extra = extra.min(cap);
                    if applied_extra > 0.0 {
                        mine.energy += applied_extra;
                        extra_ticks_equivalent += applied_extra / regen;
                    }
                }
            }
        }

        // 3) Convert extra progress into HP cost, do not kill the player
        if extra_ticks_equivalent > 0.0 && health.current > 1 {
            let hp_cost = extra_ticks_equivalent * FA_HP_PER_TICK;
            let entry = life_debt.acc.entry(entity).or_insert(0.0);
            *entry += hp_cost;
            if *entry >= 1.0 {
                let whole = entry.floor() as u32;
                if whole > 0 {
                    let max_damage = health.current.saturating_sub(1);
                    let damage = whole.min(max_damage);
                    if damage > 0 {
                        health.current =
                            health.current.saturating_sub(damage).max(1);
                        *entry -= damage as f32;
                    }
                }
            }
        }
    }

    // Clear energy deltas after consumption to avoid stale accumulation next tick
    energy_deltas.whirl.clear();
    energy_deltas.flicker.clear();
    energy_deltas.mine.clear();
}
