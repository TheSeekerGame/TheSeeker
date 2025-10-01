use bevy::prelude::*;

use crate::game::enemy::Enemy;
use crate::game::player::input_buffer::InputBuffer;
use crate::game::player::passives::frenzied_attack::EnergyRegenDeltas;
use crate::game::player::spawns::amplified_bell::Bell;
use crate::game::player::states::{
    transition_action, FlickerPhase, FlickerStriking, WeaponType,
};
use crate::game::player::weapon::PlayerMeleeWeapon;
use crate::game::player::{
    FlickerAbility, Passives, Player, PlayerAction, PlayerStatMod,
};
use super::types::FlickerStrikeMetadata;
use theseeker_engine::time::GameTime;

// Debug logging helper (compiled only in debug builds)
macro_rules! debug_log {
    ($($arg:tt)*) => {
        #[cfg(debug_assertions)]
        {
            println!("[FLICKER_DRIVER] {}", format!($($arg)*));
        }
    };
}

pub(crate) const FLICKER_STRIKE_METADATA: FlickerStrikeMetadata = FlickerStrikeMetadata {
    max_energy: 3.0,
    chunk_cost: 0.25,
    regen_per_second: 0.075,
    range: 150.0,
};

/// Try to start Flicker Strike
pub fn try_start_flicker_strike_slot(
    entity: Entity,
    commands: &mut Commands,
    action_state: &leafwing_input_manager::action_state::ActionState<
        PlayerAction,
    >,
    buffer: &mut InputBuffer,
    melee_weapon: &PlayerMeleeWeapon,
    now_tick: u64,
    flicker_ability: &FlickerAbility,
    slot_action: PlayerAction,
    player_transform: &Transform,
    enemy_query: &Query<(Entity, &Transform), Or<(With<Enemy>, With<Bell>)>>,
    passives: &Passives,
) -> bool {
    debug_log!(
        "DRIVER: Checking flicker strike for slot {:?}, entity {:?}",
        slot_action,
        entity
    );

    // Check if requested
    let requested = action_state.just_pressed(&slot_action)
        || buffer.check_buffered(slot_action, now_tick).is_some();
    debug_log!("DRIVER: Requested: {}", requested);
    if !requested {
        debug_log!("DRIVER: Not requested, returning false");
        return false;
    }

    // Energy gate: require enough for one hit chunk
    debug_log!(
        "DRIVER: Energy check - current: {:.2}, required_chunk: {:.2}",
        flicker_ability.energy,
        FLICKER_STRIKE_METADATA.chunk_cost
    );
    if flicker_ability.energy < FLICKER_STRIKE_METADATA.chunk_cost {
        debug_log!("DRIVER: Insufficient energy, returning false");
        return false;
    }

    // Check for enemies in range
    let player_pos = player_transform.translation.truncate();
    let mut enemy_count = 0;
    let mut enemies_in_range = 0;
    for (_, enemy_transform) in enemy_query.iter() {
        enemy_count += 1;
        let enemy_pos = enemy_transform.translation.truncate();
        let distance = player_pos.distance(enemy_pos);
        if distance <= FLICKER_STRIKE_METADATA.range {
            enemies_in_range += 1;
        }
    }

    debug_log!(
        "DRIVER: Enemy check - {} enemies total, {} in range (range: {})",
        enemy_count,
        enemies_in_range,
        FLICKER_STRIKE_METADATA.range
    );

    if enemies_in_range == 0 {
        debug_log!("DRIVER: No enemies in range, returning false");
        return false;
    }

    // Determine weapon type
    let weapon_type = match *melee_weapon {
        PlayerMeleeWeapon::Sword => WeaponType::Sword,
        PlayerMeleeWeapon::Hammer => WeaponType::Hammer,
    };
    debug_log!("DRIVER: Weapon type: {:?}", weapon_type);

    // Calculate ticks per animation frame based on relevant CDR passives
    let has_cdr_passive = passives.equipped.iter().any(|passive| {
        matches!(
            passive,
            crate::game::player::Passive::SerpentRing
                | crate::game::player::Passive::DeadlyFeather
                | crate::game::player::Passive::FrenziedAttack
        )
    });
    debug_log!(
        "DRIVER: Has CDR passive: {}",
        has_cdr_passive
    );

    let ticks_per_frame = if has_cdr_passive { 6 } else { 8 };
    debug_log!(
        "DRIVER: Ticks per frame: {}",
        ticks_per_frame
    );

    buffer.clear_action(slot_action);
    let mut flickering =
        FlickerStriking::new(weapon_type).with_ticks_per_frame(ticks_per_frame);
    flickering.slot_action = Some(slot_action);

    debug_log!("DRIVER: Starting FlickerStrike - transitioning action state");
    transition_action(commands, entity, flickering);
    debug_log!("DRIVER: FlickerStrike started successfully");
    true
}

/// Regenerate flicker energy while not flickering
pub fn flicker_energy_regen(
    mut query: Query<
        (
            Entity,
            &mut FlickerAbility,
            Option<&PlayerStatMod>,
        ),
        (With<Player>, Without<FlickerStriking>),
    >,
    mut energy_deltas: ResMut<EnergyRegenDeltas>,
    time: Res<GameTime>,
) {
    // Clear previous tick values once per tick
    energy_deltas.flicker.clear();
    for (entity, mut flicker_ability, stat_mod) in query.iter_mut() {
        let cdr = stat_mod.map(|s| s.cdr).unwrap_or(1.0);
        let regen_per_tick =
            FLICKER_STRIKE_METADATA.regen_per_second * cdr / time.hz as f32;
        let capacity =
            (FLICKER_STRIKE_METADATA.max_energy - flicker_ability.energy)
                .max(0.0);
        let applied = regen_per_tick.min(capacity);
        if applied > 0.0 {
            flicker_ability.energy += applied;
            energy_deltas.flicker.insert(entity, applied);
        }
    }
}

/// Main update for active flicker strike
pub fn flicker_active_tick(
    mut commands: Commands,
    mut query: Query<
        (
            Entity,
            &mut FlickerStriking,
            &mut FlickerAbility,
            &leafwing_input_manager::action_state::ActionState<PlayerAction>,
            Option<&PlayerStatMod>,
        ),
        With<Player>,
    >,
    _time: Res<GameTime>,
) {
    for (entity, mut flickering, mut _flicker_ability, input, _stat_mod) in
        query.iter_mut()
    {
        debug_log!(
            "ACTIVE_TICK: Entity {:?} in phase {:?}",
            entity,
            flickering.phase
        );

    // Energy is debited in chunks at damage start; no per-tick drain here

        // Check if should continue
        let button_held = if let Some(slot_action) = flickering.slot_action {
            input.pressed(&slot_action)
        } else {
            false
        };

        debug_log!(
            "ACTIVE_TICK: Button held: {}",
            button_held
        );

        // Handle outro phase specially: must complete
        if matches!(
            flickering.phase,
            crate::game::player::states::FlickerPhase::Outro
        ) {
            debug_log!("ACTIVE_TICK: In outro phase, cannot cancel");
            // Outro cannot be cancelled, just wait for completion
            return;
        }

        if !button_held {
            debug_log!(
                "ACTIVE_TICK: Requesting outro - button released, phase: {:?}",
                flickering.phase
            );
            // Do not cancel during dash or intro; defer outro until safe
            if matches!(
                flickering.phase,
                FlickerPhase::Dashing | FlickerPhase::Intro
            ) {
                flickering.pending_outro = true;
            } else {
                // Not mid-dash/intro: safe to transition immediately
                commands.entity(entity).insert(FlickerOutroTransition);
            }
        }
    }
}

// Marker for transitioning to outro
#[derive(Component)]
pub struct FlickerOutroTransition;
