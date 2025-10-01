use super::{
    amplified_bell, attack, burning_dash, dash, dash_strike, explosive_mine,
    flicker_strike, ice_nova, spinner, stealth, whirl,
};
use super::cooldowns::Cooldowns;
use super::types::SkillId;
use crate::game::combat::Health;
use crate::game::enemy::Enemy;
use crate::game::gentstate::Facing;
use crate::game::player::input_buffer::InputBuffer;
use crate::game::player::Passives;
use crate::game::player::sensors::GroundSensor;
use crate::game::player::skills::explosive_mine::ExplosiveMineAbility;
use crate::game::player::spawns::amplified_bell::Bell;
use crate::game::player::weapon::{CurrentWeapon, PlayerMeleeWeapon};
use crate::game::player::{FlickerAbility, PlayerAction, PlayerStatMod, WhirlAbility};
use bevy::prelude::*;
use leafwing_input_manager::action_state::ActionState;
use theseeker_engine::physics::PhysicsWorld;

pub struct SkillActivationContext<'a, 'b> {
    pub action_state: &'a ActionState<PlayerAction>,
    pub buffer: &'a mut InputBuffer,
    pub weapon: &'a CurrentWeapon<'b>,
    pub melee_weapon: &'a PlayerMeleeWeapon,
    pub whirl_ability: &'a WhirlAbility,
    pub flicker_ability: &'a FlickerAbility,
    pub mine_ability: &'a ExplosiveMineAbility,
    pub transform: &'a Transform,
    pub facing: &'a Facing,
    pub health: &'a Health,
    pub current_tick: u64,
    pub stats: Option<&'a PlayerStatMod>,
    pub autoaim_decisive: bool,
    pub has_burning_dashing: bool,
    pub passives: &'a Passives,
    pub ground: &'a GroundSensor,
}

/// Mapping between equipped slot index and the input action that triggers it.
#[derive(Clone, Copy)]
pub struct SlotInput {
    pub index: usize,
    pub action: PlayerAction,
    pub block_on_success: bool,
}

pub const SLOT_INPUTS: [SlotInput; 5] = [
    SlotInput {
        index: 0,
        action: PlayerAction::Skill1,
        block_on_success: true,
    },
    SlotInput {
        index: 1,
        action: PlayerAction::Skill2,
        block_on_success: true,
    },
    SlotInput {
        index: 2,
        action: PlayerAction::Skill3,
        block_on_success: true,
    },
    SlotInput {
        index: 3,
        action: PlayerAction::Skill4,
        block_on_success: false,
    },
    SlotInput {
        index: 4,
        action: PlayerAction::Skill5,
        block_on_success: false,
    },
];

/// Dispatch to the correct skill handler using the shared activation arguments.
pub fn activate_skill<'a, 'b, 'w, 's>(
    skill: SkillId,
    slot_action: PlayerAction,
    entity: Entity,
    commands: &mut Commands,
    cooldowns: &mut ResMut<Cooldowns>,
    enemy_query: &Query<'w, 's, (Entity, &Transform), Or<(With<Enemy>, With<Bell>)>>,
    spatial_query: &PhysicsWorld<'w, 's>,
    ctx: &mut SkillActivationContext<'a, 'b>,
) -> bool {
    match skill {
        SkillId::Attack => attack::try_start_attack_slot(
            entity,
            commands,
            ctx.action_state,
            ctx.buffer,
            ctx.weapon,
            ctx.current_tick,
            cooldowns,
            ctx.stats,
            slot_action,
        ),
        SkillId::Dash => dash::try_start_dash_slot(
            entity,
            commands,
            ctx.action_state,
            ctx.buffer,
            ctx.facing,
            ctx.current_tick,
            cooldowns,
            ctx.stats,
            ctx.autoaim_decisive,
            slot_action,
        ),
        SkillId::DashStrike => dash_strike::try_start_dash_strike_slot(
            entity,
            commands,
            ctx.action_state,
            ctx.buffer,
            ctx.facing,
            ctx.current_tick,
            cooldowns,
            ctx.stats,
            slot_action,
            ctx.ground.is_grounded,
        ),
        SkillId::Whirl => whirl::try_start_whirl_slot(
            entity,
            commands,
            ctx.action_state,
            ctx.buffer,
            ctx.melee_weapon,
            ctx.current_tick,
            ctx.whirl_ability,
            slot_action,
        ),
        SkillId::Stealth => stealth::try_start_stealth_slot(
            entity,
            commands,
            ctx.action_state,
            ctx.current_tick,
            cooldowns,
            ctx.stats,
            slot_action,
        ),
        SkillId::BurningDash => {
            if ctx.has_burning_dashing {
                return false;
            }
            burning_dash::try_start_burning_dash_slot(
                entity,
                commands,
                ctx.action_state,
                ctx.buffer,
                ctx.health,
                ctx.stats,
                slot_action,
                ctx.current_tick,
                cooldowns,
            )
        },
        SkillId::FlickerStrike => flicker_strike::try_start_flicker_strike_slot(
            entity,
            commands,
            ctx.action_state,
            ctx.buffer,
            ctx.melee_weapon,
            ctx.current_tick,
            ctx.flicker_ability,
            slot_action,
            ctx.transform,
            enemy_query,
            ctx.passives,
        ),
        SkillId::AmplifiedBell => amplified_bell::try_start_amplified_bell_slot(
            entity,
            commands,
            ctx.action_state,
            ctx.buffer,
            ctx.transform,
            ctx.facing,
            ctx.current_tick,
            cooldowns,
            ctx.stats,
            spatial_query,
            slot_action,
        ),
        SkillId::Spinner => spinner::try_start_spinner_slot(
            entity,
            commands,
            ctx.action_state,
            ctx.buffer,
            ctx.transform,
            ctx.facing,
            ctx.current_tick,
            cooldowns,
            ctx.stats,
            spatial_query,
            slot_action,
        ),
        SkillId::ExplosiveMine => explosive_mine::try_start_explosive_mine_slot(
            entity,
            commands,
            ctx.action_state,
            ctx.transform,
            ctx.current_tick,
            ctx.mine_ability,
            slot_action,
            ctx.ground,
            ctx.passives,
        ),
        SkillId::IceNova => ice_nova::try_start_ice_nova_slot(
            entity,
            commands,
            ctx.action_state,
            ctx.buffer,
            ctx.transform,
            ctx.current_tick,
            cooldowns,
            ctx.stats,
            slot_action,
        ),
    }
}
