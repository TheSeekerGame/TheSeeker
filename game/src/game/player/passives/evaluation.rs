use super::{DamageModifiers, PassiveAction, PassiveContext, PassiveEvent};
use crate::game::player::{Passive, Passives, PlayerStatMod};
use crate::game::player::skills::types::SkillId;
use rand::Rng;

/// Resolved for one evaluation, so rolled values and contextual inputs remain fresh.
#[derive(Clone, Copy)]
pub struct PassiveEvaluation {
    pub kind: Passive,
    value: f32,
}

pub fn ordered_passives(passives: &Passives) -> [Option<PassiveEvaluation>; Passives::MAX_EQUIPPED] {
    let mut ordered = std::array::from_fn(|index| {
        passives.equipped.get(index).map(|&kind| PassiveEvaluation {
            kind,
            value: passives.passive_effective_value(kind)
                .or_else(|| kind.roll_spec().map(|spec| spec.default)).unwrap_or(0.0),
        })
    });
    // Stable sorting preserves equipment order at equal priority; at most four entries.
    ordered.sort_by_key(|entry| entry.map_or(1, |entry| -entry.priority()));
    ordered
}

impl PassiveEvaluation {
    fn priority(self) -> i32 { self.kind.definition().priority }

    pub fn modify_stats(self, stats: &mut PlayerStatMod, context: &PassiveContext) {
        use Passive::*;
        match self.kind {
            GlowingShard => stats.defense *= 1.0 + context.enemies_nearby as f32 * self.value,
            SerpentRing => {
                stats.speed *= 1.2;
                stats.cdr *= 1.33;
            }
            RabbitsFoot => {
                stats.speed *= 1.0 + self.value;
                stats.extra_jumps = stats.extra_jumps.saturating_add(1);
            }
            HeavyBoots => {
                let multiplier = heavy_boots_multiplier(context, self.value);
                stats.damage *= multiplier;
                stats.defense *= multiplier;
            }
            DeadlyFeather if context.in_air => stats.cdr *= 1.0 + self.value,
            DeadlyFeather => stats.defense *= 0.5,
            Permutator if context.rotation_active => stats.cdr *= self.value,
            _ => {}
        }
    }

    pub fn modify_damage(self, modifiers: &mut DamageModifiers, context: &PassiveContext) {
        use Passive::*;
        match self.kind {
            IceDagger => {
                modifiers.can_backstab = true;
                modifiers.backstab_multiplier = self.value;
            }
            PackKiller if context.target_position.is_some() => {
                modifiers.damage_multiplier *= pack_multiplier(context.enemies_near_target, self.value);
            }
            PrimeSygil if context.target_health_pct.is_some_and(|health| health > self.value) => {
                modifiers.damage_multiplier *= 2.0;
            }
            Sharpshooter => modifiers.damage_multiplier *= distance_multiplier(context, self.value),
            VitalityOverclock if context.target_position.is_none() => {
                modifiers.damage_multiplier *= vitality_multiplier(context, self.value);
            }
            _ => {}
        }
    }

    pub fn animation_slots(self) -> &'static [(&'static str, bool)] {
        match self.kind {
            Passive::SerpentRing => &[("SerpentRing", true)],
            Passive::FrenziedAttack => &[("FrenziedAttack", true)],
            _ => &[],
        }
    }

    pub fn on_tick(self, context: &PassiveContext, tick: u64, actions: &mut Vec<PassiveAction>) {
        if self.kind != Passive::DeathShroud || context.health.max == 0 {
            return;
        }
        let interval = self.value.round().max(1.0) as u64;
        let ratio = context.health.current as f32 / context.health.max as f32;
        if tick.is_multiple_of(interval) && ratio < 0.5 {
            actions.push(PassiveAction::Heal(1));
        }
    }

    pub fn on_message(self, event: &PassiveEvent, context: &PassiveContext, actions: &mut Vec<PassiveAction>) {
        use Passive::*;
        use PassiveAction::*;
        match (self.kind, *event) {
            (Bloodstone, PassiveEvent::XpOrbPickup)
            | (CriticalRegeneration, PassiveEvent::CriticalHit { .. }) => actions.push(Heal(self.value.round() as u32)),
            (ObsidianNecklace, PassiveEvent::CriticalHit { .. }) => {
                actions.extend([ReduceCooldowns(self.value), AddEnergy(self.value)]);
            }
            (IceDagger, PassiveEvent::BackstabKill { .. }) => actions.extend([ResetCooldowns, RefillEnergyFull]),
            (ProtectiveSpirit, PassiveEvent::DamageTaken(info)) => {
                protective_heal(info.amount, context, self.value, actions);
            }
            (PulseDrive, PassiveEvent::CriticalHit { damage_source, source_skill: Some(SkillId::DashStrike) }) => {
                pulse_drive(self.value, damage_source, actions);
            }
            (_, PassiveEvent::HitCountAdvanced { hit_count, .. }) if self.schedules_crit(hit_count, context) => {
                actions.push(ScheduleNextCrit);
            }
            _ => {}
        }
    }

    fn schedules_crit(self, count: u32, context: &PassiveContext) -> bool {
        let divisors: &[u32] = match self.kind {
            Passive::FlamingHeart if (context.health.current as f32) < context.health.max as f32 * self.value => &[2, 3],
            Passive::DeadlyFeather if context.in_air => &[5, 7, 11, 13],
            Passive::ObsidianNecklace => &[23, 29, 31],
            Passive::RabbitsFoot => &[37, 41, 43, 47],
            _ => &[],
        };
        divisors.iter().any(|&divisor| count.is_multiple_of(divisor))
    }
}

fn heavy_boots_multiplier(context: &PassiveContext, penalty: f32) -> f32 {
    let stationary = context.velocity.length() < 0.01 || context.movement_input.abs() < 0.01;
    if stationary && !context.jump_pressed { 2.0 } else { penalty }
}

fn pack_multiplier(enemies: u32, bonus: f32) -> f32 {
    if enemies == 0 { 0.5 } else { 1.0 + enemies as f32 * bonus }
}

fn pulse_drive(chance: f32, source: bevy::prelude::Entity, actions: &mut Vec<PassiveAction>) {
    // Even a guaranteed trigger consumes its random sample.
    if rand::rng().random::<f32>() > chance {
        return;
    }
    actions.push(PassiveAction::TriggerInstantSkills { source_damage: Some(source) });
}

fn protective_heal(amount: f32, context: &PassiveContext, fraction: f32, actions: &mut Vec<PassiveAction>) {
    let max_allowed = context.health.max as f32 * fraction;
    if amount <= max_allowed {
        return;
    }
    let excess = (amount - max_allowed.max(0.0)).round() as u32;
    if excess > 0 {
        actions.push(PassiveAction::Heal(excess));
    }
}

fn distance_multiplier(context: &PassiveContext, maximum: f32) -> f32 {
    let (Some(target), Some(grounded)) = (context.target_position, context.last_grounded_position) else {
        return 1.0;
    };
    let scale = ((maximum - 1.0) / 3.0).max(0.0);
    let remap = |baseline: f32| 1.0 + (baseline - 1.0) * scale;
    match target.distance(grounded) {
        distance if distance >= 250.0 => maximum,
        distance if distance >= 200.0 => remap(3.5),
        distance if distance >= 150.0 => remap(3.0),
        distance if distance >= 120.0 => remap(2.5),
        distance if distance >= 90.0 => remap(2.0),
        distance if distance >= 60.0 => remap(1.5),
        distance if distance >= 30.0 => remap(1.25),
        _ => 1.0,
    }
}

fn vitality_multiplier(context: &PassiveContext, maximum: f32) -> f32 {
    let scale = ((maximum - 1.0) / 2.0).max(0.0);
    let remap = |baseline: f32| 1.0 + (baseline - 1.0) * scale;
    match context.health.current as f32 / context.health.max as f32 {
        ratio if ratio >= 0.9 => maximum,
        ratio if ratio >= 0.7 => remap(2.0),
        ratio if ratio >= 0.5 => remap(1.5),
        ratio if ratio >= 0.25 => remap(1.25),
        _ => 1.0,
    }
}

#[cfg(test)]
#[path = "evaluation_tests.rs"]
mod tests;
