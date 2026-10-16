use crate::game::player::foreground::{ActionRef, ActionMut, ActionTag};
use bevy::prelude::*;
use rand::Rng;
use std::f32::consts::TAU;
use theseeker_engine::animation::PlaybackState;
use theseeker_engine::assets::animation::SpriteAnimation;
use theseeker_engine::gent::Gent;
use theseeker_engine::physics::LinearVelocity;
use theseeker_engine::script::ScriptPlayer;
use theseeker_engine::time::GameTickUpdate;

use crate::game::combat::damage_source::Backstab;
use crate::game::combat::{
    sparks::WeaponHitSlot, DamageSource, Health, Invulnerable,
    Stealthed,
};
use crate::game::enemy::Enemy;
use crate::game::gentstate::{Dead, Facing};
use crate::game::player::player_anim::set_direction_slots;
use crate::game::player::skills::flicker_strike::FlickerOutroTransition;
use crate::game::player::skills::types::{
    flicker_strike_metadata, FlickerStrikeMetadata,
};
use crate::game::player::spawns::amplified_bell::Bell;
use crate::game::player::states::{
    FlickerPhase, FlickerStriking, FlickerTarget,
    FlickerVariant, InAir,
};
use crate::game::player::weapon::PlayerMeleeWeapon;
use crate::game::player::FlickerAbility;
use crate::game::player::{Player, PlayerStatMod, PlayerStateSet};

const PER_HIT_DETOUR_MIN_DISTANCE: f32 = 10.0;
const PER_HIT_DETOUR_MAX_DISTANCE: f32 = 20.0;
const FLICKER_EVENT_INTRO_DONE: &str = "flicker_intro_done";
const FLICKER_EVENT_DAMAGE_DONE: &str = "flicker_damage_done";

type FlickerEnemyQueryFilter = (
    Or<(With<Enemy>, With<Bell>)>,
    Without<Dead>,
    Without<Player>,
);
type FlickerCollisionEnemyQueryFilter = (
    Or<(With<Enemy>, With<Bell>)>,
    Without<Dead>,
    Without<crate::game::player::Player>,
);
type FlickerTargetCandidate = (Entity, Vec2, f32);

struct FlickerTargetCandidates {
    vulnerable: Vec<FlickerTargetCandidate>,
    invulnerable: Vec<FlickerTargetCandidate>,
    alive_enemies_in_range: usize,
}

struct FlickerCollisionContext<'a, 'w, 's> {
    commands: &'a mut Commands<'w, 's>,
    player_entity: Entity,
    player_transform: &'a Transform,
    flicker_ability: &'a mut FlickerAbility,
    player_stat_mod: Option<&'a PlayerStatMod>,
    is_stealthed: bool,
    damage_entity: Option<Entity>,
}

pub struct FlickerStrikingStatePlugin;

impl Plugin for FlickerStrikingStatePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            GameTickUpdate,
            (
                flicker_striking_enter_system,
                flicker_striking_phase_system,
                flicker_striking_targeting_system,
                flicker_striking_movement_system,
                flicker_striking_collision_system,
                flicker_striking_animation_system,
                flicker_striking_outro_system,
            )
                .chain()
                .in_set(PlayerStateSet::Behavior)
                .in_set(crate::game::player::PlayerBehaviorSet::Execution),
        );
    }
}

fn flicker_striking_enter_system(
    mut query: Query<
        (Entity, ActionMut<FlickerStriking>, &Gent),
        (With<Player>, Added<ActionTag<FlickerStriking>>),
    >,
    mut gfx_query: Query<&mut ScriptPlayer<SpriteAnimation>>,
) {
    for (entity, mut flickering, gent) in query.iter_mut() {
        trace!(
            "ENTER: Entity {:?} starting FlickerStrike with weapon {:?}",
            entity,
            flickering.weapon
        );

        // Start intro animation
        if let Ok(mut script_player) = gfx_query.get_mut(gent.e_gfx) {
            let anim_key =
                animation_key(flickering.weapon, flickering.current_variant);
            flickering.execution_mut().play(&mut script_player, &anim_key);
        }
    }
}

fn flicker_striking_phase_system(
    mut commands: Commands,
    mut query: Query<(Entity, ActionMut<FlickerStriking>, &Gent), With<Player>>,
    enemy_query: Query<
        (
            Entity,
            &Transform,
            &Health,
            Has<Invulnerable>,
        ),
        FlickerEnemyQueryFilter,
    >,
    signal_query: Query<&PlaybackState>,
) {
    for (entity, mut flickering, gent) in query.iter_mut() {
        flickering.tick = flickering.tick.saturating_add(1);
        let animation_signals = flickering.execution().playback(signal_query.get(gent.e_gfx).ok());

        validate_current_flicker_target(&mut flickering, &enemy_query);
        let execution = flickering.execution().id;
        update_flicker_phase(
            &mut commands,
            entity,
            &mut flickering,
            animation_signals,
            execution,
        );
    }
}

fn validate_current_flicker_target(
    flickering: &mut FlickerStriking,
    enemy_query: &Query<
        (Entity, &Transform, &Health, Has<Invulnerable>),
        FlickerEnemyQueryFilter,
    >,
) {
    let Some(target) = flickering.current_target else {
        return;
    };
    if flicker_target_still_alive(target, enemy_query) {
        return;
    }

    flickering.current_target = None;
    flickering.damage_applied = false;
}

fn flicker_target_still_alive(
    target: FlickerTarget,
    enemy_query: &Query<
        (Entity, &Transform, &Health, Has<Invulnerable>),
        FlickerEnemyQueryFilter,
    >,
) -> bool {
    match target {
        FlickerTarget::Enemy(target_entity) => {
            flicker_target_entity_alive(target_entity, enemy_query)
        },
        FlickerTarget::Detour { return_to, .. } => {
            flicker_target_entity_alive(
                return_to,
                enemy_query,
            )
        },
    }
}

fn flicker_target_entity_alive(
    target_entity: Entity,
    enemy_query: &Query<
        (Entity, &Transform, &Health, Has<Invulnerable>),
        FlickerEnemyQueryFilter,
    >,
) -> bool {
    let Ok((_entity, _transform, health, _is_invulnerable)) =
        enemy_query.get(target_entity)
    else {
        return false;
    };
    health.current != 0
}

fn update_flicker_phase(
    commands: &mut Commands,
    entity: Entity,
    flickering: &mut FlickerStriking,
    animation_signals: Option<&PlaybackState>,
    execution: u64,
) {
    match flickering.phase {
        FlickerPhase::Intro => {
            update_flicker_intro_phase(flickering, animation_signals);
        },
        FlickerPhase::Dashing => {
        },
        FlickerPhase::Damage => {
            update_flicker_damage_phase(flickering, animation_signals);
        },
        FlickerPhase::Outro => {
            update_flicker_outro_phase(commands, entity, animation_signals, execution);
        },
    }
}

fn update_flicker_intro_phase(
    flickering: &mut FlickerStriking,
    animation_signals: Option<&PlaybackState>,
) {
    if !animation_has_event(animation_signals, FLICKER_EVENT_INTRO_DONE) {
        return;
    }

    flickering.phase = FlickerPhase::Dashing;
    flickering.current_target = None;
}

fn update_flicker_damage_phase(
    flickering: &mut FlickerStriking,
    animation_signals: Option<&PlaybackState>,
) {
    if !animation_has_event(animation_signals, FLICKER_EVENT_DAMAGE_DONE) {
        return;
    }

    complete_flicker_damage_phase(flickering);
}

fn complete_flicker_damage_phase(flickering: &mut FlickerStriking) {
    if flickering.pending_outro {
        flickering.phase = FlickerPhase::Outro;
        flickering.current_target = None;
        flickering.damage_applied = false;
        flickering.pending_outro = false;
        return;
    }

    flickering.phase = FlickerPhase::Dashing;
    flickering.current_target = None;
    flickering.damage_applied = false;
}

fn update_flicker_outro_phase(
    commands: &mut Commands,
    entity: Entity,
    animation_signals: Option<&PlaybackState>,
    execution: u64,
) {
    if !animation_finished(animation_signals) {
        return;
    }

    crate::game::player::foreground::finish_action(commands, entity, execution);
}

fn animation_has_event(
    animation_signals: Option<&PlaybackState>,
    event: &str,
) -> bool {
    animation_signals.is_some_and(|signals| signals.has_event(event))
}

fn animation_finished(animation_signals: Option<&PlaybackState>) -> bool {
    animation_signals.is_some_and(|signals| signals.finished)
}

fn flicker_striking_targeting_system(
    mut commands: Commands,
    mut query: Query<
        (
            Entity,
            ActionMut<FlickerStriking>,
            &Transform,
            &FlickerAbility,
        ),
        With<Player>,
    >,
    enemy_query: Query<
        (
            Entity,
            &Transform,
            &Health,
            Has<Invulnerable>,
        ),
        (
            Or<(With<Enemy>, With<Bell>)>,
            Without<Dead>,
            Without<Player>,
        ),
    >,
    has_outro: Query<&FlickerOutroTransition>,
) {
    for (entity, mut flickering, player_transform, flicker_ability) in
        query.iter_mut()
    {
        let meta = flicker_strike_metadata();
        let player_pos = player_transform.translation.truncate();

        if handle_flicker_outro_request(
            &mut commands,
            entity,
            &mut flickering,
            &has_outro,
        ) {
            continue;
        }

        // Do not process pending_outro here before first dash; it will be
        // honored after a dash completes (in Damage), ensuring at least
        // intro + one dash occurs even on a quick tap.

        // Only target during dashing phase
        if !matches!(flickering.phase, FlickerPhase::Dashing) {
            continue;
        }

        if matches!(flickering.current_target, Some(FlickerTarget::Enemy(_))) {
            continue;
        }

        let candidates =
            collect_flicker_target_candidates(&enemy_query, player_pos, meta);

        if handle_active_detour_target(&mut flickering, &candidates) {
            continue;
        }

        // Energy gate per dash: require enough for a chunk before initiating a new dash
        if flicker_ability.energy < meta.chunk_cost {
            flickering.phase = FlickerPhase::Outro;
            flickering.current_target = None;
            continue;
        }

        let have_vulnerable_now = !candidates.vulnerable.is_empty();
        let alive_enemies_in_range = candidates.alive_enemies_in_range;
        let valid_targets = valid_flicker_targets(candidates, &flickering);

        if valid_targets.is_empty() {
            handle_empty_flicker_targets(
                &mut flickering,
                alive_enemies_in_range,
            );
            continue;
        }

        if flickering.invulnerable_only_mode && !have_vulnerable_now {
            flickering.phase = FlickerPhase::Outro;
            flickering.current_target = None;
            flickering.invulnerable_only_mode = false;
            continue;
        }

        let (target_entity, target_pos, target_index) =
            select_flicker_target(&valid_targets, &flickering);

        trace!(
            "TARGETING: Selected target {:?} at index {}",
            target_entity,
            target_index
        );

        apply_selected_flicker_target(
            &mut flickering,
            target_entity,
            target_pos,
            player_pos,
            meta,
        );

        flickering.invulnerable_only_mode = !have_vulnerable_now;

    }
}

fn handle_flicker_outro_request(
    commands: &mut Commands,
    entity: Entity,
    flickering: &mut FlickerStriking,
    has_outro: &Query<&FlickerOutroTransition>,
) -> bool {
    if has_outro.single().is_err()
        || matches!(flickering.phase, FlickerPhase::Outro)
    {
        return false;
    }

    commands.entity(entity).remove::<FlickerOutroTransition>();

    if matches!(flickering.phase, FlickerPhase::Dashing)
        && flickering.current_target.is_some()
    {
        flickering.pending_outro = true;
        return false;
    }

    if matches!(flickering.phase, FlickerPhase::Intro) {
        flickering.pending_outro = true;
        return false;
    }

    flickering.phase = FlickerPhase::Outro;
    flickering.current_target = None;
    true
}

fn collect_flicker_target_candidates(
    enemy_query: &Query<
        (Entity, &Transform, &Health, Has<Invulnerable>),
        FlickerEnemyQueryFilter,
    >,
    player_pos: Vec2,
    meta: &FlickerStrikeMetadata,
) -> FlickerTargetCandidates {
    let mut candidates = FlickerTargetCandidates {
        vulnerable: Vec::new(),
        invulnerable: Vec::new(),
        alive_enemies_in_range: 0,
    };

    for (enemy_entity, enemy_transform, health, is_invulnerable) in
        enemy_query.iter()
    {
        record_flicker_candidate(
            &mut candidates,
            enemy_entity,
            enemy_transform,
            health,
            is_invulnerable,
            player_pos,
            meta.range,
        );
    }

    candidates
}

fn record_flicker_candidate(
    candidates: &mut FlickerTargetCandidates,
    enemy_entity: Entity,
    enemy_transform: &Transform,
    health: &Health,
    is_invulnerable: bool,
    player_pos: Vec2,
    range: f32,
) {
    if health.current == 0 {
        return;
    }

    let enemy_pos = enemy_transform.translation.truncate();
    let distance = player_pos.distance(enemy_pos);

    if distance > range {
        return;
    }

    candidates.alive_enemies_in_range += 1;
    let candidate = (enemy_entity, enemy_pos, distance);
    if is_invulnerable {
        candidates.invulnerable.push(candidate);
        return;
    }

    candidates.vulnerable.push(candidate);
}

fn handle_active_detour_target(
    flickering: &mut FlickerStriking,
    candidates: &FlickerTargetCandidates,
) -> bool {
    let Some(FlickerTarget::Detour { return_to, .. }) =
        flickering.current_target
    else {
        return false;
    };

    if candidates.alive_enemies_in_range == 0 {
        flickering.phase = FlickerPhase::Outro;
        flickering.current_target = None;
        return true;
    }

    if !flicker_return_target_in_range(return_to, candidates) {
        flickering.current_target = None;
        return false;
    }

    true
}

fn flicker_return_target_in_range(
    return_to: Entity,
    candidates: &FlickerTargetCandidates,
) -> bool {
    candidates
        .vulnerable
        .iter()
        .chain(candidates.invulnerable.iter())
        .any(|(entity, _, _)| *entity == return_to)
}

fn valid_flicker_targets(
    candidates: FlickerTargetCandidates,
    flickering: &FlickerStriking,
) -> Vec<FlickerTargetCandidate> {
    if candidates.vulnerable.is_empty() {
        return candidates.invulnerable;
    }

    if candidates.vulnerable.len() <= 1 {
        return candidates.vulnerable;
    }

    candidates
        .vulnerable
        .into_iter()
        .filter(|(entity, _, _)| !flickering.damaged_entities.contains(entity))
        .collect()
}

fn handle_empty_flicker_targets(
    flickering: &mut FlickerStriking,
    alive_enemies_in_range: usize,
) {

    if alive_enemies_in_range == 0 {
        flickering.phase = FlickerPhase::Outro;
        flickering.current_target = None;
        return;
    }

    flickering.damaged_entities.clear();
}

fn select_flicker_target(
    valid_targets: &[FlickerTargetCandidate],
    flickering: &FlickerStriking,
) -> (Entity, Vec2, usize) {
    if flickering.last_target.is_none() {
        return choose_nearest_flicker_target(valid_targets, flickering.tick);
    }

    choose_followup_flicker_target(
        valid_targets,
        flickering.tick,
        flickering.last_target,
    )
}

fn choose_nearest_flicker_target(
    valid_targets: &[FlickerTargetCandidate],
    tick: u32,
) -> (Entity, Vec2, usize) {
    let min_dist = valid_targets
        .iter()
        .map(|target| target.2)
        .fold(f32::INFINITY, f32::min);
    let ties: Vec<(usize, FlickerTargetCandidate)> = valid_targets
        .iter()
        .copied()
        .enumerate()
        .filter(|(_, (_entity, _pos, distance))| *distance == min_dist)
        .collect();
    let choice_index = if ties.len() == 1 {
        0
    } else {
        (tick as usize) % ties.len()
    };
    let (target_index, (entity, pos, _distance)) = ties[choice_index];
    (entity, pos, target_index)
}

fn choose_followup_flicker_target(
    valid_targets: &[FlickerTargetCandidate],
    tick: u32,
    last_target: Option<Entity>,
) -> (Entity, Vec2, usize) {
    let mut target_index = (tick as usize) % valid_targets.len();
    let Some(prev) = last_target else {
        let (entity, pos, _dist) = valid_targets[target_index];
        return (entity, pos, target_index);
    };

    if valid_targets.len() > 1 && valid_targets[target_index].0 == prev {
        target_index = (target_index + 1) % valid_targets.len();
    }

    let (entity, pos, _dist) = valid_targets[target_index];
    (entity, pos, target_index)
}

fn apply_selected_flicker_target(
    flickering: &mut FlickerStriking,
    target_entity: Entity,
    target_pos: Vec2,
    player_pos: Vec2,
    meta: &FlickerStrikeMetadata,
) {
    if flickering.last_target.is_some() {
        apply_flicker_detour_target(
            flickering,
            target_entity,
            player_pos,
            meta.range,
        );
        return;
    }

    apply_direct_flicker_target(
        flickering,
        target_entity,
        target_pos,
        player_pos,
    );
}

fn apply_flicker_detour_target(
    flickering: &mut FlickerStriking,
    target_entity: Entity,
    player_pos: Vec2,
    range: f32,
) {
    let max_distance = PER_HIT_DETOUR_MAX_DISTANCE.min(range);
    let min_distance = PER_HIT_DETOUR_MIN_DISTANCE.min(max_distance * 0.75);
    let mut rng = rand::rng();
    let detour_angle = rng.random_range(0.0..TAU);
    let detour_distance =
        rng.random_range(min_distance..=max_distance.max(1.0));
    let detour_offset =
        Vec2::new(detour_angle.cos(), detour_angle.sin()) * detour_distance;
    let detour_target = player_pos + detour_offset;

    let detour_direction = detour_target - player_pos;
    flickering.current_variant = select_variant_from_angle(
        detour_direction.y.atan2(detour_direction.x),
    );
    flickering.last_target = Some(target_entity);
    flickering.current_target = Some(FlickerTarget::Detour {
        position: detour_target,
        return_to: target_entity,
    });
}

fn apply_direct_flicker_target(
    flickering: &mut FlickerStriking,
    target_entity: Entity,
    target_pos: Vec2,
    player_pos: Vec2,
) {
    let direction = target_pos - player_pos;
    let angle = direction.y.atan2(direction.x);
    flickering.current_variant = select_variant_from_angle(angle);
    flickering.last_target = Some(target_entity);
    flickering.current_target = Some(FlickerTarget::Enemy(target_entity));

}

fn flicker_striking_movement_system(
    mut query: Query<
        (
            &mut LinearVelocity,
            &mut Transform,
            &mut Facing,
            ActionRef<FlickerStriking>,
        ),
        With<Player>,
    >,
    enemy_query: Query<
        &Transform,
        (
            Or<(With<Enemy>, With<Bell>)>,
            Without<Dead>,
            Without<Player>,
        ),
    >,
) {
    const DASH_SPEED: f32 = 16.0; // Pixels per tick - reduced for more precise collision detection

    for (mut velocity, player_transform, mut facing, flickering) in
        query.iter_mut()
    {
        // Only move during dashing phase with a target
        if !matches!(flickering.phase, FlickerPhase::Dashing) {
            velocity.0 = Vec2::ZERO;
            continue;
        }

        let player_pos = player_transform.translation.truncate();
        let Some(target_pos) =
            flicker_movement_target(&flickering, &enemy_query)
        else {
            velocity.0 = Vec2::ZERO;
            continue;
        };

        let direction = (target_pos - player_pos).normalize_or_zero();

        // Set velocity toward target
        let distance_to_target = player_pos.distance(target_pos);

        if distance_to_target < DASH_SPEED {
            // Adjust velocity to reach exactly
            velocity.0 = direction * distance_to_target;
        } else {
            velocity.0 = direction * DASH_SPEED;
        }

        // Update facing direction
        if direction.x > 0.0 {
            *facing = Facing::Right;
        } else if direction.x < 0.0 {
            *facing = Facing::Left;
        }
    }
}

fn flicker_movement_target(
    flickering: &FlickerStriking,
    enemy_query: &Query<&Transform, FlickerEnemyQueryFilter>,
) -> Option<Vec2> {
    match flickering.current_target {
        Some(FlickerTarget::Enemy(target_entity)) => {
            flicker_enemy_movement_target(target_entity, enemy_query)
        },
        Some(FlickerTarget::Detour { position, .. }) => Some(position),
        None => {
            None
        },
    }
}

fn flicker_enemy_movement_target(
    target_entity: Entity,
    enemy_query: &Query<&Transform, FlickerEnemyQueryFilter>,
) -> Option<Vec2> {
    let Ok(target_transform) = enemy_query.get(target_entity) else {
        return None;
    };

    Some(target_transform.translation.truncate())
}

fn flicker_striking_collision_system(
    mut commands: Commands,
    mut query: Query<
        (
            Entity,
            ActionMut<FlickerStriking>,
            &Transform,
            &mut FlickerAbility,
            Option<&PlayerStatMod>,
            Has<crate::game::effects::stealthed::StealthEffect>,
        ),
        With<Player>,
    >,
    enemy_query: Query<
        (
            Entity,
            &Transform,
            &crate::game::gentstate::Facing,
        ),
        (
            Or<(With<Enemy>, With<Bell>)>,
            Without<Dead>,
            Without<crate::game::player::Player>,
        ),
    >,
) {
    const COLLISION_DISTANCE: f32 = 16.0; // Pixels - increased for reliable collision detection

    for (
        player_entity,
        mut flickering,
        player_transform,
        mut flicker_ability,
        player_stat_mod,
        is_stealthed,
    ) in query.iter_mut()
    {

        // Only check during dashing phase with a target
        if !matches!(flickering.phase, FlickerPhase::Dashing) {
            continue;
        }

        let player_pos = player_transform.translation.truncate();
        let mut context = FlickerCollisionContext {
            commands: &mut commands,
            player_entity,
            player_transform,
            flicker_ability: &mut flicker_ability,
            player_stat_mod,
            is_stealthed,
            damage_entity: None,
        };
        handle_flicker_collision_target(
            &mut context,
            &mut flickering,
            &enemy_query,
            player_pos,
            COLLISION_DISTANCE,
        );
        if let Some(hitbox) = context.damage_entity {
            flickering.execution_mut().own(hitbox);
        }
    }
}

fn handle_flicker_collision_target(
    context: &mut FlickerCollisionContext,
    flickering: &mut FlickerStriking,
    enemy_query: &Query<
        (Entity, &Transform, &crate::game::gentstate::Facing),
        FlickerCollisionEnemyQueryFilter,
    >,
    player_pos: Vec2,
    collision_distance: f32,
) {
    match flickering.current_target {
        Some(FlickerTarget::Detour {
            position,
            return_to,
        }) => handle_flicker_detour_collision(
            flickering,
            enemy_query,
            player_pos,
            position,
            return_to,
            collision_distance,
        ),
        Some(FlickerTarget::Enemy(target_entity)) => handle_flicker_enemy_collision(
            context,
            flickering,
            enemy_query,
            player_pos,
            target_entity,
            collision_distance,
        ),
        None => {
        },
    }
}

fn handle_flicker_detour_collision(
    flickering: &mut FlickerStriking,
    enemy_query: &Query<
        (Entity, &Transform, &crate::game::gentstate::Facing),
        FlickerCollisionEnemyQueryFilter,
    >,
    player_pos: Vec2,
    position: Vec2,
    return_to: Entity,
    collision_distance: f32,
) {
    let distance = player_pos.distance(position);

    if distance > collision_distance {
        return;
    }

    let Ok((_entity, return_transform, _facing)) = enemy_query.get(return_to)
    else {
        flickering.current_target = None;
        return;
    };

    let direction = return_transform.translation.truncate() - player_pos;
    flickering.current_variant =
        select_variant_from_angle(direction.y.atan2(direction.x));
    flickering.current_target = Some(FlickerTarget::Enemy(return_to));
}

fn handle_flicker_enemy_collision(
    context: &mut FlickerCollisionContext,
    flickering: &mut FlickerStriking,
    enemy_query: &Query<
        (Entity, &Transform, &crate::game::gentstate::Facing),
        FlickerCollisionEnemyQueryFilter,
    >,
    player_pos: Vec2,
    target_entity: Entity,
    collision_distance: f32,
) {
    let Ok((enemy_entity, enemy_transform, enemy_facing)) =
        enemy_query.get(target_entity)
    else {
        return;
    };

    let enemy_pos = enemy_transform.translation.truncate();
    let distance = player_pos.distance(enemy_pos);

    if distance > collision_distance {
        return;
    }

    trace!("COLLISION: Hit detected! Transitioning to damage phase");
    flickering.phase = FlickerPhase::Damage;

    if flickering.damage_applied {
        return;
    }

    apply_flicker_collision_damage(
        context,
        flickering,
        enemy_entity,
        enemy_transform,
        enemy_facing,
    );
}

fn apply_flicker_collision_damage(
    context: &mut FlickerCollisionContext,
    flickering: &mut FlickerStriking,
    enemy_entity: Entity,
    enemy_transform: &Transform,
    enemy_facing: &crate::game::gentstate::Facing,
) {
    debit_flicker_energy(context.flicker_ability);
    let attack = flickering.weapon.attack();
    let damage = attack.damage * context.player_stat_mod.map_or(1.0, |stat_mod| stat_mod.damage);
    let is_backstab =
        flicker_hit_is_backstab(enemy_transform, context.player_transform, enemy_facing);

    let damage_source_entity = spawn_flicker_damage_source(
        context.commands,
        context.player_entity,
        damage,
        attack.hit_slot,
        enemy_transform,
    );
    context.damage_entity = Some(damage_source_entity);
    apply_flicker_damage_markers(
        context.commands,
        damage_source_entity,
        is_backstab,
        context.is_stealthed,
    );

    flickering.damaged_entities.insert(enemy_entity);
    flickering.damage_applied = true;

}

fn debit_flicker_energy(flicker_ability: &mut FlickerAbility) {
    let before = flicker_ability.energy;
    let after = (before - flicker_strike_metadata().chunk_cost).max(0.0);
    flicker_ability.energy = after;
}

fn flicker_hit_is_backstab(
    enemy_transform: &Transform,
    player_transform: &Transform,
    enemy_facing: &crate::game::gentstate::Facing,
) -> bool {
    match *enemy_facing {
        crate::game::gentstate::Facing::Left => {
            enemy_transform.translation.x < player_transform.translation.x
        },
        crate::game::gentstate::Facing::Right => {
            enemy_transform.translation.x > player_transform.translation.x
        },
    }
}

fn spawn_flicker_damage_source(
    commands: &mut Commands,
    player_entity: Entity,
    damage: f32,
    weapon_slot: &str,
    enemy_transform: &Transform,
) -> Entity {
    commands
        .spawn((
            // Give a few ticks of lifetime to ensure the combat pipeline
            // sees this entity regardless of system ordering on the current frame.
            DamageSource::new(3, player_entity, damage).with_max_targets(1),
            WeaponHitSlot {
                slot_name: weapon_slot.to_string(),
            },
            Transform::from_translation(enemy_transform.translation),
            GlobalTransform::default(),
            theseeker_engine::physics::Collider::cuboid(8.0, 8.0),
            theseeker_engine::physics::groups::player_attack(),
        ))
        .id()
}

fn apply_flicker_damage_markers(
    commands: &mut Commands,
    damage_source_entity: Entity,
    is_backstab: bool,
    is_stealthed: bool,
) {
    if is_backstab {
        commands.entity(damage_source_entity).insert(Backstab);
    }

    if is_stealthed {
        commands.entity(damage_source_entity).insert(Stealthed);
    }
}

fn flicker_striking_animation_system(
    mut query: Query<(ActionMut<FlickerStriking>, &Gent, &Facing), With<Player>>,
    mut gfx_query: Query<&mut ScriptPlayer<SpriteAnimation>>,
) {
    for (mut flickering, gent, facing) in query.iter_mut() {
        let Ok(mut script_player) = gfx_query.get_mut(gent.e_gfx) else {
            continue;
        };

        let id = flickering.execution().id;
        update_flicker_animation(&mut script_player, &flickering, facing, id);
        flickering.execution_mut().playback = theseeker_engine::animation::PlaybackId(script_player.execution_id());
    }
}

fn update_flicker_animation(
    script_player: &mut ScriptPlayer<SpriteAnimation>,
    flickering: &FlickerStriking,
    facing: &Facing,
    execution: u64,
) {

    set_direction_slots(script_player, facing);
    let anim_key = animation_key(flickering.weapon, flickering.current_variant);
    set_flicker_phase_slots(script_player, flickering.phase, execution);
    play_flicker_animation_key(script_player, &anim_key, execution);
}

fn set_flicker_phase_slots(
    script_player: &mut ScriptPlayer<SpriteAnimation>,
    phase: FlickerPhase,
    execution: u64,
) {
    match phase {
        FlickerPhase::Intro => {
            set_flicker_animation_slots(script_player, true, false, false, false, execution);
        },
        FlickerPhase::Dashing => {
            set_flicker_animation_slots(script_player, false, true, false, false, execution);
        },
        FlickerPhase::Damage => {
            set_flicker_animation_slots(script_player, false, false, true, false, execution);
        },
        FlickerPhase::Outro => {
            set_flicker_animation_slots(script_player, false, false, false, true, execution);
        },
    }
}

fn set_flicker_animation_slots(
    script_player: &mut ScriptPlayer<SpriteAnimation>,
    intro: bool,
    dashing: bool,
    damage: bool,
    outro: bool,
    execution: u64,
) {
    script_player.set_presentation_slot(execution, "FlickerIntro", intro);
    script_player.set_presentation_slot(execution, "FlickerDashing", dashing);
    script_player.set_presentation_slot(execution, "FlickerDamage", damage);
    script_player.set_presentation_slot(execution, "FlickerOutro", outro);
}

fn play_flicker_animation_key(
    script_player: &mut ScriptPlayer<SpriteAnimation>,
    anim_key: &str,
    execution: u64,
) {
    if script_player.current_key() == Some(anim_key) {
        return;
    }

    script_player.acquire_presentation(execution);
    script_player.play_key(anim_key);
}

fn flicker_striking_outro_system(
    mut commands: Commands,
    mut query: Query<(Entity, ActionMut<FlickerStriking>), With<Player>>,
) {
    for (entity, mut flickering) in query.iter_mut() {
        if matches!(flickering.phase, FlickerPhase::Outro) {
            let execution = flickering.execution_mut();
            execution.controls.motion_x = false;
            execution.controls.motion_y = false;
            commands.entity(entity).remove::<InAir>();
        }
    }
}

fn select_variant_from_angle(angle: f32) -> FlickerVariant {
    let degrees = angle.to_degrees();

    // Normalize to 0-360
    let normalized = if degrees < 0.0 {
        degrees + 360.0
    } else {
        degrees
    };

    match normalized {
        337.5..=360.0 | 0.0..=22.5 => FlickerVariant::Forward,
        22.5..=67.5 => FlickerVariant::FrontUpward,
        67.5..=112.5 => FlickerVariant::Upward,
        112.5..=157.5 => FlickerVariant::FrontUpward, // Will be flipped
        157.5..=202.5 => FlickerVariant::Forward,     // Will be flipped
        202.5..=247.5 => FlickerVariant::FrontDownward, // Will be flipped
        247.5..=292.5 => FlickerVariant::Downward,
        292.5..=337.5 => FlickerVariant::FrontDownward,
        _ => FlickerVariant::Forward,
    }
}

fn animation_key(weapon: PlayerMeleeWeapon, variant: FlickerVariant) -> String {
    let variant = match variant {
        FlickerVariant::Forward => "Forward",
        FlickerVariant::Upward => "Upward",
        FlickerVariant::Downward => "Downward",
        FlickerVariant::FrontUpward => "FrontUpward",
        FlickerVariant::FrontDownward => "FrontDownward",
    };
    format!("anim.player.{}Flicker{variant}", weapon.name())
}
