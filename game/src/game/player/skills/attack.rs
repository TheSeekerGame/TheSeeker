use bevy::prelude::*;

use super::cooldowns::Cooldowns;
use super::types::{
    attack_variant_metadata,
    AttackAnimationMetadata,
    AttackVariantMetadata,
    AttackWeaponMetadata,
    ActiveWindowSpec,
    CooldownMode,
    CooldownSpec,
    SkillId,
    SkillWeaponKind,
    Variant,
};

use crate::game::player::input_buffer::{InputBuffer, InputVariant};
use crate::game::player::states::{AttackVariant, Attacking, WeaponType};
use crate::game::player::weapon::{CurrentWeapon, PlayerCombatStyle};
use crate::game::player::PlayerAction;

pub(super) const ATTACK_METADATA: [AttackWeaponMetadata; 3] = [
    AttackWeaponMetadata {
        weapon: SkillWeaponKind::Sword,
        variants: [
            AttackVariantMetadata {
                active_window: ActiveWindowSpec { duration_ticks: 14 },
                state_duration_ticks: 32,
                cooldown: CooldownSpec {
                    min_ticks: 0,
                    max_ticks: 32,
                    mode: CooldownMode::RateBased,
                },
            },
            AttackVariantMetadata {
                active_window: ActiveWindowSpec { duration_ticks: 16 },
                state_duration_ticks: 36,
                cooldown: CooldownSpec {
                    min_ticks: 0,
                    max_ticks: 36,
                    mode: CooldownMode::RateBased,
                },
            },
            AttackVariantMetadata {
                active_window: ActiveWindowSpec { duration_ticks: 18 },
                state_duration_ticks: 40,
                cooldown: CooldownSpec {
                    min_ticks: 0,
                    max_ticks: 40,
                    mode: CooldownMode::RateBased,
                },
            },
        ],
        animations: AttackAnimationMetadata {
            idle: "anim.player.SwordBasicIdle",
            run: "anim.player.SwordBasicRun",
            air: "anim.player.SwordBasicAir",
        },
    },
    AttackWeaponMetadata {
        weapon: SkillWeaponKind::Hammer,
        variants: [
            AttackVariantMetadata {
                active_window: ActiveWindowSpec { duration_ticks: 22 },
                state_duration_ticks: 56,
                cooldown: CooldownSpec {
                    min_ticks: 0,
                    max_ticks: 48,
                    mode: CooldownMode::RateBased,
                },
            },
            AttackVariantMetadata {
                active_window: ActiveWindowSpec { duration_ticks: 26 },
                state_duration_ticks: 56,
                cooldown: CooldownSpec {
                    min_ticks: 0,
                    max_ticks: 54,
                    mode: CooldownMode::RateBased,
                },
            },
            AttackVariantMetadata {
                active_window: ActiveWindowSpec { duration_ticks: 30 },
                state_duration_ticks: 56,
                cooldown: CooldownSpec {
                    min_ticks: 0,
                    max_ticks: 60,
                    mode: CooldownMode::RateBased,
                },
            },
        ],
        animations: AttackAnimationMetadata {
            idle: "anim.player.HammerBasicIdle",
            run: "anim.player.HammerBasicRun",
            air: "anim.player.HammerBasicAir",
        },
    },
    AttackWeaponMetadata {
        weapon: SkillWeaponKind::Bow,
        variants: [
            AttackVariantMetadata {
                active_window: ActiveWindowSpec { duration_ticks: 10 },
                state_duration_ticks: 40,
                cooldown: CooldownSpec {
                    min_ticks: 0,
                    max_ticks: 32,
                    mode: CooldownMode::RateBased,
                },
            },
            AttackVariantMetadata {
                active_window: ActiveWindowSpec { duration_ticks: 12 },
                state_duration_ticks: 40,
                cooldown: CooldownSpec {
                    min_ticks: 0,
                    max_ticks: 36,
                    mode: CooldownMode::RateBased,
                },
            },
            AttackVariantMetadata {
                active_window: ActiveWindowSpec { duration_ticks: 14 },
                state_duration_ticks: 40,
                cooldown: CooldownSpec {
                    min_ticks: 0,
                    max_ticks: 40,
                    mode: CooldownMode::RateBased,
                },
            },
        ],
        animations: AttackAnimationMetadata {
            idle: "anim.player.BowBasicIdle",
            run: "anim.player.BowBasicRun",
            air: "anim.player.BowBasicAir",
        },
    },
];

/// Try to start the Attack skill: choose variant, gate via cooldowns, and enter `Attacking`.
pub fn try_start_attack_slot(
    entity: Entity,
    commands: &mut Commands,
    action_state: &leafwing_input_manager::action_state::ActionState<
        PlayerAction,
    >,
    buffer: &mut InputBuffer,
    weapon: &crate::game::player::weapon::CurrentWeapon,
    now_tick: u64,
    cooldowns: &mut ResMut<Cooldowns>,
    stats: Option<&crate::game::player::PlayerStatMod>,
    slot_action: PlayerAction,
) -> bool {
    // Determine requested variant with live directional override (Down takes precedence over Up).
    let current_dir_variant = if action_state.pressed(&PlayerAction::Fall) {
        Some(Variant::Down)
    } else if action_state.pressed(&PlayerAction::Jump) {
        Some(Variant::Up)
    } else {
        None
    };

    let requested_variant =
        if let Some(buffered) = buffer.check_buffered(slot_action, now_tick) {
            // If the player is currently holding a direction, override the buffered direction
            if let Some(v) = current_dir_variant {
                v
            } else {
                match buffered.variant_modifier {
                    InputVariant::Up => Variant::Up,
                    InputVariant::Down => Variant::Down,
                    InputVariant::Normal => Variant::Forward,
                }
            }
        } else if action_state.just_pressed(&slot_action) {
            if let Some(v) = current_dir_variant {
                v
            } else {
                Variant::Forward
            }
        } else {
            return false;
        };

    // Cooldown gate using dynamic per-weapon base (skills layer owns gating).
    let weapon_kind = weapon_kind_from_current(weapon);
    let timing = attack_variant_metadata(weapon_kind, requested_variant);
    let spec = timing.cooldown;
    if !cooldowns.is_ready(entity, SkillId::Attack, now_tick) {
        return false;
    }
    let cdr_snapshot = stats.map(|s| s.cdr).unwrap_or(1.0);
    cooldowns.start(
        entity,
        SkillId::Attack,
        spec,
        cdr_snapshot,
        now_tick,
    );

    // Translate to action-state variant
    let state_variant = match requested_variant {
        Variant::Forward => AttackVariant::Forward,
        Variant::Up => AttackVariant::Up,
        Variant::Down => AttackVariant::Down,
        _ => AttackVariant::Forward,
    };

    let weapon_type = weapon_type_from_kind(weapon_kind);

    // Behavioral lifetime for `Attacking` derived from local tables
    let state_duration = timing.state_duration_ticks;

    buffer.clear_action(slot_action);
    // Insert `Attacking` with max_ticks from skills; hitbox lifetimes are handled when spawning.
    crate::game::player::states::transition_action(
        commands,
        entity,
        Attacking::new(state_variant, weapon_type)
            .with_max_ticks(state_duration),
    );
    true
}

fn weapon_kind_from_current(weapon: &CurrentWeapon) -> SkillWeaponKind {
    match weapon.combat_style() {
        PlayerCombatStyle::Melee => weapon.melee_weapon().into(),
        PlayerCombatStyle::Ranged => weapon.ranged_weapon().into(),
    }
}

fn weapon_type_from_kind(kind: SkillWeaponKind) -> WeaponType {
    match kind {
        SkillWeaponKind::Sword => WeaponType::Sword,
        SkillWeaponKind::Hammer => WeaponType::Hammer,
        SkillWeaponKind::Bow => WeaponType::Bow,
    }
}
