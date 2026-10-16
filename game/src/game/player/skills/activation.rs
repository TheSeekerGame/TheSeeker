//! Input admission is separate from foreground execution and instant effects.

use bevy::{ecs::{query::QueryData, system::SystemParam}, prelude::*};
use leafwing_input_manager::action_state::ActionState;
use theseeker_engine::{time::{GameTickUpdate, GameTime}};
use crate::game::{combat::Health, enemy::Enemy, gentstate::{Dead, Facing}};
use super::{ registry::{activate_skill, SkillActivationContext, SLOT_INPUTS}, types::SkillId};
use super::super::{autoaim::{effective_facing, RangedAttackFacingLock}, foreground::ForegroundAction,
    sensors::GroundSensor, states::{BurningDashing, Elevator, Gathering, HorizontalInputIntent, LastMoveDir},
    weapon::{PlayerMeleeWeapon, WeaponInventory}, spawns::amplified_bell::Bell,
    FlickerAbility, InputBuffer, MaterialInventory, Player, PlayerAction, PlayerStatMod,
    SkillInventory, WhirlAbility};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActivationMode { Instant, Foreground }

#[derive(Default)]
struct ActivationBudget { foreground_started: bool }
impl ActivationBudget {
    fn allows(&self, mode: ActivationMode) -> bool { mode == ActivationMode::Instant || !self.foreground_started }
    fn accept(&mut self, mode: ActivationMode) { self.foreground_started |= mode == ActivationMode::Foreground; }
}

#[derive(QueryData)]
#[query_data(mutable)]
struct SkillUser {
    entity: Entity,
    input: &'static ActionState<PlayerAction>,
    buffer: &'static mut InputBuffer,
    skills: &'static SkillInventory,
    materials: &'static mut MaterialInventory,
    weapons: &'static mut WeaponInventory,
    foreground: &'static ForegroundAction,
    whirl: &'static WhirlAbility,
    flicker: &'static FlickerAbility,
    transform: &'static Transform,
    facing: &'static Facing,
    facing_lock: Option<&'static RangedAttackFacingLock>,
    horizontal: &'static HorizontalInputIntent,
    last_move: Option<&'static LastMoveDir>,
    health: &'static Health,
    ground: &'static GroundSensor,
    stats: Option<&'static PlayerStatMod>,
}

#[derive(SystemParam)]
pub(super) struct ActivationWorld<'w, 's> {
    pub instant: super::instant::InstantWorld<'w, 's>,
    melee_weapon: Res<'w, PlayerMeleeWeapon>,
    time: Res<'w, GameTime>,
    pub enemies: Query<'w, 's, (Entity, &'static Transform), (Or<(With<Enemy>, With<Bell>)>, Without<Dead>)>,
    passive_events: MessageWriter<'w, super::super::passives::PassiveEvent>,
}

pub struct SkillActivationPlugin;
impl Plugin for SkillActivationPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(GameTickUpdate, skill_dispatch_system.in_set(super::super::PlayerBehaviorSet::Activation)
            .after(super::cooldowns::tick_cooldowns));
    }
}

fn skill_dispatch_system(
    mut players: Query<SkillUser, (With<Player>, Without<Dead>,
        Without<crate::game::spikes::SpikeRespawnPending>)>,
    mut world: ActivationWorld,
) {
    for mut player in &mut players {
        if player.health.current == 0 { continue; }
        if player.foreground.state::<Elevator>().is_some() {
            clear_skill_input_buffer(&mut player.buffer);
            continue;
        }
        dispatch_slots(&mut player, &mut world);
    }
}

fn dispatch_slots(player: &mut SkillUserItem<'_, '_>, world: &mut ActivationWorld) {
    let tick = world.time.tick();
    let facing = effective_facing(player.facing, player.facing_lock, tick);
    let melee_weapon = *world.melee_weapon;
    let mut budget = ActivationBudget::default();
    for slot in SLOT_INPUTS {
        let Some(skill) = requested_skill(player, slot.index, slot.action, tick) else { continue; };
        let mode = skill.activation_mode();
        if !budget.allows(mode) { continue; }
        let mut context = SkillActivationContext {
            entity: player.entity, slot: slot.action,
            action_state: player.input, buffer: &mut player.buffer, melee_weapon: &melee_weapon,
            whirl_ability: player.whirl, flicker_ability: player.flicker, transform: player.transform,
            facing: &facing, last_move_dir: player.last_move.map_or(0.0, |direction| direction.0),
            health: player.health, current_tick: tick, move_dir: player.horizontal.movement,
            stats: player.stats, has_burning_dashing: player.foreground.state::<BurningDashing>().is_some(),
            ground: player.ground, materials: &mut player.materials, weapons: &mut player.weapons,
        };
        let accepted = activate_skill(skill, world, &mut context);
        if !accepted { continue; }
        budget.accept(mode);
        world.passive_events.write(super::super::passives::PassiveEvent::SkillUsed { owner: player.entity, skill });
    }
}

fn requested_skill(player: &SkillUserItem<'_, '_>, index: usize, action: PlayerAction, tick: u64) -> Option<SkillId> {
    if !player.input.pressed(&action) && player.buffer.check_buffered(action, tick).is_none() { return None; }
    let skill = player.skills.get_skill_at_slot(index)?;
    // Placement-preview tools commit their requests on release in their own input systems.
    if matches!(skill, SkillId::PortableTent | SkillId::TetherNode) { return None; }
    if player.foreground.state::<Gathering>().is_some() && !skill.is_instant() { return None; }
    Some(skill)
}

fn clear_skill_input_buffer(buffer: &mut InputBuffer) {
    for slot in SLOT_INPUTS { buffer.clear_action(slot.action); }
}

#[cfg(test)]
mod tests;
