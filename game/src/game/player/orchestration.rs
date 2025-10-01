use crate::prelude::*;
use theseeker_engine::time::{GameTickUpdate, GameTimeAppExt};
use crate::game::effects::{chilled, stealthed};

/// Player Plugin - Orchestrates all player-related systems
///
/// ## Stat Modification Architecture
///
/// Player stats flow through a deterministic pipeline each game tick:
///
/// ### 1. Base Stats (`PlayerStats`)
/// - **Base values**: Stored in `PlayerStats.base_stats` (e.g., MoveVelMax = 0.729 per tick)
/// - **Effective values**: Stored in `PlayerStats.effective_stats` (initially mirrors base)
/// - **Initialization**: Set once via `load_player_stats` when player spawns
/// - **Access**: Movement systems call `stats.get(StatType::MoveVelMax)`
///
/// ### 2. Stat Modifiers (`PlayerStatMod`)
/// - **Purpose**: Multiplicative modifiers that stack (damage, defense, speed, cdr)
/// - **Reset**: Every tick to 1.0 (baseline) by the passive system
/// - **Application**: Passives and effects multiply these values
/// - **Final calculation**: `base_speed * stat_mod.speed * other_multipliers`
///
/// ### 3. System Execution Order (per tick)
///
/// ```
/// PlayerStateSet::Sensors
///   └─ Sensor systems (ground, wall, enemy proximity)
///
/// PlayerStateSet::Input  
///   └─ Input buffer and action processing
///
/// PlayerStateSet::Behavior
///   ├─ load_player_stats (only on Added<PlayerStats>)
///   ├─ process_passives_new_system
///   │   └─ Resets PlayerStatMod to 1.0, applies passive modifiers
///   ├─ Effect systems (after passives)
///   │   ├─ stealth_effect_system: Multiplies speed by 1.15
///   │   └─ chilled_effect_system: Multiplies speed by 0.5
///   └─ State update systems (running, jumping, etc.)
///       └─ Read stats and stat_mod to determine final velocity
/// ```
///
/// ### 4. Key Design Principles
/// - **Multiplicative stacking**: All modifiers multiply together
/// - **Frame-perfect control**: Movement uses per-tick displacements, not forces
/// - **Clear data flow**: Base → Passives → Effects → Movement
/// - **No hidden state**: All modifiers visible in PlayerStatMod
pub struct PlayerPlugin;

/// Ordered system sets for player-related logic
#[derive(SystemSet, Clone, PartialEq, Eq, Debug, Hash)]
pub enum PlayerStateSet {
    Sensors,
    Input,
    Behavior,
    Collisions,
    Animation,
}

impl Plugin for PlayerPlugin {
    fn build(&self, app: &mut App) {
        // Configure system set ordering
        app.configure_sets(
            GameTickUpdate,
            (
                PlayerStateSet::Sensors,
                PlayerStateSet::Input.after(PlayerStateSet::Sensors),
                PlayerStateSet::Behavior.after(PlayerStateSet::Input),
                PlayerStateSet::Collisions.after(PlayerStateSet::Behavior),
                PlayerStateSet::Animation.after(PlayerStateSet::Collisions),
            ),
        );

        // Init auto-aim decisiveness state
        app.init_resource::<super::BowAutoAimState>();

        // Player stat management & passives runtime.
        // Passive events are processed on the next tick for clear ordering and determinism.
        app.add_gametick_event::<super::passives::PassiveEvent>();
        app.register_type::<super::crits::Crits>();
        app.add_systems(
            GameTickUpdate,
            (
                super::stats::load_player_stats,
                super::track_hits,
                super::passives::runtime::process_passives_new_system,
                super::passives::runtime::apply_passive_animation_slots,
                super::crits::track_crits,
            )
                .chain()
                .in_set(PlayerStateSet::Behavior)
                .after(PlayerStateSet::Sensors),
        );
        // Track skill rotation state for Permutator passives
        app.add_systems(
            GameTickUpdate,
            super::passives::permutator::permutator_skill_tracker
                .in_set(PlayerStateSet::Behavior)
                .run_if(in_state(AppState::InGame)),
        );
        // Process passive events after damage has been applied so crit and damage events are visible
        app.add_systems(
            GameTickUpdate,
            (super::passives::runtime::process_passive_events_system)
                .after(crate::game::combat::damage_source::apply_damage)
                .in_set(PlayerStateSet::Behavior),
        );
        app.add_systems(
            GameTickUpdate,
            (super::passives::runtime::emit_passive_events_from_xp,)
                .after(PlayerStateSet::Behavior),
        );
        app.add_systems(
            GameTickUpdate,
            (
                super::apply_vitality_overclock,
                super::update_serpentring_health,
                super::enforce_skill_slot_capacity,
            )
                .after(PlayerStateSet::Behavior),
        );

        // Dash after-image FX are now handled by the generic ghosting system
        // in engine/src/effects/ghosting.rs via the GhostingSource component

        // Spawn/despawn
        app.add_systems(
            GameTickUpdate,
            ((
                super::spawn::setup_player,
                super::spawn::despawn_dead_player,
            )
                .run_if(in_state(GameState::Playing)))
            .after(PlayerStateSet::Animation)
            .run_if(in_state(AppState::InGame)),
        );

        // Ground maintenance
        app.add_systems(
            GameTickUpdate,
            super::manage_grounded_component
                .in_set(PlayerStateSet::Behavior)
                .after(PlayerStateSet::Sensors)
                .run_if(in_state(AppState::InGame)),
        );

        // Cooldowns and whirl energy model at the skills layer
        app.add_systems(
            GameTickUpdate,
            super::skills::cooldowns::tick_cooldowns
                .in_set(PlayerStateSet::Behavior)
                .run_if(in_state(AppState::InGame)),
        );
        // Register runtime resources used by Frenzied Attack
        app.init_resource::<super::passives::frenzied_attack::EnergyRegenDeltas>();
        app.init_resource::<super::passives::frenzied_attack::LifeDebt>();
        app.add_systems(
            GameTickUpdate,
            super::skills::whirl::whirl_energy_regen
                .in_set(PlayerStateSet::Behavior)
                .run_if(in_state(AppState::InGame)),
        );
        app.add_systems(
            GameTickUpdate,
            super::skills::whirl::whirl_active_tick
                .after(super::states::skill::whirling::whirling_update_system)
                .in_set(PlayerStateSet::Behavior)
                .run_if(in_state(AppState::InGame)),
        );
        app.add_systems(
            GameTickUpdate,
            super::skills::flicker_strike::flicker_energy_regen
                .in_set(PlayerStateSet::Behavior)
                .run_if(in_state(AppState::InGame)),
        );
        app.add_systems(
            GameTickUpdate,
            super::skills::flicker_strike::flicker_active_tick
                .in_set(PlayerStateSet::Behavior)
                .run_if(in_state(AppState::InGame)),
        );
        // Emit synthetic repeats for any channeled skill (generic marker)
        app.add_systems(
            GameTickUpdate,
            super::skills::channeled::channeled_skill_tick_and_emit_repeats
                .after(super::skills::whirl::whirl_active_tick)
                .after(super::skills::flicker_strike::flicker_active_tick)
                .in_set(PlayerStateSet::Behavior)
                .run_if(in_state(AppState::InGame)),
        );
        // Apply Frenzied Attack post-processing after cooldown/energy updates
        app.add_systems(
            GameTickUpdate,
            super::passives::frenzied_attack::frenzied_attack_runtime
                .after(super::skills::cooldowns::tick_cooldowns)
                .after(super::skills::whirl::whirl_energy_regen)
                .after(super::skills::flicker_strike::flicker_energy_regen)
                .after(super::skills::explosive_mine::mine_energy_regen)
                .in_set(PlayerStateSet::Behavior)
                .run_if(in_state(AppState::InGame)),
        );

        // Movement speed effects - Apply after passives but before movement systems
        // CRITICAL ORDERING: These effects modify PlayerStatMod.speed multiplicatively
        // They must run AFTER the passive system resets PlayerStatMod to baseline (1.0)
        // but BEFORE movement systems read the final speed value.
        // This allows effects to stack properly: base_speed * passives * effects

        // Stealth effect - Speeds up movement and provides visual transparency
        app.add_systems(
            GameTickUpdate,
            (
                stealthed::stealth_effect_system,
                stealthed::stealth_exit_visibility_system,
                stealthed::stealth_damage_break_system,
                stealthed::start_stealth_cooldown_on_remove,
                super::reset_autoaim_on_stealth_change,
            )
                .chain()
                .in_set(PlayerStateSet::Behavior)
                .after(super::passives::runtime::process_passives_new_system)
                .run_if(in_state(AppState::InGame)),
        );

        // Chilled effect - Slows movement and applies visual tint
        app.add_systems(
            GameTickUpdate,
            (
                chilled::chilled_effect_system,
                chilled::chilled_exit_restoration_system,
                chilled::chilled_refresh_system,
            )
                .chain()
                .in_set(PlayerStateSet::Behavior)
                .after(super::passives::runtime::process_passives_new_system)
                .run_if(in_state(AppState::InGame)),
        );

        // Input-time auto-aim for bow
        app.add_systems(
            GameTickUpdate,
            (super::autoaim::continuous_bow_auto_aim)
                .in_set(PlayerStateSet::Input)
                .run_if(in_state(AppState::InGame)),
        );

        // Pogo is now handled inside states::skill::attacking, after attack systems

        // Core player plugins
        app.add_plugins((
            super::sensors::SensorPlugin,
            super::input_buffer::InputBufferPlugin,
            super::player_action::PlayerActionPlugin,
            super::collision::PlayerCollisionPlugin,
            super::player_anim::PlayerAnimationPlugin,
            super::weapon::PlayerWeaponPlugin,
            super::equipment::EquipmentPlugin,
        ));

        // State plugins
        app.add_plugins((
            super::states::IdleStatePlugin,
            super::states::RunStatePlugin,
            super::states::JumpingStatePlugin,
            super::states::FallingStatePlugin,
            super::states::ReadyStatePlugin,
            super::states::AttackingStatePlugin,
            super::states::DashingStatePlugin,
            super::states::WhirlingStatePlugin,
            super::states::BurningDashingStatePlugin,
            super::states::FlickerStrikingStatePlugin,
            crate::game::player::states::skill::dash_striking::DashStrikingStatePlugin,
        ));

        // Spawnables under player spawns (e.g., Amplified Bell, XP orbs, Kinetic Orb)
        app.add_plugins(
            crate::game::player::spawns::amplified_bell::AmplifiedBellPlugin,
        );
        app.add_plugins(crate::game::player::spawns::xp_orbs::XpOrbsPlugin);
        app.add_plugins(
            crate::game::player::spawns::kinetic_orb::KineticOrbPlugin,
        );
        app.add_plugins(crate::game::player::spawns::spinner::SpinnerPlugin);
        app.add_plugins(crate::game::player::spawns::mine::ExplosiveMinePlugin);
        app.add_plugins(crate::game::player::spawns::ice_nova::IceNovaPlugin);

        // Initialize skills framework resources
        app.add_plugins(super::skills::SkillsPlugin);
        // Mine energy systems and debit-on-use
        app.add_systems(
            GameTickUpdate,
            (
                super::skills::explosive_mine::mine_energy_regen,
                super::skills::explosive_mine::mine_energy_debit_on_spawn,
            )
                .in_set(PlayerStateSet::Behavior)
                .run_if(in_state(AppState::InGame)),
        );

        // Centralized dash-end animation restart
        app.add_systems(
            GameTickUpdate,
            restart_animation_on_dash_end
                .in_set(PlayerStateSet::Animation)
                .run_if(in_state(AppState::InGame)),
        );
    }
}

use theseeker_engine::assets::animation::SpriteAnimation;
use theseeker_engine::gent::Gent;
use theseeker_engine::script::ScriptPlayer;

/// When dashing ends, restart Idle/Run animation based on current locomotion state.
fn restart_animation_on_dash_end(
    mut removed: RemovedComponents<super::states::Dashing>,
    idle_q: Query<(&Gent, &crate::game::gentstate::Facing), With<super::states::Idle>>,
    running_q: Query<(&Gent, &crate::game::gentstate::Facing), With<super::states::Running>>,
    mut gfx_q: Query<&mut ScriptPlayer<SpriteAnimation>, With<super::PlayerGfx>>,
) {
    for e in removed.read() {
        if let Ok((gent, facing)) = idle_q.get(e) {
            if let Ok(mut anim) = gfx_q.get_mut(gent.e_gfx) {
                super::player_anim::set_direction_slots(&mut anim, facing);
                anim.set_slot("AttackTransition", false);
                anim.set_slot("DownwardAttack", false);
                anim.play_key("anim.player.Idle");
            }
            continue;
        }
        if let Ok((gent, facing)) = running_q.get(e) {
            if let Ok(mut anim) = gfx_q.get_mut(gent.e_gfx) {
                super::player_anim::set_direction_slots(&mut anim, facing);
                anim.play_key("anim.player.Run");
            }
            continue;
        }
    }
}
