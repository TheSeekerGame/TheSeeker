use crate::game::effects::{chilled, stealthed};
use crate::prelude::*;
use theseeker_engine::physics::PhysicsSet;
use theseeker_engine::script::ScriptSet;
use theseeker_engine::time::{GameTickUpdate, GameTimeAppExt};

/// Orders player sensing, input, behavior, collision, and presentation.
/// Passive stat modifiers are refreshed before effects and movement consume them;
/// combat events are processed after damage integration.
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

/// Admission commits before background locomotion and foreground execution.
#[derive(SystemSet, Clone, PartialEq, Eq, Debug, Hash)]
pub enum PlayerBehaviorSet {
    Activation,
    Locomotion,
    Execution,
}

impl Plugin for PlayerPlugin {
    #[expect(
        clippy::too_many_lines,
        reason = "This Bevy plugin method declares player state-machine systems and schedule ordering in one auditable block."
    )]
    fn build(&self, app: &mut App) {
        // Configure system set ordering
        app.configure_sets(
            GameTickUpdate,
            (
                PlayerStateSet::Sensors.after(PhysicsSet),
                PlayerStateSet::Input.after(PlayerStateSet::Sensors),
                PlayerStateSet::Behavior.after(PlayerStateSet::Input),
                PlayerStateSet::Collisions.after(PlayerStateSet::Behavior),
                PlayerStateSet::Animation.after(PlayerStateSet::Collisions),
            ),
        );

        app.configure_sets(GameTickUpdate, (
            PlayerBehaviorSet::Activation,
            PlayerBehaviorSet::Locomotion,
            PlayerBehaviorSet::Execution,
        ).chain().in_set(PlayerStateSet::Behavior));
        app.add_observer(super::foreground::interrupt_on_death);
        app.add_systems(GameTickUpdate, super::foreground::interrupt_stopped_playback
            .after(theseeker_engine::animation::AnimationSet::Signals)
            .before(PlayerBehaviorSet::Activation));
        app.init_resource::<super::spawn::PersistedPlayerProgression>();
        app.init_resource::<super::skills::portable_tent::TentInteriorAssets>();

        // Player stat management & passives runtime.
        app.add_gametick_event::<super::passives::PassiveEvent>();
        app.add_gametick_event::<super::passives::blood_shards::HealApplied>();
        app.init_resource::<
            super::passives::runtime::ElementalSpriteTintCache,
        >();
        app.register_type::<super::crits::Crits>();
        app.add_systems(
            GameTickUpdate,
            (
                super::passives::runtime::process_passive_stats,
                super::passives::runtime::apply_passive_animation_slots,
                super::crits::track_crits,
            )
                .chain()
                .in_set(PlayerStateSet::Behavior)
                .after(PlayerStateSet::Sensors),
        );
        app.add_systems(
            GameTickUpdate,
            super::passives::runtime::apply_elemental_crystal_player_tint
                .in_set(PlayerStateSet::Animation)
                .after(ScriptSet::Run)
                .run_if(in_state(AppState::InGame)),
        );
        app.add_systems(
            GameTickUpdate,
            super::skills::consumables::apply_consumable_stat_buffs
                .in_set(PlayerStateSet::Behavior)
                .after(super::passives::runtime::process_passive_stats)
                .before(super::skills::cooldowns::tick_cooldowns)
                .run_if(in_state(AppState::InGame)),
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
            (
                super::passives::runtime::process_passive_events_system,
                super::skills::consumables::tick_consumable_regeneration,
                super::passives::blood_shards::spawn_blood_shards_on_heal,
            )
                .chain()
                .after(crate::game::combat::damage_source::apply_damage)
                .after(
                    crate::game::combat::damage_source::emit_passive_events_from_damage,
                ),
        );
        app.add_systems(
            GameTickUpdate,
            (super::passives::runtime::emit_passive_events_from_xp,)
                .after(crate::game::player::spawns::xp_orbs::XpOrbSet),
        );
        app.add_systems(
            GameTickUpdate,
            (
                super::apply_vitality_overclock,
                super::update_serpentring_health,
                super::enforce_skill_slot_capacity,
                (
                    super::passives::blood_shards::update_blood_shards_pos,
                    super::passives::blood_shards::update_blood_shards_vel_and_hit,
                )
                    .chain(),
                super::passives::blood_shards::despawn_blood_shard_on_hit
                    .after(crate::game::combat::damage_source::apply_damage),
            )
                .after(PlayerStateSet::Behavior),
        );
        app.add_systems(
            GameTickUpdate,
            super::skills::consumables::sync_skill_stock_unlocks
                .after(PlayerStateSet::Behavior)
                .after(crate::game::player::spawns::item_drops::ItemDropSet)
                .run_if(in_state(AppState::InGame)),
        );
        app.add_systems(
            GameTickUpdate,
            (
                super::skills::placement_preview::update_placement_previews,
                super::skills::portable_tent::resolve_pending_tent_entry,
                super::skills::placement_preview::cleanup_orphaned_previews,
            )
                .chain()
                .in_set(PlayerStateSet::Behavior)
                .run_if(in_state(AppState::InGame)),
        );
        app.add_systems(
            GameTickUpdate,
            super::skills::portable_tent::exit_tent_on_exit_overlap
                .after(PlayerStateSet::Collisions)
                .run_if(in_state(AppState::InGame)),
        );
        app.add_systems(
            OnEnter(AppState::InGame),
            super::skills::portable_tent::load_tent_interior_image,
        );
        app.add_systems(
            Update,
            (
                super::skills::portable_tent::populate_tent_interior_map,
                super::skills::portable_tent::sync_tent_interior_backdrop,
                super::skills::portable_tent::sync_tent_interior_view,
            )
                .chain()
                .run_if(in_state(AppState::InGame)),
        );

        // Dash after-images use the generic ghosting system
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
            )
                .chain()
                .in_set(PlayerStateSet::Behavior)
                .after(super::passives::runtime::process_passive_stats)
                .run_if(in_state(AppState::InGame)),
        );

        // Chilled effect - Slows movement and applies visual tint
        app.add_systems(
            GameTickUpdate,
            (
                chilled::chilled_effect_system,
                chilled::chilled_exit_restoration_system,
            )
                .chain()
                .in_set(PlayerStateSet::Behavior)
                .after(super::passives::runtime::process_passive_stats)
                .run_if(in_state(AppState::InGame)),
        );

        // Pogo runs inside states::skill::attacking, after attack systems.

        // Core player plugins
        app.add_plugins((
            super::sensors::SensorPlugin,
            super::input_buffer::InputBufferPlugin,
            super::player_action::PlayerActionPlugin,
            super::autoaim::RangedAutoAimPlugin,
            super::passives::pulse_echo::PulseEchoPlugin,
            super::collision::PlayerCollisionPlugin,
            super::dust::PlayerDustPlugin,
            super::player_anim::PlayerAnimationPlugin,
            super::weapon::PlayerWeaponPlugin,
            super::equipment::EquipmentPlugin,
            super::progression::PlayerProgressionPlugin,
        ));

        // State plugins
        app.add_plugins((
            super::states::IdleStatePlugin,
            super::states::RunStatePlugin,
            super::states::JumpingStatePlugin,
            super::states::FallingStatePlugin,
            super::states::ElevatorStatePlugin,
            super::states::GatheringStatePlugin,
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
        app.add_plugins(
            crate::game::player::spawns::quark_pylon::QuarkPylonPlugin,
        );
        app.add_plugins(
            crate::game::player::spawns::tether_node::TetherNodePlugin,
        );
        app.add_plugins(crate::game::player::spawns::spit::SpitPlugin);
        app.add_plugins(
            crate::game::player::spawns::item_drops::ItemDropsPlugin,
        );

        // Initialize skills framework resources
        app.add_plugins(super::skills::SkillsPlugin);


    }
}
