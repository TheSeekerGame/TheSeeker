use bevy::prelude::*;

use crate::game::player::sensors::GroundSensor;
use crate::game::player::skills::types::{ExplosiveMineMetadata, SkillId};
use crate::game::player::spawns::mine::spawn_mine;
use crate::game::player::{Passives, Player, PlayerAction, PlayerStatMod};
use theseeker_engine::physics::GROUNDED_THRESHOLD;

/// Player ability energy for Explosive Mine.
/// Energy is modeled as 3 chunks (0.0..=3.0). Each mine costs 1.0.
#[derive(Component, Debug, Default)]
pub struct ExplosiveMineAbility {
    pub energy: f32,
}

/// Constants for mine energy model
const MINE_CHUNK_REGEN_TICKS: u32 = 300; // 300 ticks per chunk (assumption => 900 ticks full)

pub(crate) const EXPLOSIVE_MINE_METADATA: ExplosiveMineMetadata = ExplosiveMineMetadata {
    max_energy: 3.0,
    chunk_cost: 1.0,
    regen_per_tick: 1.0 / (MINE_CHUNK_REGEN_TICKS as f32),
};

pub const AIR_MINE_MAX_GROUND_DISTANCE: f32 = 36.0;

/// Determine a valid mine spawn position based on the player's transform and
/// ground proximity. Returns `None` when we are too far from the ground.
pub fn resolve_spawn_position(
    transform: &Transform,
    ground: &GroundSensor,
) -> Option<Vec2> {
    let mut spawn_pos = transform.translation.truncate();
    if !ground.is_grounded {
        if ground.distance > AIR_MINE_MAX_GROUND_DISTANCE {
            return None;
        }

        let delta_y = (GROUNDED_THRESHOLD - ground.distance).min(0.0);
        spawn_pos.y += delta_y;
    }
    Some(spawn_pos)
}

/// Try to start Explosive Mine from a toolbar slot.
/// - Instant skill: just spawns a mine at player's transform, no animation on player.
/// - Gated by: just_pressed input, grounded (or close to ground), and energy >= 1 chunk.
pub fn try_start_explosive_mine_slot(
    entity: Entity,
    commands: &mut Commands,
    action_state: &leafwing_input_manager::action_state::ActionState<
        PlayerAction,
    >,
    transform: &Transform,
    _now_tick: u64,
    mine_ability: &ExplosiveMineAbility,
    slot_action: PlayerAction,
    ground: &GroundSensor,
    _passives: &Passives,
) -> bool {
    // Require discrete key-down
    if !action_state.just_pressed(&slot_action) {
        return false;
    }

    // Determine placement position; allow slight air placement if ground is close
    let Some(spawn_pos) = resolve_spawn_position(transform, ground) else {
        return false;
    };

    // Require one full chunk
    if mine_ability.energy < EXPLOSIVE_MINE_METADATA.chunk_cost {
        return false;
    }

    // Spawn mine at player's position; visuals/behavior handled by the spawn systems
    let facing_dir = 0.0; // not used for mines; keep consistent API
    spawn_mine(commands, entity, spawn_pos, facing_dir);

    // Success — energy is actually debited by the per-tick active system to avoid borrow issues here.
    // We'll emit SkillUsed in the caller when we return true.
    true
}

/// Regenerate mine energy while not constrained; tick-based model multiplied by live CDR
pub fn mine_energy_regen(
    mut query: Query<
        (
            Entity,
            &mut ExplosiveMineAbility,
            Option<&PlayerStatMod>,
        ),
        With<Player>,
    >,
    mut energy_deltas: ResMut<
        crate::game::player::passives::frenzied_attack::EnergyRegenDeltas,
    >,
) {
    // Clear previous tick values for mine deltas
    energy_deltas.mine.clear();
    for (entity, mut mine, stat_mod) in query.iter_mut() {
        let cdr = stat_mod.map(|s| s.cdr).unwrap_or(1.0);
        // Tick-based energy — 1/100 chunk per tick scaled by CDR
        let regen_per_tick = EXPLOSIVE_MINE_METADATA.regen_per_tick * cdr;
        let capacity = (EXPLOSIVE_MINE_METADATA.max_energy - mine.energy).max(0.0);
        let applied = regen_per_tick.min(capacity);
        if applied > 0.0 {
            mine.energy += applied;
            energy_deltas.mine.insert(entity, applied);
        }
    }
}

/// Debits 1 chunk when a mine is actually spawned.
/// This runs after try_start_explosive_mine_slot returns true and the spawn has been queued.
pub fn mine_energy_debit_on_spawn(
    mut query: Query<(Entity, &mut ExplosiveMineAbility), With<Player>>,
    mut events: EventReader<crate::game::player::passives::PassiveEvent>,
) {
    use crate::game::player::passives::PassiveEvent;
    for evt in events.read() {
        if let PassiveEvent::SkillUsed { owner, skill } = *evt {
            if skill == SkillId::ExplosiveMine {
                if let Ok((_e, mut mine)) = query.get_mut(owner) {
                    if mine.energy >= EXPLOSIVE_MINE_METADATA.chunk_cost {
                        mine.energy -= EXPLOSIVE_MINE_METADATA.chunk_cost;
                    }
                }
            }
        }
    }
}
