
use super::types::{CooldownMode, CooldownSpec, SkillId};

use crate::game::player::input_buffer::InputVariant;
use crate::game::player::states::{AttackVariant, Attacking};
use crate::game::player::weapon::WeaponType;
use crate::game::player::PlayerAction;

pub(super) fn try_start_melee_attack_slot(
    ctx: &mut super::registry::SkillActivationContext,
    world: &mut super::activation::ActivationWorld,
) -> bool {
    try_start_attack_slot(SkillId::Attack, (*ctx.melee_weapon).into(), ctx, world)
}

pub(super) fn try_start_bow_attack_slot(
    ctx: &mut super::registry::SkillActivationContext,
    world: &mut super::activation::ActivationWorld,
) -> bool {
    if !ctx.weapons.can_fire_bow() { return false; }
    let started = try_start_attack_slot(SkillId::BowAttack, WeaponType::Bow, ctx, world);
    if started { ctx.weapons.consume_bow_ammo(); }
    started
}

pub(super) fn try_start_attack_slot(
    skill_id: SkillId,
    weapon: WeaponType,
    ctx: &mut super::registry::SkillActivationContext,
    world: &mut super::activation::ActivationWorld,
) -> bool {
    let commands = &mut world.instant.commands;
    let cooldowns = &mut world.instant.cooldowns;
    // A direction held now overrides the buffered one (Down takes precedence over Up).
    let held_variant = current_attack_direction_variant(ctx.action_state);

    let variant =
        if let Some(buffered) = ctx.buffer.check_buffered(ctx.slot, ctx.current_tick) {
            held_variant.unwrap_or_else(|| {
                buffered_attack_direction_variant(
                    buffered.variant_modifier,
                    ctx.action_state,
                )
            })
        } else if ctx.action_state.just_pressed(&ctx.slot) {
            held_variant.unwrap_or(AttackVariant::Forward)
        } else {
            return false;
        };

    if !cooldowns.is_ready(ctx.entity, skill_id, ctx.current_tick) {
        return false;
    }
    let cooldown = CooldownSpec {
        min_ticks: 0,
        max_ticks: weapon.attack().swing(variant).cooldown_ticks,
        mode: CooldownMode::RateBased,
    };
    let cdr_snapshot = ctx.stats.map(|s| s.cdr).unwrap_or(1.0);
    cooldowns.start(
        ctx.entity,
        skill_id,
        cooldown,
        cdr_snapshot,
        ctx.current_tick,
    );

    ctx.buffer.clear_action(ctx.slot);
    crate::game::player::states::transition_action(
        commands,
        ctx.entity,
        Attacking::new(variant, weapon),
    );
    true
}

fn current_attack_direction_variant(
    action_state: &leafwing_input_manager::action_state::ActionState<
        PlayerAction,
    >,
) -> Option<AttackVariant> {
    if action_state.pressed(&PlayerAction::Fall) {
        return Some(AttackVariant::Down);
    }
    if action_state.pressed(&PlayerAction::UpModifier) {
        return Some(AttackVariant::Up);
    }

    None
}

fn buffered_attack_direction_variant(
    variant: InputVariant,
    action_state: &leafwing_input_manager::action_state::ActionState<
        PlayerAction,
    >,
) -> AttackVariant {
    match variant {
        InputVariant::Down => AttackVariant::Down,
        InputVariant::Up if action_state.pressed(&PlayerAction::UpModifier) => {
            AttackVariant::Up
        },
        InputVariant::Normal | InputVariant::Up => AttackVariant::Forward,
    }
}
