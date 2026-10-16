use crate::game::combat::Health;
use crate::game::player::skills::cooldowns::Cooldowns;
use crate::game::player::skills::types::{
    flicker_strike_metadata, whirl_metadata, SkillId,
};
use crate::game::player::{
    FlickerAbility, Passive, Passives, Player, WhirlAbility,
};
use bevy::prelude::*;
use std::collections::HashMap;

// Frenzied Attack design:
// - Convert a fraction of cooldown progress (and inverse-cooldown energy regen) into extra advance each tick
// - Convert that extra advance into HP cost; never kills the player (floored and capped)

// Fraction of progress to convert into HP cost
pub const FA_FRACTION: f32 = 1.0;
// Default health cost per cooldown tick-equivalent of extra progress
pub const FA_HP_PER_TICK_DEFAULT: f32 = 0.1;

// Tracks per-entity energy regeneration deltas this tick for inverse-cooldown skills
#[derive(Resource, Default)]
pub struct EnergyRegenDeltas {
    pub whirl: HashMap<Entity, f32>,
    pub flicker: HashMap<Entity, f32>,
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
    mut cooldowns: ResMut<Cooldowns>,
    mut query: Query<
        (
            Entity,
            &Passives,
            &mut Health,
            Option<&mut WhirlAbility>,
            Option<&mut FlickerAbility>,
        ),
        With<Player>,
    >,
    mut energy_deltas: ResMut<EnergyRegenDeltas>,
    mut life_debt: ResMut<LifeDebt>,
    _time: Res<theseeker_engine::time::GameTime>,
) {
    for (
        entity,
        passives,
        mut health,
        mut maybe_whirl,
        mut maybe_flicker,
    ) in query.iter_mut()
    {
        if !passives.contains(&Passive::FrenziedAttack) {
            continue;
        }

        let extra_ticks_equivalent =
            apply_frenzied_cooldown_extra(entity, &mut cooldowns)
                + apply_frenzied_whirl_extra(
                    entity,
                    &energy_deltas,
                    maybe_whirl.as_deref_mut(),
                )
                + apply_frenzied_flicker_extra(
                    entity,
                    &energy_deltas,
                    maybe_flicker.as_deref_mut(),
                );
        apply_frenzied_life_debt(
            entity,
            passives,
            &mut health,
            &mut life_debt,
            extra_ticks_equivalent,
        );
    }

    // Clear energy deltas after consumption to avoid stale accumulation next tick
    energy_deltas.whirl.clear();
    energy_deltas.flicker.clear();
}

fn apply_frenzied_cooldown_extra(
    entity: Entity,
    cooldowns: &mut Cooldowns,
) -> f32 {
    let keys: Vec<(Entity, SkillId)> = cooldowns
        .tick_deltas
        .keys()
        .filter(|(e, _)| *e == entity)
        .copied()
        .collect();

    keys.into_iter()
        .map(|(e, skill)| apply_frenzied_cooldown_key(e, skill, cooldowns))
        .sum()
}

fn apply_frenzied_cooldown_key(
    entity: Entity,
    skill: SkillId,
    cooldowns: &mut Cooldowns,
) -> f32 {
    let Some(natural_delta) = cooldowns.tick_deltas.get(&(entity, skill)).copied()
    else {
        return 0.0;
    };
    if natural_delta <= 0.0 {
        return 0.0;
    }

    let extra = natural_delta * FA_FRACTION;
    cooldowns.reduce_specific(entity, skill, extra);
    extra
}

fn apply_frenzied_whirl_extra(
    entity: Entity,
    energy_deltas: &EnergyRegenDeltas,
    maybe_whirl: Option<&mut WhirlAbility>,
) -> f32 {
    let Some(whirl) = maybe_whirl else {
        return 0.0;
    };
    let Some(regen) = energy_deltas.whirl.get(&entity).copied() else {
        return 0.0;
    };

    apply_frenzied_energy_extra(regen, whirl_metadata().max_energy, &mut whirl.energy)
}

fn apply_frenzied_flicker_extra(
    entity: Entity,
    energy_deltas: &EnergyRegenDeltas,
    maybe_flicker: Option<&mut FlickerAbility>,
) -> f32 {
    let Some(flicker) = maybe_flicker else {
        return 0.0;
    };
    let Some(regen) = energy_deltas.flicker.get(&entity).copied() else {
        return 0.0;
    };

    apply_frenzied_energy_extra(
        regen,
        flicker_strike_metadata().max_energy,
        &mut flicker.energy,
    )
}

fn apply_frenzied_energy_extra(
    regen: f32,
    max_energy: f32,
    energy: &mut f32,
) -> f32 {
    if regen <= 0.0 {
        return 0.0;
    }

    let extra = regen * FA_FRACTION;
    let cap = (max_energy - *energy).max(0.0);
    let applied_extra = extra.min(cap);
    if applied_extra <= 0.0 {
        return 0.0;
    }

    *energy += applied_extra;
    // Tick-equivalent: applied_extra / regen == FA_FRACTION of a tick when not truncated.
    applied_extra / regen
}

fn apply_frenzied_life_debt(
    entity: Entity,
    passives: &Passives,
    health: &mut Health,
    life_debt: &mut LifeDebt,
    extra_ticks_equivalent: f32,
) {
    if extra_ticks_equivalent <= 0.0 || health.current <= 1 {
        return;
    }

    let hp_per_tick = passives
        .passive_effective_value(Passive::FrenziedAttack)
        .unwrap_or(FA_HP_PER_TICK_DEFAULT);
    let entry = life_debt.acc.entry(entity).or_insert(0.0);
    *entry += extra_ticks_equivalent * hp_per_tick;

    let damage = pending_frenzied_life_debt_damage(*entry, health.current);
    if damage == 0 {
        return;
    }

    health.current = health.current.saturating_sub(damage).max(1);
    *entry -= damage as f32;
}

fn pending_frenzied_life_debt_damage(entry: f32, current_health: u32) -> u32 {
    if entry < 1.0 {
        return 0;
    }

    let whole = entry.floor() as u32;
    let max_damage = current_health.saturating_sub(1);
    whole.min(max_damage)
}
