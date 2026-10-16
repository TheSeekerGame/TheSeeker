use super::*;
use crate::game::combat::{DamageInfo, Health};
use bevy::prelude::*;

#[test]
fn priority_ties_keep_equipment_order_and_rolls_are_fresh() {
    let mut passives = Passives { equipped: vec![Passive::Bloodstone, Passive::IceDagger,
        Passive::ProtectiveSpirit, Passive::CriticalRegeneration], ..default() };
    let kinds = ordered_passives(&passives).map(|p| p.unwrap().kind);
    assert_eq!(kinds, [Passive::ProtectiveSpirit, Passive::IceDagger,
        Passive::CriticalRegeneration, Passive::Bloodstone]);
    passives.set_passive_roll(Passive::Bloodstone, 5.0);
    assert_eq!(ordered_passives(&passives)[3].unwrap().value, 5.0);
}

#[test]
fn actions_use_one_snapshot_before_any_heal_is_applied() {
    let passives = Passives { equipped: vec![Passive::FlamingHeart, Passive::ProtectiveSpirit], ..default() };
    let health = Health { current: 20, max: 120 };
    let context = PassiveContext::with_health(&health);
    let entity = Entity::PLACEHOLDER;
    let damage = PassiveEvent::DamageTaken(DamageInfo { owner: entity, source: entity,
        target: entity, amount: 100.0, crit: false, stealthed: false, backstab: false });
    let hit = PassiveEvent::HitCountAdvanced { owner: entity, hit_count: 6 };
    let mut actions = Vec::new();
    for event in [damage, hit] {
        for passive in ordered_passives(&passives).into_iter().flatten() {
            passive.on_message(&event, &context, &mut actions);
        }
    }
    assert!(matches!(actions.as_slice(), [PassiveAction::Heal(60), PassiveAction::ScheduleNextCrit]));
    assert_eq!(health.current, 20);
}

#[test]
fn vitality_applies_once_and_shroud_uses_tick_boundaries() {
    let health = Health { current: 110, max: 120 };
    let mut context = PassiveContext::with_health(&health);
    let vitality = PassiveEvaluation { kind: Passive::VitalityOverclock, value: 3.0 };
    let mut damage = DamageModifiers::default();
    vitality.modify_damage(&mut damage, &context);
    context.target_position = Some(Vec2::ZERO);
    vitality.modify_damage(&mut damage, &context);
    assert_eq!(damage.damage_multiplier, 3.0);
    let health = Health { current: 59, max: 120 };
    context.health = &health;
    let shroud = PassiveEvaluation { kind: Passive::DeathShroud, value: 8.0 };
    let mut actions = Vec::new();
    shroud.on_tick(&context, 7, &mut actions);
    assert!(actions.is_empty());
    shroud.on_tick(&context, 8, &mut actions);
    assert!(matches!(actions.as_slice(), [PassiveAction::Heal(1)]));
}
