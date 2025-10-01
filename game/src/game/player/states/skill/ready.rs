use bevy::prelude::*;
use leafwing_input_manager::prelude::*;
use theseeker_engine::time::{GameTickUpdate, GameTime};

use crate::game::combat::Health;
use crate::game::gentstate::Facing;
use crate::game::player::sensors::GroundSensor;
use crate::game::player::skills::registry::{
    activate_skill, SkillActivationContext, SLOT_INPUTS,
};
use crate::game::player::states::BurningDashing;
use crate::game::player::weapon::{CurrentWeapon, PlayerMeleeWeapon};
use crate::game::player::BowAutoAimState;
use crate::game::player::{
    FlickerAbility, InputBuffer, Player, PlayerAction, PlayerStateSet,
    SkillInventory, WhirlAbility,
};

pub struct ReadyStatePlugin;

impl Plugin for ReadyStatePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            GameTickUpdate,
            skill_dispatch_system
                .chain()
                // Runs regardless of current action state so skills can interrupt each other
                .in_set(PlayerStateSet::Behavior),
        );
    }
}

fn skill_dispatch_system(
    mut commands: Commands,
    mut query: Query<
        (
            Entity,
            &ActionState<PlayerAction>,
            &WhirlAbility,
            &FlickerAbility,
            &crate::game::player::skills::explosive_mine::ExplosiveMineAbility,
            &Transform,
            &Facing,
            &mut InputBuffer,
            &SkillInventory,
            &Health,
            Has<BurningDashing>,
            &crate::game::player::Passives,
            &GroundSensor,
        ),
        With<Player>,
    >,
    enemy_query: Query<
        (Entity, &Transform),
        Or<(
            With<crate::game::enemy::Enemy>,
            With<crate::game::player::spawns::amplified_bell::Bell>,
        )>,
    >,
    weapon: CurrentWeapon,
    melee_weapon: Res<PlayerMeleeWeapon>,
    time: Res<GameTime>,
    autoaim: Res<BowAutoAimState>,
    mut cooldowns: ResMut<crate::game::player::skills::cooldowns::Cooldowns>,
    stats_query: Query<
        Option<&crate::game::player::PlayerStatMod>,
        With<Player>,
    >,
    spatial_query: theseeker_engine::physics::PhysicsWorld,
    mut passive_events: EventWriter<
        crate::game::player::passives::PassiveEvent,
    >,
) {
    'player: for (
        entity,
        action_state,
        whirl_ability,
        flicker_ability,
        mine_ability,
        transform,
        facing,
        mut buffer,
        skill_inventory,
        health,
        has_burning_dashing,
        passives,
        ground,
    ) in query.iter_mut()
    {
        let current_tick = time.tick() as u64;
        let stats = stats_query.get(entity).ok().flatten();

        for slot in SLOT_INPUTS.iter() {
            if !action_state.pressed(&slot.action) {
                continue;
            }

            if let Some(skill) = skill_inventory.get_skill_at_slot(slot.index) {
                let mut ctx = SkillActivationContext {
                    action_state,
                    buffer: &mut buffer,
                    weapon: &weapon,
                    melee_weapon: &melee_weapon,
                    whirl_ability,
                    flicker_ability,
                    mine_ability,
                    transform,
                    facing,
                    health,
                    current_tick,
                    stats,
                    autoaim_decisive: autoaim.decisive,
                    has_burning_dashing,
                    passives,
                    ground,
                };
                let started = activate_skill(
                    skill,
                    slot.action,
                    entity,
                    &mut commands,
                    &mut cooldowns,
                    &enemy_query,
                    &spatial_query,
                    &mut ctx,
                );
                if started {
                    passive_events.write(
                        crate::game::player::passives::PassiveEvent::SkillUsed {
                            owner: entity,
                            skill,
                        },
                    );

                    // Some slots are exclusive: once one skill starts, skip evaluating others this tick
                    if slot.block_on_success {
                        continue 'player;
                    }
                }
            }
        }
    }
}
