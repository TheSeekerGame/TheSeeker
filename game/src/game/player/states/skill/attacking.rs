use bevy::ecs::hierarchy::ChildOf;
use bevy::ecs::schedule::IntoScheduleConfigs;
use bevy::prelude::*;
use theseeker_engine::assets::animation::SpriteAnimation;
use theseeker_engine::script::ScriptPlayer;
use theseeker_engine::time::GameTickUpdate;

use crate::game::player::{
    Passives, Player, PlayerStatMod, PlayerStateSet, PlayerStats,
};

// Auto-aim constants for melee attacks
const MELEE_AUTO_AIM_RANGE: f32 = 48.0;
const AUTO_AIM_VERTICAL_THRESHOLD: f32 = 8.0;
const AUTO_AIM_HORIZONTAL_LEEWAY: f32 = 24.0;

// Projectile tuning
const RANGED_ATTACK_LIFETIME_TICKS: u32 = 192;
const ARROW_VELOCITY: f32 = 500.0;

// Attack collider sizes (placeholders - AnimationCollider updates from magenta pixels)
const MELEE_ATTACK_COLLIDER_HALF_WIDTH: f32 = 8.0;
const MELEE_ATTACK_COLLIDER_HALF_HEIGHT: f32 = 8.0;
const ARROW_COLLIDER_HALF_WIDTH: f32 = 4.0;
const ARROW_COLLIDER_HALF_HEIGHT: f32 = 1.0;
use crate::game::combat::sparks::WeaponHitSlot;
use crate::game::player::sensors::EnemyProximitySensor;
use crate::game::player::weapon::{PlayerMeleeWeapon, PlayerRangedWeapon};
use crate::game::combat::{
    DamageSource, DownwardAttack, Pushback, SelfPushback, Stealthed,
};
use crate::game::enemy::Enemy;
use crate::game::gentstate::Facing;
use crate::game::physics::projectile::{Arrow, PreviousPosition, Projectile};
use crate::game::physics::Knockback;
use crate::game::player::sensors::CeilingSensor;
use crate::game::player::skills::cooldowns::Cooldowns;
use crate::game::player::skills::types::{
    attack_animation_metadata,
    attack_variant_metadata,
    SkillWeaponKind,
    Variant as SkillVariant,
};
use crate::game::player::states::{
    transition_action, transition_locomotion, AttackVariant, Attacking, Falling,
    Jumping, Ready, Running, WeaponType,
};
use crate::game::player::weapon::CurrentWeapon;
use crate::game::player::BowAutoAimState;
use crate::game::player::PlayerAction;
use crate::game::player::player_anim::set_direction_slots;
use leafwing_input_manager::action_state::ActionState;
use theseeker_engine::effects::{
    FadeCurve, GhostColorMode, GhostMovement, GhostingSource, ScaleCurve,
};
use theseeker_engine::gent::Gent;
use theseeker_engine::physics::AnimationCollider;
use theseeker_engine::physics::{groups, Collider, LinearVelocity};

// Pogo effect: upward velocity curve applied when a downward attack first hits an enemy
#[derive(Component, Debug)]
pub struct Pogo {
    pub tick: u32,
}

// Pogo per-tick upward displacements (converted from px/s by dividing by 96)
const POGO_VELOCITIES: [f32; 24] = [
    0.818, 0.781, 0.745, 0.708, 0.672, 0.635, 0.599, 0.563, 0.526, 0.490,
    0.453, 0.417, 0.380, 0.344, 0.307, 0.271, 0.234, 0.198, 0.161, 0.125,
    0.089, 0.052, 0.016, 0.000,
];

pub struct AttackingStatePlugin;

impl Plugin for AttackingStatePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            GameTickUpdate,
            (
                attacking_enter_system,
                attacking_update_system,
            )
                .chain()
                .run_if(any_with_component::<Attacking>)
                .in_set(PlayerStateSet::Behavior),
        );
        // Start pogo after attack pipeline tags hits
        app.add_systems(
            GameTickUpdate,
            start_pogo_on_downward_attack_hit.after(
                crate::game::combat::damage_source::apply_damage_modifications,
            ),
        );
        // Maintain pogo velocity in behavior set
        app.add_systems(
            GameTickUpdate,
            pogo_update_system.in_set(PlayerStateSet::Behavior),
        );
        // Attack preemption is handled by the general skill_dispatch_system in ready.rs
    }
}

// Durations are governed by the attack skill tables (see `skills::attack`).

// Get the attack animation key based on current locomotion state
// These match the format used in player_anim.rs: weapon.get_anim_key("BasicIdle") etc
fn get_attack_animation_key(
    weapon: WeaponType,
    _variant: AttackVariant,
    is_running: bool,
    is_airborne: bool,
) -> &'static str {
    let kind: SkillWeaponKind = weapon.into();
    let animations = attack_animation_metadata(kind);

    if is_airborne {
        animations.air
    } else if is_running {
        animations.run
    } else {
        animations.idle
    }
}

// System that runs when entering Attacking state
fn attacking_enter_system(
    mut commands: Commands,
    mut query: Query<
        (
            Entity,
            &mut Attacking,
            &Transform,
            &mut Facing,
            Option<&EnemyProximitySensor>,
            &Gent,
            Option<&PlayerStats>,
            Option<&PlayerStatMod>,
            Option<&Passives>,
            Has<Running>,
            Has<Jumping>,
            Has<Falling>,
    Has<crate::game::effects::stealthed::StealthEffect>,
        ),
        (With<Player>, Added<Attacking>),
    >,
    mut gfx_query: Query<&mut ScriptPlayer<SpriteAnimation>>,
    enemy_query: Query<&Transform, With<Enemy>>,
    melee_weapon: Res<PlayerMeleeWeapon>,
    ranged_weapon: Res<PlayerRangedWeapon>,
    mut _cooldowns: ResMut<Cooldowns>,
    _time: Res<theseeker_engine::time::GameTime>,
) {
    for (
        entity,
        mut attacking,
        transform,
        mut facing,
        proximity_sensor,
        gent,
        _stats,
        stat_mod,
        _passives,
        has_running,
        has_jumping,
        has_falling,
        is_stealthed,
    ) in query.iter_mut()
    {
        // Determine locomotion state
        let is_running = has_running;
        let is_airborne = has_jumping || has_falling;

        // Calculate auto-aim for melee weapons only
        let mut final_variant = if attacking.weapon_type.is_melee() {
            if let Some(proximity_sensor) = proximity_sensor {
                let player_pos = transform.translation.truncate();
                let mut variant = calculate_melee_auto_aim(
                    player_pos,
                    &facing,
                    proximity_sensor,
                    attacking.variant,
                    &enemy_query,
                );
                // Downward auto-aim for melee while airborne when no enemy is directly in front
                let is_airborne = has_jumping || has_falling;
                if is_airborne
                    && !proximity_sensor.has_forward_enemy
                    && proximity_sensor.has_enemy_below
                {
                    // Force downward variant regardless of Up key; no Down key required
                    variant = AttackVariant::Down;
                }
                variant
            } else {
                attacking.variant
            }
        } else {
            attacking.variant
        };

        attacking.variant = final_variant;

        // Start attack animation based on locomotion state
        if let Ok(mut script_player) = gfx_query.get_mut(gent.e_gfx) {
            // Ensure direction slots reflect current Facing before playing a key
            set_direction_slots(&mut script_player, &facing);
            let anim_key = get_attack_animation_key(
                attacking.weapon_type,
                final_variant,
                is_running,
                is_airborne,
            );

            // Airborne down attacks enable a dedicated slot
            if is_airborne && final_variant == AttackVariant::Down {
                script_player.enable_slot("DownwardAttack");
            }

            // Clear transition slot defensively to avoid SetFrameNext persistence across state changes
            script_player.set_slot("AttackTransition", false);
            script_player.play_key(anim_key);
        }

        // Spawn attack entity based on weapon type
        if let Some(stat_mod) = stat_mod {
            let base_damage = match attacking.weapon_type {
                WeaponType::Sword => PlayerMeleeWeapon::Sword.base_damage(),
                WeaponType::Hammer => PlayerMeleeWeapon::Hammer.base_damage(),
                WeaponType::Bow => PlayerRangedWeapon::Bow.base_damage(),
            };
            let damage = base_damage * stat_mod.damage;

            if attacking.weapon_type.is_melee() {
                // Melee: spawn attack entity with AnimationCollider. Lifetime equals active window.
                let skill_variant = match final_variant {
                    AttackVariant::Forward => SkillVariant::Forward,
                    AttackVariant::Up => SkillVariant::Up,
                    AttackVariant::Down => SkillVariant::Down,
                };
                let weapon_kind: SkillWeaponKind = attacking.weapon_type.into();
                let lifetime = attack_variant_metadata(weapon_kind, skill_variant)
                    .active_window
                    .duration_ticks;

                // Get pushback values based on weapon type
                let pushback_values = melee_weapon.pushback_values();

                let attack_entity = commands
                    .spawn((
                        DamageSource::new(lifetime, entity, damage),
                        Collider::cuboid(
                            MELEE_ATTACK_COLLIDER_HALF_WIDTH,
                            MELEE_ATTACK_COLLIDER_HALF_HEIGHT,
                        ), // Placeholder - AnimationCollider will update from magenta pixels
                        AnimationCollider(gent.e_gfx),
                        groups::player_attack(),
                        Transform::from_translation(Vec3::ZERO),
                        GlobalTransform::default(),
                        SelfPushback(Knockback::new(
                            Vec2::new(
                                pushback_values.self_pushback
                                    * -facing.direction(),
                                0.0,
                            ),
                            pushback_values.self_pushback_ticks,
                        )),
                        Pushback(Knockback::new(
                            Vec2::new(
                                facing.direction() * pushback_values.pushback,
                                0.0,
                            ),
                            pushback_values.pushback_ticks,
                        )),
                    ))
                    .insert(ChildOf(entity))
                    .id();

                // Mark weapon hit slot so sparks can play correct impact SFX
                let weapon_slot = match attacking.weapon_type {
                    WeaponType::Sword => "SwordHit",
                    WeaponType::Hammer => "HammerHit",
                    WeaponType::Bow => "BowHit",
                };
                commands.entity(attack_entity).insert(WeaponHitSlot {
                    slot_name: weapon_slot.to_string(),
                });

                // Add stealth if player is stealthed
                if is_stealthed {
                    commands.entity(attack_entity).insert(Stealthed);
                }

                // Add DownwardAttack component for airborne down attacks
                if final_variant == AttackVariant::Down && is_airborne {
                    commands.entity(attack_entity).insert(DownwardAttack);
                }
            } else if attacking.weapon_type == WeaponType::Bow {
                // Bow spawns a straight-flying arrow (no gravity)

                // Bow shoots forward only (no up/down variants)
                let dir = Vec2::X * facing.direction();
                let dir = dir.normalize_or_zero();
                if dir == Vec2::ZERO {
                } else {
                    let projectile = Projectile {
                        vel: LinearVelocity(dir * ARROW_VELOCITY),
                    };

                    // Get pushback values for ranged weapon
                    let pushback_values = ranged_weapon.pushback_values();

                    // Spawn attack with its own visual (no child) to avoid hierarchy issues
                    let mut arrow_script_player =
                        ScriptPlayer::<SpriteAnimation>::default();
                    arrow_script_player.play_key("anim.player.BowBasicArrow");

                    let attack_entity = commands
                        .spawn((
                            DamageSource::new(
                                RANGED_ATTACK_LIFETIME_TICKS,
                                entity,
                                damage,
                            )
                            .with_max_targets(1),
                            projectile,
                            Arrow, // Marker component for player arrows
                            PreviousPosition(transform.translation.xy()), // Track position for CCD
                            Collider::cuboid(
                                ARROW_COLLIDER_HALF_WIDTH,
                                ARROW_COLLIDER_HALF_HEIGHT,
                            ),
                            groups::player_attack(),
                            Pushback(Knockback::new(
                                Vec2::new(
                                    facing.direction()
                                        * pushback_values.pushback,
                                    0.0,
                                ),
                                pushback_values.pushback_ticks,
                            )),
                            // Visuals on the same entity as the attack to simplify lifecycle
                            arrow_script_player,
                            Sprite {
                                texture_atlas: Some(TextureAtlas::default()),
                                ..Default::default()
                            },
                            Transform::from_translation(transform.translation),
                            GlobalTransform::default(),
                            Visibility::Visible,
                            InheritedVisibility::default(),
                            // Add ghosting effect to arrow projectile
                            GhostingSource {
                                spawn_interval_ticks: 3, // Spawn ghost every 3 ticks for subtle trail
                                ghost_lifetime_ticks: 19, // ~0.2 seconds at 96Hz
                                initial_alpha: 0.3,       // Semi-transparent
                                fade_curve: FadeCurve::Linear,
                                color_mode: GhostColorMode::Tint(Color::srgb(
                                    3.0, 3.0, 3.0,
                                )), // Brighten toward white
                                scale_over_time: ScaleCurve::Constant, // No scale changes
                                offset: Vec2::ZERO,
                                movement: GhostMovement::Static,
                                ticks_since_last_spawn: 0,
                            },
                        ))
                        .id();

                    // Mark weapon hit slot for bow impacts so sparks pick correct SFX
                    commands.entity(attack_entity).insert(WeaponHitSlot {
                        slot_name: "BowHit".to_string(),
                    });

                    if is_stealthed {
                        commands.entity(attack_entity).insert(Stealthed);
                    }
                }
            }
        }
        // Lifetime provided by skills at start; no per-state recalculation here.

        // Do not (re)stamp cooldown here; gating and stamping are owned by skills::attack before state entry.
    }
}

// Main update system for Attacking state
fn attacking_update_system(
    mut commands: Commands,
    mut query: Query<
        (
            Entity,
            &mut Attacking,
            &mut theseeker_engine::physics::LinearVelocity,
            Has<Pogo>,
        ),
        With<Player>,
    >,
) {
    for (entity, mut attacking, mut vel, has_pogo) in query.iter_mut() {
        // Increment tick counter
        attacking.tick += 1;

        // Duration-agnostic: prefer max_ticks provided by the skill tables; fallback to default window
        let duration = attacking.max_ticks.unwrap_or_else(|| {
            let v = match attacking.variant {
                AttackVariant::Forward => SkillVariant::Forward,
                AttackVariant::Up => SkillVariant::Up,
                AttackVariant::Down => SkillVariant::Down,
            };
            let kind: SkillWeaponKind = attacking.weapon_type.into();
            attack_variant_metadata(kind, v)
                .active_window
                .duration_ticks
        });
        if attacking.tick >= duration {
            // If pogo is active, let pogo own velocity and finish; otherwise zero velocity on exit
            if !has_pogo {
                vel.0 = Vec2::ZERO;
            }
            transition_action(&mut commands, entity, Ready);
        }
    }
}

// Detect the first hit of an airborne downward attack and start pogo on the attacker
fn start_pogo_on_downward_attack_hit(
    mut commands: Commands,
    attack_query: Query<
        &crate::game::combat::DamageSource,
        (
            Added<crate::game::combat::Hit>,
            With<crate::game::combat::DownwardAttack>,
        ),
    >,
    mut player_q: Query<
        (
            &mut theseeker_engine::physics::LinearVelocity,
            Option<&mut crate::game::player::JumpCount>,
        ),
        With<Player>,
    >,
) {
    for attack in attack_query.iter() {
        if let Ok((mut vel, maybe_jump_count)) = player_q.get_mut(attack.owner)
        {
            // Begin pogo: set initial upward velocity and attach effect
            vel.0.y = POGO_VELOCITIES[0];
            commands.entity(attack.owner).insert(Pogo { tick: 0 });
            // Reset jump count to allow subsequent aerial control akin to a fresh jump
            if let Some(mut jc) = maybe_jump_count {
                jc.reset();
            }
        }
    }
}

// Apply pogo velocity curve and clean up
fn pogo_update_system(
    mut commands: Commands,
    mut q: Query<
        (
            Entity,
            &mut theseeker_engine::physics::LinearVelocity,
            &mut Pogo,
            Option<&mut Falling>,
            Option<&Jumping>,
            &CeilingSensor,
            &ActionState<PlayerAction>,
            &crate::game::player::PlayerStats,
            &crate::game::player::PlayerStatMod,
            &mut Facing,
            Has<crate::game::player::Dashing>,
            Has<crate::game::effects::stealthed::StealthEffect>,
        ),
        With<Player>,
    >,
    weapon: CurrentWeapon,
    autoaim: Res<BowAutoAimState>,
) {
    const AIR_SPEED_FACTOR: f32 = 1.0;
    for (
        entity,
        mut vel,
        mut pogo,
        maybe_falling,
        maybe_jumping,
        ceiling,
        action_state,
        stats,
        stat_mod,
        mut facing,
        is_dashing,
        _is_stealthed,
    ) in q.iter_mut()
    {
        // If we touch the ceiling during pogo, end pogo immediately and hand over to Falling
        if ceiling.is_touching_ceiling {
            vel.0.y = 0.0;
            if let Some(mut falling) = maybe_falling {
                falling.fall_ticks = 0;
                falling.wall_slide = None;
            } else if let Some(jumping) = maybe_jumping {
                transition_locomotion(
                    &mut commands,
                    entity,
                    Falling::from_jump(jumping.jump_count),
                );
            } else {
                transition_locomotion(
                    &mut commands,
                    entity,
                    Falling::from_jump(0),
                );
            }
            commands.entity(entity).remove::<Pogo>();
            continue;
        }
        let idx = pogo.tick.min(POGO_VELOCITIES.len() as u32 - 1) as usize;
        vel.0.y = POGO_VELOCITIES[idx];

        // Allow horizontal air control during pogo (same as jumping/falling), unless dashing
        if !is_dashing {
            let direction = action_state.clamped_value(&PlayerAction::Move);
            if direction != 0.0 {
                let base_speed =
                    stats.get(crate::game::player::StatType::MoveVelMax);
                vel.0.x =
                    direction * base_speed * AIR_SPEED_FACTOR * stat_mod.speed;
                // Update facing when not bow-decided
                if !weapon.has_bow_equipped()
                    || !autoaim.blocks_manual(*facing)
                {
                    if direction > 0.0 {
                        *facing = Facing::Right;
                    } else if direction < 0.0 {
                        *facing = Facing::Left;
                    }
                }
            } else {
                vel.0.x = 0.0;
            }
        }
        pogo.tick += 1;
        if pogo.tick >= POGO_VELOCITIES.len() as u32 {
            // Zero velocity on exit to hand control deterministically to locomotion
            vel.0.y = 0.0;
            // Reset fall curve so Falling resumes from its start (fixes terminal-velocity snap)
            if let Some(mut falling) = maybe_falling {
                falling.fall_ticks = 0;
                falling.wall_slide = None;
            } else if let Some(jumping) = maybe_jumping {
                // If we were jumping, hand over to Falling cleanly from jump context
                transition_locomotion(
                    &mut commands,
                    entity,
                    Falling::from_jump(jumping.jump_count),
                );
            } else {
                // Ensure we are in a known falling state if neither existed (safety)
                transition_locomotion(
                    &mut commands,
                    entity,
                    Falling::from_jump(0),
                );
            }
            commands.entity(entity).remove::<Pogo>();
        }
    }
}

// Calculate auto-aim for melee weapons - only changes variant, doesn't flip facing
fn calculate_melee_auto_aim(
    player_pos: Vec2,
    player_facing: &Facing,
    proximity_sensor: &EnemyProximitySensor,
    requested_variant: AttackVariant,
    enemy_query: &Query<&Transform, With<Enemy>>,
) -> AttackVariant {
    // If player requested up/down variant, always honor it
    if matches!(
        requested_variant,
        AttackVariant::Up | AttackVariant::Down
    ) {
        return requested_variant;
    }

    // Get enemies in different directions
    let forward_enemies = enemies_in_direction(
        &proximity_sensor.enemies_in_melee_range,
        &proximity_sensor.enemies_in_ranged_range,
        player_pos,
        player_facing.direction(),
        MELEE_AUTO_AIM_RANGE,
        enemy_query,
    );

    if !forward_enemies.is_empty() {
        return AttackVariant::Forward; // Attack normally
    }

    // No enemies horizontally - check vertically
    let above_enemies = enemies_above(
        &proximity_sensor.enemies_in_melee_range,
        &proximity_sensor.enemies_in_ranged_range,
        player_pos,
        MELEE_AUTO_AIM_RANGE,
        enemy_query,
    );

    if !above_enemies.is_empty() {
        return AttackVariant::Up;
    }

    let below_enemies = enemies_below(
        &proximity_sensor.enemies_in_melee_range,
        &proximity_sensor.enemies_in_ranged_range,
        player_pos,
        MELEE_AUTO_AIM_RANGE,
        enemy_query,
    );

    if !below_enemies.is_empty() {
        return AttackVariant::Down;
    }

    // No valid targets - attack forward anyway
    AttackVariant::Forward
}

// State lifetime is driven by the attack skill tables; no local animation-derived durations

// Helper functions for directional enemy detection
fn enemies_in_direction(
    melee_enemies: &Vec<Entity>,
    ranged_enemies: &Vec<Entity>,
    player_pos: Vec2,
    direction: f32,
    range: f32,
    enemy_query: &Query<&Transform, With<Enemy>>,
) -> Vec<Entity> {
    let enemies = if range <= MELEE_AUTO_AIM_RANGE {
        melee_enemies
    } else {
        ranged_enemies
    };

    enemies
        .iter()
        .filter(|&&enemy| {
            if let Ok(enemy_transform) = enemy_query.get(enemy) {
                let enemy_pos = enemy_transform.translation.truncate();
                let to_enemy = enemy_pos - player_pos;

                // Check if enemy is in the direction we're facing
                // direction is 1.0 for right, -1.0 for left
                to_enemy.x.signum() == direction
            } else {
                false
            }
        })
        .copied()
        .collect()
}

fn enemies_above(
    melee_enemies: &Vec<Entity>,
    ranged_enemies: &Vec<Entity>,
    player_pos: Vec2,
    range: f32,
    enemy_query: &Query<&Transform, With<Enemy>>,
) -> Vec<Entity> {
    let enemies = if range <= MELEE_AUTO_AIM_RANGE {
        melee_enemies
    } else {
        ranged_enemies
    };

    enemies
        .iter()
        .filter(|&&enemy| {
            if let Ok(enemy_transform) = enemy_query.get(enemy) {
                let enemy_pos = enemy_transform.translation.truncate();
                let to_enemy = enemy_pos - player_pos;

                // Check if enemy is above (+Y is up in this project)
                to_enemy.y > AUTO_AIM_VERTICAL_THRESHOLD
                    && to_enemy.x.abs() < AUTO_AIM_HORIZONTAL_LEEWAY
            } else {
                false
            }
        })
        .copied()
        .collect()
}

fn enemies_below(
    melee_enemies: &Vec<Entity>,
    ranged_enemies: &Vec<Entity>,
    player_pos: Vec2,
    range: f32,
    enemy_query: &Query<&Transform, With<Enemy>>,
) -> Vec<Entity> {
    let enemies = if range <= MELEE_AUTO_AIM_RANGE {
        melee_enemies
    } else {
        ranged_enemies
    };

    enemies
        .iter()
        .filter(|&&enemy| {
            if let Ok(enemy_transform) = enemy_query.get(enemy) {
                let enemy_pos = enemy_transform.translation.truncate();
                let to_enemy = enemy_pos - player_pos;

                // Check if enemy is below
                to_enemy.y < -AUTO_AIM_VERTICAL_THRESHOLD
                    && to_enemy.x.abs() < AUTO_AIM_HORIZONTAL_LEEWAY
            } else {
                false
            }
        })
        .copied()
        .collect()
}
