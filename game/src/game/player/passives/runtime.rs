use bevy::prelude::*;
use leafwing_input_manager::prelude::ActionState;
use theseeker_engine::assets::animation::SpriteAnimation;
use theseeker_engine::gent::Gent;
use theseeker_engine::physics::PhysicsWorld;
use theseeker_engine::script::ScriptPlayer;
use theseeker_engine::time::GameTime;

use crate::game::player::player_action::PlayerAction;
use crate::game::player::states::{InAir, Running};

use super::super::sensors::EnemyProximitySensor;
use super::super::{
    EnemiesNearby, Passives, Player, PlayerStatMod, WhirlAbility,
};
use crate::game::combat::Health;

fn add_energy_clamped(current: &mut f32, amount: f32, max: f32) {
    if amount <= 0.0 {
        return;
    }
    *current = (*current + amount).min(max);
}

/// Bridge XP orb pickups into the passive event stream.
pub fn emit_passive_events_from_xp(
    mut xp_events: EventReader<
        crate::game::player::spawns::xp_orbs::XpOrbPickup,
    >,
    mut passive_events: EventWriter<super::PassiveEvent>,
) {
    for _ in xp_events.read() {
        passive_events.write(super::PassiveEvent::XpOrbPickup);
    }
}

/// Update player stat modifiers each tick based on equipped passives.
///
/// This system runs early in PlayerStateSet::Behavior and:
/// 1. Resets PlayerStatMod to baseline (1.0 for multipliers, 0 for additions)
/// 2. Applies all passive effects in priority order
/// 3. Leaves PlayerStatMod ready for further modification by effects (e.g., chilled)
///
/// This runs every tick to ensure stat modifiers are always fresh and
/// correctly reflect the current game state (health %, enemies nearby, etc.)
pub fn process_passives_new_system(
    mut query: Query<
        (
            Entity,
            &Passives,
            &mut PlayerStatMod,
            &mut Health,
            &theseeker_engine::physics::LinearVelocity,
            &ActionState<PlayerAction>,
            &EnemiesNearby,
            &super::super::BuffTick,
            Option<&super::super::Grounded>,
            Option<&InAir>,
            Option<&Running>,
            Option<&mut WhirlAbility>,
            Option<&EnemyProximitySensor>,
            Option<&mut crate::game::player::crits::Crits>,
            Option<&super::super::LastGroundedPosition>,
        ),
        With<Player>,
    >,
    perm_q: Query<Option<&super::permutator::PermutatorState>, With<Player>>,
    mut _passive_events: EventReader<super::PassiveEvent>,
    _damage_source_query: Query<&crate::game::combat::DamageSource>,
    mut _cooldowns: ResMut<crate::game::player::skills::cooldowns::Cooldowns>,
) {
    use super::{get_passive_implementation, PassiveContext, PassiveContextInputs};

    for (
        entity,
        passives_component,
        mut stats,
        mut health,
        velocity,
        action_state,
        enemies_nearby,
        buff_tick,
        grounded,
        in_air,
        running,
        _maybe_whirl,
        enemy_proximity,
        _maybe_crits,
        last_grounded_pos,
    ) in query.iter_mut()
    {
        let mut active_passives: Vec<Box<dyn super::PassiveEffect>> =
            passives_component
                .equipped
                .iter()
                .map(|p| get_passive_implementation(p))
                .collect();
        active_passives.sort_by_key(|p| -p.priority());

        // Reset all stat modifiers to baseline (these are multiplicative modifiers)
        stats.damage = 1.0;
        stats.defense = 1.0;
        stats.speed = 1.0;
        stats.cdr = 1.0;
        stats.extra_jumps = 0;

        let enemies_nearby_count: u32 = enemy_proximity
            .map(|s| s.enemies_in_melee_range.len() as u32)
            .unwrap_or(enemies_nearby.0);
        let base_context = PassiveContext::from_inputs(PassiveContextInputs {
            health: Some(health.as_ref()),
            velocity: Some(velocity.0),
            enemies_nearby: Some(enemies_nearby_count),
            grounded: grounded.is_some(),
            in_air: in_air.is_some(),
            buff_stacks: Some(buff_tick.stacks),
            closest_enemy_distance: enemy_proximity.map(|s| s.closest_distance),
            is_running: running.is_some(),
            target_position: None,
            last_grounded_position: last_grounded_pos.map(|p| p.position),
            movement_input: Some(action_state.clamped_value(&PlayerAction::Move)),
            jump_pressed: Some(action_state.pressed(&PlayerAction::Jump)),
            enemies_near_target: Some(0),
            target_health_pct: None,
            rotation_active: perm_q
                .get(entity)
                .ok()
                .flatten()
                .map(|p| p.active)
                .unwrap_or(false),
        });

        for passive in &active_passives {
            passive.modify_stats(&mut stats, &base_context);
        }

        // Event-driven actions are handled in `process_passive_events_system`.
    }
}

/// Apply animation slot toggles exported by passives (e.g., cosmetic markers).
pub fn apply_passive_animation_slots(
    player_query: Query<(&Passives, Option<&Running>, &Gent), With<Player>>,
    mut gfx_query: Query<
        &mut ScriptPlayer<SpriteAnimation>,
        With<super::super::PlayerGfx>,
    >,
) {
    use super::get_passive_implementation;
    for (passives, _running, gent) in player_query.iter() {
        if let Ok(mut anim) = gfx_query.get_mut(gent.e_gfx) {
            anim.set_slot("SerpentRing", false);
            anim.set_slot("FrenziedAttack", false);
            let mut slot_updates: Vec<(&'static str, bool)> = Vec::new();
            let mut impls: Vec<Box<dyn super::PassiveEffect>> = passives
                .equipped
                .iter()
                .map(get_passive_implementation)
                .collect();
            impls.sort_by_key(|p| -p.priority());
            for p in impls.iter() {
                slot_updates.extend(p.animation_slots());
            }
            for (name, value) in slot_updates {
                anim.set_slot(name, value);
            }
        }
    }
}

/// Process passive events and apply resulting actions (heals, cooldown changes, etc.).
/// Runs after emitters (e.g., damage source pipeline, XP bridge) and before event clear.
pub fn process_passive_events_system(
    mut commands: Commands,
    mut query: Query<
        (
            Entity,
            &Passives,
            &mut Health,
            &theseeker_engine::physics::LinearVelocity,
            &ActionState<PlayerAction>,
            &EnemiesNearby,
            &super::super::BuffTick,
            Option<&super::super::Grounded>,
            Option<&InAir>,
            Option<&Running>,
            Option<&mut WhirlAbility>,
            Option<&mut super::super::FlickerAbility>,
            Option<&EnemyProximitySensor>,
            Option<&mut crate::game::player::crits::Crits>,
            Option<&super::super::LastGroundedPosition>,
        ),
        With<Player>,
    >,
    pose_query: Query<
        (
            &Transform,
            &crate::game::gentstate::Facing,
            &super::super::SkillInventory,
            &crate::game::player::sensors::GroundSensor,
        ),
        With<Player>,
    >,
    mut mine_q: Query<
        &mut crate::game::player::skills::explosive_mine::ExplosiveMineAbility,
        With<Player>,
    >,
    mut passive_events: EventReader<super::PassiveEvent>,
    damage_source_query: Query<&crate::game::combat::DamageSource>,
    mut cooldowns: ResMut<crate::game::player::skills::cooldowns::Cooldowns>,
    perm_q: Query<Option<&super::permutator::PermutatorState>, With<Player>>,
    time: Res<GameTime>,
    spatial_query: PhysicsWorld,
) {
    use super::{
        get_passive_implementation, PassiveAction, PassiveContext,
        PassiveContextInputs,
    };
    for (
        entity,
        passives_component,
        mut health,
        velocity,
        action_state,
        enemies_nearby,
        buff_tick,
        grounded,
        in_air,
        running,
        mut maybe_whirl,
        mut maybe_flicker,
        enemy_proximity,
        mut maybe_crits,
        last_grounded_pos,
    ) in query.iter_mut()
    {
        let Ok((transform, facing, skill_inventory, ground_sensor)) =
            pose_query.get(entity)
        else {
            continue;
        };
        // Build implementations sorted by priority
        let mut active_passives: Vec<Box<dyn super::PassiveEffect>> =
            passives_component
                .equipped
                .iter()
                .map(|p| get_passive_implementation(p))
                .collect();
        active_passives.sort_by_key(|p| -p.priority());

        let enemies_nearby_count: u32 = enemy_proximity
            .map(|s| s.enemies_in_melee_range.len() as u32)
            .unwrap_or(enemies_nearby.0);
        let base_context = PassiveContext::from_inputs(PassiveContextInputs {
            health: Some(health.as_ref()),
            velocity: Some(velocity.0),
            enemies_nearby: Some(enemies_nearby_count),
            grounded: grounded.is_some(),
            in_air: in_air.is_some(),
            buff_stacks: Some(buff_tick.stacks),
            closest_enemy_distance: enemy_proximity.map(|s| s.closest_distance),
            is_running: running.is_some(),
            target_position: None,
            last_grounded_position: last_grounded_pos.map(|p| p.position),
            movement_input: Some(action_state.clamped_value(&PlayerAction::Move)),
            jump_pressed: Some(action_state.pressed(&PlayerAction::Jump)),
            enemies_near_target: Some(0),
            target_health_pct: None,
            rotation_active: perm_q
                .get(entity)
                .ok()
                .flatten()
                .map(|p| p.active)
                .unwrap_or(false),
        });

        let mut actions_to_apply = Vec::new();
        for event in passive_events.read() {
            let applies = match event {
                super::PassiveEvent::SkillUsed { owner, .. } => {
                    *owner == entity
                },
                super::PassiveEvent::XpOrbPickup => true,
                super::PassiveEvent::EnemyKilled { owner } => *owner == entity,
                super::PassiveEvent::BackstabKill { owner } => *owner == entity,
                super::PassiveEvent::CriticalHit { damage_source, .. } => {
                    damage_source_query
                        .get(*damage_source)
                        .map(|ds| ds.owner == entity)
                        .unwrap_or(false)
                },
                super::PassiveEvent::DamageHit { damage_source } => {
                    damage_source_query
                        .get(*damage_source)
                        .map(|ds| ds.owner == entity)
                        .unwrap_or(false)
                },
                super::PassiveEvent::DamageDealt(info) => info.owner == entity,
                super::PassiveEvent::DamageTaken(info) => info.target == entity,
                super::PassiveEvent::HitCountAdvanced { owner, .. } => {
                    *owner == entity
                },
            };
            if applies {
                for passive in &active_passives {
                    actions_to_apply
                        .extend(passive.on_event(event, &base_context));
                }
            }
        }

        for action in actions_to_apply {
            match action {
                PassiveAction::Heal(amount) => {
                    health.current = (health.current + amount).min(health.max);
                },
                PassiveAction::Damage(amount) => {
                    health.current =
                        health.current.saturating_sub(amount).max(1);
                },
                PassiveAction::ReduceCooldowns(dt_seconds) => {
                    // Centralized: reduce all skills cooldowns for this entity by dt (in ticks)
                    let dt_ticks = dt_seconds * 96.0;
                    cooldowns.reduce_all(entity, dt_ticks);
                },
                PassiveAction::ResetCooldowns => {
                    cooldowns.reset_all(entity);
                },
                // Generic energy grant: apply to all channelled skills with energy pools
                PassiveAction::AddEnergy(amount) => {
                    if let Some(ref mut whirl) = maybe_whirl {
                        add_energy_clamped(
                            &mut whirl.energy,
                            amount,
                            crate::game::player::skills::types::whirl_metadata()
                                .max_energy,
                        );
                    }
                    if let Some(ref mut flicker) = maybe_flicker {
                        add_energy_clamped(
                            &mut flicker.energy,
                            amount,
                            crate::game::player::skills::types::flicker_strike_metadata()
                                .max_energy,
                        );
                    }
                    if let Ok(mut mine) = mine_q.get_mut(entity) {
                        add_energy_clamped(
                            &mut mine.energy,
                            amount,
                            crate::game::player::skills::types::explosive_mine_metadata()
                                .max_energy,
                        );
                    }
                },
                // Set energy to full capacity for channelled skills
                PassiveAction::RefillEnergyFull => {
                    if let Some(ref mut whirl) = maybe_whirl {
                        whirl.energy = crate::game::player::skills::types::whirl_metadata()
                            .max_energy;
                    }
                    if let Some(ref mut flicker) = maybe_flicker {
                        flicker.energy =
                            crate::game::player::skills::types::flicker_strike_metadata()
                                .max_energy;
                    }
                    if let Ok(mut mine) = mine_q.get_mut(entity) {
                        mine.energy = crate::game::player::skills::types::explosive_mine_metadata()
                            .max_energy;
                    }
                },
                // Back-compat: legacy variant that only targeted Whirl; now treated as generic
                PassiveAction::AddWhirlEnergy(amount) => {
                    if let Some(ref mut whirl) = maybe_whirl {
                        add_energy_clamped(
                            &mut whirl.energy,
                            amount,
                            crate::game::player::skills::types::whirl_metadata()
                                .max_energy,
                        );
                    }
                    if let Some(ref mut flicker) = maybe_flicker {
                        add_energy_clamped(
                            &mut flicker.energy,
                            amount,
                            crate::game::player::skills::types::flicker_strike_metadata()
                                .max_energy,
                        );
                    }
                    if let Ok(mut mine) = mine_q.get_mut(entity) {
                        add_energy_clamped(
                            &mut mine.energy,
                            amount,
                            crate::game::player::skills::types::explosive_mine_metadata()
                                .max_energy,
                        );
                    }
                },
                PassiveAction::ScheduleNextCrit => {
                    if let Some(crits) = maybe_crits.as_deref_mut() {
                        crits.schedule_next_crit();
                    }
                },
                PassiveAction::TriggerInstantSkills { source_damage } => {
                    let now_tick = time.tick();
                    for skill in &skill_inventory.equipped {
                        use crate::game::player::skills::types::SkillId;
                        if !skill.is_instant() {
                            continue;
                        }
                        match skill {
                            SkillId::Stealth => {
                                let mut effect =
                                    crate::game::effects::stealthed::StealthEffect::new(
                                        now_tick,
                                    );
                                effect.ignore_damage_from = source_damage;
                                commands.entity(entity).insert(effect);
                            },
                            SkillId::Spinner => {
                                let dir = facing.direction();
                                let spawn_pos =
                                    transform.translation.truncate() + Vec2::new(10.0 * dir, 0.0);
                                crate::game::player::spawns::spinner::spawn_spinner(
                                    &mut commands,
                                    entity,
                                    spawn_pos,
                                    dir,
                                );
                            },
                            SkillId::IceNova => {
                                crate::game::player::spawns::ice_nova::spawn_ice_nova(
                                    &mut commands,
                                    entity,
                                    transform.translation.truncate(),
                                );
                            },
                            SkillId::AmplifiedBell => {
                                if let Some(center) =
                                    crate::game::player::skills::amplified_bell::find_amplified_bell_placement(
                                        transform,
                                        facing,
                                        &spatial_query,
                                        entity,
                                    )
                                {
                                    crate::game::player::spawns::amplified_bell::spawn_bell(
                                        &mut commands,
                                        entity,
                                        center,
                                        facing.direction(),
                                    );
                                }
                            },
                            SkillId::ExplosiveMine => {
                                let meta =
                                    crate::game::player::skills::types::explosive_mine_metadata();
                                if let Ok(mut mine) = mine_q.get_mut(entity) {
                                    if mine.energy >= meta.chunk_cost {
                                        if let Some(spawn_pos) =
                                            crate::game::player::skills::explosive_mine::resolve_spawn_position(
                                                transform,
                                                ground_sensor,
                                            )
                                        {
                                            crate::game::player::spawns::mine::spawn_mine(
                                                &mut commands,
                                                entity,
                                                spawn_pos,
                                                facing.direction(),
                                            );
                                            mine.energy -= meta.chunk_cost;
                                        }
                                    }
                                }
                            },
                            _ => {},
                        }
                    }
                },
                _ => {},
            }
        }
    }
}
