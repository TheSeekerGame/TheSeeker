use super::types::SkillId;
use super::{
    attack, burning_dash, dash, dash_strike, flicker_strike, spit, whirl,
    consumables,
};
use crate::game::combat::Health;
use crate::game::gentstate::Facing;
use crate::game::player::input_buffer::InputBuffer;
use crate::game::player::sensors::GroundSensor;
use crate::game::player::weapon::{PlayerMeleeWeapon, WeaponInventory};
use crate::game::player::{
    FlickerAbility, PlayerAction, PlayerStatMod, WhirlAbility,
};
use bevy::prelude::*;
use leafwing_input_manager::action_state::ActionState;

pub struct SkillActivationContext<'a> {
    pub entity: Entity,
    pub slot: PlayerAction,
    pub action_state: &'a ActionState<PlayerAction>,
    pub buffer: &'a mut InputBuffer,
    pub melee_weapon: &'a PlayerMeleeWeapon,
    pub whirl_ability: &'a WhirlAbility,
    pub flicker_ability: &'a FlickerAbility,
    pub transform: &'a Transform,
    pub facing: &'a Facing,
    pub last_move_dir: f32,
    pub health: &'a Health,
    pub current_tick: u64,
    pub move_dir: f32,
    pub stats: Option<&'a PlayerStatMod>,
    pub has_burning_dashing: bool,
    pub ground: &'a GroundSensor,
    pub materials: &'a mut crate::game::player::MaterialInventory,
    pub weapons: &'a mut WeaponInventory,
}

/// Mapping between equipped slot index and the input action that triggers it.
#[derive(Clone, Copy)]
pub struct SlotInput {
    pub index: usize,
    pub action: PlayerAction,
}

pub const SLOT_INPUTS: [SlotInput; 5] = [
    SlotInput {
        index: 0,
        action: PlayerAction::Skill1,
    },
    SlotInput {
        index: 1,
        action: PlayerAction::Skill2,
    },
    SlotInput {
        index: 2,
        action: PlayerAction::Skill3,
    },
    SlotInput {
        index: 3,
        action: PlayerAction::Skill4,
    },
    SlotInput {
        index: 4,
        action: PlayerAction::Skill5,
    },
];

pub(super) fn activate_skill(
    skill: SkillId,
    world: &mut super::activation::ActivationWorld,
    ctx: &mut SkillActivationContext<'_>,
) -> bool {
    match skill {
        SkillId::QuarkPylon | SkillId::Stealth | SkillId::AmplifiedBell
        | SkillId::Spinner | SkillId::ExplosiveMine | SkillId::IceNova => activate_instant_slot(skill, world, ctx),
        SkillId::Attack => attack::try_start_melee_attack_slot(ctx, world),
        SkillId::BowAttack => attack::try_start_bow_attack_slot(ctx, world),
        SkillId::Dash => dash::try_start_dash_slot(ctx, world),
        SkillId::DashStrike => dash_strike::try_start_dash_strike_slot(ctx, world),
        SkillId::Whirl => whirl::try_start_whirl_slot(ctx, world),
        SkillId::BurningDash if !ctx.has_burning_dashing => burning_dash::try_start_burning_dash_slot(ctx, world),
        SkillId::FlickerStrike => flicker_strike::try_start_flicker_strike_slot(ctx, world),
        SkillId::Spit => spit::try_start_spit_slot(ctx, world),
        SkillId::RedBerry | SkillId::BlueBerry | SkillId::MossBerry => consumables::try_start_consumable_slot(skill, ctx, world),
        SkillId::BurningDash | SkillId::PortableTent | SkillId::TetherNode => false,
    }
}

fn activate_instant_slot(
    skill: SkillId,
    world: &mut super::activation::ActivationWorld,
    ctx: &mut SkillActivationContext,
) -> bool {
    use super::instant::{ActivationOrigin, InstantResult, InstantUser};
    let slot = ctx.slot;
    let entity = ctx.entity;
    let buffered = matches!(skill, SkillId::Spinner | SkillId::IceNova | SkillId::AmplifiedBell | SkillId::QuarkPylon);
    if !(ctx.action_state.just_pressed(&slot)
        || buffered && ctx.buffer.check_buffered(slot, ctx.current_tick).is_some()) {
        return false;
    }
    let user = InstantUser { entity, transform: ctx.transform, facing: ctx.facing, ground: ctx.ground,
        tick: ctx.current_tick, cdr: ctx.stats.map_or(1.0, |stats| stats.cdr), origin: ActivationOrigin::Input };
    let result = world.instant.activate(skill, &user, Some(ctx.materials));
    let clear_invalid = result == InstantResult::InvalidPlacement
        && matches!(skill, SkillId::AmplifiedBell | SkillId::QuarkPylon);
    if buffered && result == InstantResult::Used || clear_invalid {
        ctx.buffer.clear_action(slot);
    }
    result == InstantResult::Used
}
