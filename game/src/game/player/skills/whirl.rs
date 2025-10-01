use bevy::prelude::*;

use crate::game::player::input_buffer::InputBuffer;
use crate::game::player::passives::frenzied_attack::EnergyRegenDeltas;
use crate::game::player::states::{transition_action, Ready, WeaponType, Whirling};
use crate::game::player::weapon::PlayerMeleeWeapon;
use crate::game::player::{Player, PlayerAction, PlayerStatMod, WhirlAbility};
use theseeker_engine::time::GameTime;

use super::types::WhirlMetadata;

pub(crate) const WHIRL_METADATA: WhirlMetadata = WhirlMetadata {
    max_energy: 2.0,
    min_energy_to_start: 0.5,
    drain_per_second: 1.0,
    regen_per_second: 0.2,
    animations: [
        "anim.player.SwordWhirling",
        "anim.player.HammerWhirling",
        "anim.player.SwordWhirling",
    ],
};

/// Try to start Whirl: gate via energy and enter `Whirling` with the current melee weapon.
pub fn try_start_whirl_slot(
    entity: Entity,
    commands: &mut Commands,
    action_state: &leafwing_input_manager::action_state::ActionState<
        PlayerAction,
    >,
    buffer: &mut InputBuffer,
    melee_weapon: &PlayerMeleeWeapon,
    now_tick: u64,
    whirl_ability: &WhirlAbility,
    slot_action: PlayerAction,
) -> bool {
    let _ = now_tick; // kept for symmetry with other skill APIs
                      // Start condition: just pressed or buffered
    let requested = action_state.just_pressed(&slot_action)
        || buffer.check_buffered(slot_action, now_tick).is_some();
    if !requested {
        return false;
    }

    // Energy gate
    if whirl_ability.energy < WHIRL_METADATA.min_energy_to_start {
        return false;
    }

    // Always use the melee weapon for whirling, regardless of current combat style
    let weapon_type = match *melee_weapon {
        PlayerMeleeWeapon::Sword => WeaponType::Sword,
        PlayerMeleeWeapon::Hammer => WeaponType::Hammer,
    };

    buffer.clear_action(slot_action);
    // Store which slot this whirl is on so we can check the right button during sustain
    let mut whirling = Whirling::new(weapon_type);
    whirling.slot_action = Some(slot_action);
    transition_action(commands, entity, whirling);
    true
}

/// Regenerate whirl energy while not whirling (scaled by CDR).
pub fn whirl_energy_regen(
    mut query: Query<
        (
            Entity,
            &mut WhirlAbility,
            Option<&PlayerStatMod>,
        ),
        (With<Player>, Without<Whirling>),
    >,
    mut energy_deltas: ResMut<EnergyRegenDeltas>,
    time: Res<GameTime>,
) {
    // Clear previous tick values once per tick
    energy_deltas.whirl.clear();
    for (entity, mut whirl_ability, stat_mod) in query.iter_mut() {
        let cdr = stat_mod.map(|s| s.cdr).unwrap_or(1.0);
        let regen_per_tick =
            WHIRL_METADATA.regen_per_second * cdr / time.hz as f32;
        let capacity =
            (WHIRL_METADATA.max_energy - whirl_ability.energy).max(0.0);
        let applied = regen_per_tick.min(capacity);
        if applied > 0.0 {
            whirl_ability.energy += applied;
            energy_deltas.whirl.insert(entity, applied);
        }
    }
}

/// While whirling, drain energy and end when released past minimum or on energy exhaustion.
pub fn whirl_active_tick(
    mut commands: Commands,
    mut query: Query<
        (
            Entity,
            &mut Whirling,
            &mut WhirlAbility,
            &leafwing_input_manager::action_state::ActionState<PlayerAction>,
            Option<&PlayerStatMod>,
        ),
        (With<Player>, With<Whirling>),
    >,
    time: Res<GameTime>,
) {
    const MIN_TICKS: u32 = 48; // ~0.5s at 96 Hz
    for (entity, mut whirling, mut whirl_ability, input, stat_mod) in
        query.iter_mut()
    {
        // Compute drain per tick (reverse-cooldown model)
        let cdr = stat_mod.map(|s| s.cdr).unwrap_or(1.0).max(0.001);
        let energy_cost_per_tick =
            (WHIRL_METADATA.drain_per_second / time.hz as f32) / cdr;

        // Continue if held or not yet past minimum duration and we still have energy
        // Check the slot action that was used to start this whirl
        let button_held = if let Some(slot_action) = whirling.slot_action {
            input.pressed(&slot_action)
        } else {
            // Fallback for backward compatibility
            false
        };
        let past_minimum = whirling.tick >= MIN_TICKS;
        let has_energy = whirl_ability.energy > 0.0;

        if (button_held || !past_minimum) && has_energy {
            whirl_ability.energy =
                (whirl_ability.energy - energy_cost_per_tick).max(0.0);
            continue;
        }

        // End whirl: despawn damage entity if present, then remove state
        if let Some(damage_entity) = whirling.damage_entity.take() {
            commands.entity(damage_entity).despawn();
        }
        finish_whirl(&mut commands, entity);
    }
}

fn finish_whirl(commands: &mut Commands, entity: Entity) {
    transition_action(commands, entity, Ready);
}
