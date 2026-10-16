use bevy::prelude::*;
use leafwing_input_manager::action_state::ActionState;
use theseeker_engine::assets::animation::SpriteAnimation;
use theseeker_engine::gent::Gent;
use theseeker_engine::physics::{Collider, LinearVelocity, TileCollisionWorld};
use theseeker_engine::script::ScriptPlayer;
use theseeker_engine::time::GameTime;

use crate::game::{
    gentstate::Facing,
    player::{
        InputBuffer, Player, PlayerAction, PlayerGfx,
        PlayerStatMod, PlayerStats,
    },
};

use crate::game::player::skills::portable_tent::{
    collision_space, TentInteriorAssets, TentInteriorOccupant,
};
use crate::game::world_objects::{
    capsule_is_solid_at_world_point, Capsule, CapsuleCollisionMask,
};
use crate::game::player::sensors::{GroundSensor, WallSensor};
use crate::game::player::states::skill::attacking::Pogo;
use crate::game::player::states::WALL_PUSHBACK;
use crate::game::player::states::WALL_PUSHBACK_TICKS;
use crate::game::player::states::{
    transition_locomotion, Falling, HorizontalInputIntent, Idle, InAir,
    Jumping, Running, WallSide,
};

pub struct FallingStatePlugin;

impl Plugin for FallingStatePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            crate::GameTickUpdate,
            (
                falling_enter_system,
                falling_update_system,
            )
                .chain()
                .run_if(any_with_component::<Falling>)
                .in_set(crate::game::player::PlayerBehaviorSet::Locomotion),
        );
    }
}

// Coyote time window (ticks @ 96 Hz)
const COYOTE_TIME_TICKS: u32 = 10;

// Horizontal air-control multiplier (applied to `MoveVelMax` each tick)
const AIR_SPEED_FACTOR: f32 = 1.0; // Multiplier for air speed (can be tweaked for different feel)

// Wall slide uses the early portion of the fall curve only
// (limit to first 14 entries; 0-based index 13)
const WALL_SLIDE_MAX_INDEX: u32 = 13;
const WALL_SLIDE_TOP_SAMPLE_EPS: f32 = 0.01;
const WALL_SLIDE_SIDE_SAMPLE_EPS: f32 = 0.01;

fn falling_enter_system(
    query: Query<
        (Entity, &Gent, &Falling),
        Added<Falling>,
    >,
    mut gfx_query: Query<&mut ScriptPlayer<SpriteAnimation>, With<PlayerGfx>>,
    mut commands: Commands,
) {
    for (entity, gent, falling) in query.iter() {
        commands.entity(entity).insert(InAir);
        play_falling_enter_animation(gent, falling, &mut gfx_query);
    }


}

fn play_falling_enter_animation(
    gent: &Gent,
    falling: &Falling,
    gfx_query: &mut Query<&mut ScriptPlayer<SpriteAnimation>, With<PlayerGfx>>,
) {
    let Ok(mut player) = gfx_query.get_mut(gent.e_gfx) else {
        return;
    };

    play_falling_animation(&mut player, falling.wall_slide);
}

fn play_falling_animation(
    player: &mut ScriptPlayer<SpriteAnimation>,
    wall_slide: Option<WallSide>,
) {
    let Some(wall_side) = wall_slide else {
        // Entering normal fall (not wall sliding).
        player.request_background("anim.player.Fall");
        return;
    };

    player.request_background("anim.player.WallSlide");
    crate::game::player::player_anim::set_direction_slots(
        player,
        &wall_slide_visual_facing(wall_side),
    );
}

fn wall_slide_visual_facing(wall_side: WallSide) -> Facing {
    match wall_side {
        WallSide::Left => Facing::Left,
        WallSide::Right => Facing::Right,
    }
}

fn wall_slide_side_from_input(
    move_input: f32,
    wall_sensor: &WallSensor,
) -> WallSide {
    if move_input < 0.0 && wall_sensor.left_contact {
        return WallSide::Left;
    }

    WallSide::Right
}

fn play_wall_slide_for_entity(
    entity: Entity,
    wall_side: WallSide,
    gent_query: &Query<&Gent, With<Player>>,
    gfx_query: &mut Query<&mut ScriptPlayer<SpriteAnimation>, With<PlayerGfx>>,
) {
    let Ok(gent) = gent_query.get(entity) else {
        return;
    };
    let Ok(mut player) = gfx_query.get_mut(gent.e_gfx) else {
        return;
    };

    play_falling_animation(&mut player, Some(wall_side));
}

fn play_fall_for_entity(
    entity: Entity,
    vertical_velocity: f32,
    gent_query: &Query<&Gent, With<Player>>,
    gfx_query: &mut Query<&mut ScriptPlayer<SpriteAnimation>, With<PlayerGfx>>,
) {
    if vertical_velocity >= 0.0 {
        return;
    }
    let Ok(gent) = gent_query.get(entity) else {
        return;
    };
    let Ok(mut player) = gfx_query.get_mut(gent.e_gfx) else {
        return;
    };

    player.request_background("anim.player.Fall");
}

fn transition_grounded_falling(
    commands: &mut Commands,
    entity: Entity,
    input_x: f32,
) {
    if input_x.abs() > 0.1 {
        transition_locomotion(commands, entity, Running);
        return;
    }

    transition_locomotion(commands, entity, Idle);
}

#[expect(clippy::too_many_arguments, reason = "Bevy system dependencies are expressed as explicit system parameters.")]
#[expect(
    clippy::too_many_lines,
    reason = "This player locomotion system advances one falling tick across input, collision, transitions, and skill interrupts."
)]
fn falling_update_system(
    mut query: Query<
        (
            Entity,
            &mut LinearVelocity,
            &mut Falling,
            &ActionState<PlayerAction>,
            &HorizontalInputIntent,
            &GroundSensor,
            &PlayerStats,
            &PlayerStatMod,
            &mut Facing,
            &Transform,
            &WallSensor,
            &Collider,
            &mut InputBuffer,
        ),
        (With<Player>, Without<crate::game::spikes::SpikeRespawnPending>),
    >,
    locomotion_flags_q: Query<
        (
            &crate::game::player::foreground::ForegroundAction,
            Has<Pogo>,
        ),
        With<Player>,
    >,
    gent_query: Query<&Gent, With<Player>>,
    mut commands: Commands,
    time: Res<GameTime>,
    collision_world: Option<Res<TileCollisionWorld>>,
    tent_interior: Option<Res<TentInteriorAssets>>,
    mut gfx_query: Query<&mut ScriptPlayer<SpriteAnimation>, With<PlayerGfx>>,
    stealth_q: Query<
        Has<crate::game::effects::stealthed::StealthEffect>,
        With<Player>,
    >,
    interior_occupant_q: Query<Option<&TentInteriorOccupant>, With<Player>>,
    capsule_q: Query<(Entity, &Transform, &CapsuleCollisionMask), With<Capsule>>,
) {
    for (
        entity,
        mut velocity,
        mut falling,
        action_state,
        horizontal_intent,
        ground_sensor,
        stats,
        stat_mod,
        mut facing,
        transform,
        wall_sensor,
        collider,
        mut buffer,
    ) in query.iter_mut()
    {
        let (has_locomotion_override, has_pogo) =
            locomotion_flags_q
                .get(entity)
                .map(|(action, pogo)| (action.owns_motion(), pogo))
                .unwrap_or((false, false));
        let _is_stealthed = stealth_q.get(entity).unwrap_or(false);
        // If pogo is active, defer to pogo system
        if has_pogo {
            continue;
        }
        if ground_sensor.is_grounded {
            transition_grounded_falling(&mut commands, entity, horizontal_intent.movement);
            continue;
        }
        // Suspend locomotion while a skill overrides movement
        if has_locomotion_override {
            continue;
        }
        let move_input = horizontal_intent.movement;
        let interior_kind = interior_occupant_q
            .get(entity)
            .ok()
            .flatten()
            .map(|occupant| occupant.kind);

        // Check for wall sliding conditions
        let half_extents =
            collider.cuboid_half_extents();
        let position = transform.translation.truncate();
        let active_collision = collision_space(
            interior_kind,
            tent_interior.as_deref(),
            collision_world.as_deref(),
        );
        let (collision_world, origin) = active_collision
            .map(|(world, origin)| (Some(world), origin))
            .unwrap_or((None, Vec2::ZERO));
        let local_position = position - origin;
        let mut top_contact_left = wall_slide_top_half_contact(
            collision_world,
            local_position,
            half_extents,
            WallSide::Left,
        );
        let mut top_contact_right = wall_slide_top_half_contact(
            collision_world,
            local_position,
            half_extents,
            WallSide::Right,
        );
        if interior_kind.is_none() {
            top_contact_left |= capsule_wall_slide_top_half_contact(
                position,
                half_extents,
                WallSide::Left,
                &capsule_q,
            );
            top_contact_right |= capsule_wall_slide_top_half_contact(
                position,
                half_extents,
                WallSide::Right,
                &capsule_q,
            );
        }

        let should_wall_slide = velocity.0.y < 0.0 // Falling
            && move_input.abs() > 0.1 // Holding a direction
            && ((move_input < 0.0 && wall_sensor.left_contact) // Pushing left into left wall
                || (move_input > 0.0 && wall_sensor.right_contact)) // Pushing right into right wall
            && ((move_input < 0.0 && top_contact_left)
                || (move_input > 0.0 && top_contact_right));

        // Wall slide state transitions and visuals
        if should_wall_slide && falling.wall_slide.is_none() {
            let wall_side = wall_slide_side_from_input(move_input, wall_sensor);
            falling.wall_slide = Some(wall_side);
            commands
                .entity(entity)
                .insert(crate::game::player::WallSlideTime(0.0));
            // Clamp fall index to wall-slide subset if we entered with high speed
            falling.fall_ticks = falling.fall_ticks.min(WALL_SLIDE_MAX_INDEX);

            // Play wall-slide animation if no skill is driving visuals
            play_wall_slide_for_entity(
                entity,
                wall_side,
                &gent_query,
                &mut gfx_query,
            );
        } else if !should_wall_slide && falling.wall_slide.is_some() {
            // Exit wall slide; resume normal fall curve
            falling.wall_slide = None;

            // Switch back to fall animation if not in a skill state
            play_fall_for_entity(
                entity,
                velocity.0.y,
                &gent_query,
                &mut gfx_query,
            );
        }

        // Jump handling (instant, buffered, wall/coyote/double)
        let current_tick = time.tick();
        let jump_requested = action_state.just_pressed(&PlayerAction::Jump)
            || buffer
                .check_buffered(PlayerAction::Jump, current_tick)
                .is_some();
        if jump_requested
            && try_wall_jump(
                entity,
                &mut commands,
                &mut velocity,
                &falling,
                horizontal_intent,
                &mut facing,
                &mut buffer,
            )
        {
            continue;
        }
        if jump_requested
            && try_coyote_jump(entity, &mut commands, &falling, &mut buffer)
        {
            continue;
        }
        if jump_requested
            && try_midair_jump(
                entity,
                &mut commands,
                &falling,
                stat_mod,
                &mut buffer,
            )
        {
            continue;
        }

        // Set fall velocity based on wall slide vs. normal fall
        apply_falling_velocity(
            &mut velocity,
            &falling,
            horizontal_intent,
            stats,
            stat_mod,
            &mut facing,
        );

        // Advance counters
        advance_falling_counters(&mut falling);
    }
}

fn try_wall_jump(
    entity: Entity,
    commands: &mut Commands,
    velocity: &mut LinearVelocity,
    falling: &Falling,
    horizontal_intent: &HorizontalInputIntent,
    facing: &mut Facing,
    buffer: &mut InputBuffer,
) -> bool {
    let Some(wall_side) = falling.wall_slide else {
        return false;
    };

    if pressing_into_wall(wall_side, horizontal_intent.movement) {
        *facing = wall_jump_facing(wall_side);
        velocity.0.x = wall_jump_pushback(wall_side);
        insert_wall_jump_pushback(commands, entity, velocity.0.x);
    }

    buffer.clear_action(PlayerAction::Jump);
    transition_locomotion(commands, entity, Jumping::new());
    true
}

fn pressing_into_wall(wall_side: WallSide, direction: f32) -> bool {
    match wall_side {
        WallSide::Left => direction < -0.1,
        WallSide::Right => direction > 0.1,
    }
}

fn wall_jump_facing(wall_side: WallSide) -> Facing {
    match wall_side {
        WallSide::Left => Facing::Right,
        WallSide::Right => Facing::Left,
    }
}

fn wall_jump_pushback(wall_side: WallSide) -> f32 {
    match wall_side {
        WallSide::Left => WALL_PUSHBACK,
        WallSide::Right => -WALL_PUSHBACK,
    }
}

fn insert_wall_jump_pushback(
    commands: &mut Commands,
    entity: Entity,
    pushback_x: f32,
) {
    use crate::game::physics::Knockback;

    commands.entity(entity).insert(Knockback::new(
        Vec2::new(pushback_x, 0.0),
        WALL_PUSHBACK_TICKS,
    ));
    commands
        .entity(entity)
        .insert(crate::game::player::WallSlideTime(f32::MAX));
}

fn try_coyote_jump(
    entity: Entity,
    commands: &mut Commands,
    falling: &Falling,
    buffer: &mut InputBuffer,
) -> bool {
    if !falling.from_ground || falling.coyote_ticks >= COYOTE_TIME_TICKS {
        return false;
    }

    buffer.clear_action(PlayerAction::Jump);
    transition_locomotion(commands, entity, Jumping::new());
    true
}

fn try_midair_jump(
    entity: Entity,
    commands: &mut Commands,
    falling: &Falling,
    stat_mod: &PlayerStatMod,
    buffer: &mut InputBuffer,
) -> bool {
    if falling.from_ground {
        return false;
    }

    let allowed_jumps = 2 + stat_mod.extra_jumps;
    if falling.jump_count >= allowed_jumps {
        return false;
    }

    buffer.clear_action(PlayerAction::Jump);
    transition_locomotion(
        commands,
        entity,
        Jumping::with_count(falling.jump_count + 1),
    );
    true
}

fn apply_falling_velocity(
    velocity: &mut LinearVelocity,
    falling: &Falling,
    horizontal_intent: &HorizontalInputIntent,
    stats: &PlayerStats,
    stat_mod: &PlayerStatMod,
    facing: &mut Facing,
) {
    let Some(wall_side) = falling.wall_slide else {
        apply_normal_fall_velocity(
            velocity,
            falling,
            horizontal_intent,
            stats,
            stat_mod,
        );
        return;
    };

    apply_wall_slide_velocity(velocity, falling, wall_side, facing);
}

fn apply_wall_slide_velocity(
    velocity: &mut LinearVelocity,
    falling: &Falling,
    wall_side: WallSide,
    facing: &mut Facing,
) {
    let max_idx = WALL_SLIDE_MAX_INDEX
        .min(crate::game::physics::GRAVITY_VELOCITIES.len() as u32 - 1);
    velocity.0.y = fall_velocity_for_tick(falling.fall_ticks.min(max_idx));
    velocity.0.x = 0.0;
    *facing = wall_jump_facing(wall_side);
}

fn apply_normal_fall_velocity(
    velocity: &mut LinearVelocity,
    falling: &Falling,
    horizontal_intent: &HorizontalInputIntent,
    stats: &PlayerStats,
    stat_mod: &PlayerStatMod,
) {
    velocity.0.y = fall_velocity_for_tick(falling.fall_ticks);
    let direction = horizontal_intent.movement;
    if direction != 0.0 {
        let base_speed = stats.max_move_velocity;
        velocity.0.x =
            direction * base_speed * AIR_SPEED_FACTOR * stat_mod.speed;
        return;
    }

    velocity.0.x = 0.0;
}

fn fall_velocity_for_tick(fall_ticks: u32) -> f32 {
    let max_idx =
        (crate::game::physics::GRAVITY_VELOCITIES.len() as u32).saturating_sub(1);
    crate::game::physics::gravity_velocity(fall_ticks.min(max_idx))
}

fn advance_falling_counters(falling: &mut Falling) {
    falling.coyote_ticks += 1;
    let max_idx = if falling.wall_slide.is_some() {
        WALL_SLIDE_MAX_INDEX
    } else {
        (crate::game::physics::GRAVITY_VELOCITIES.len() as u32).saturating_sub(1)
    };

    if falling.fall_ticks < max_idx {
        falling.fall_ticks += 1;
    }
}

fn wall_slide_top_half_contact(
    collision_world: Option<&TileCollisionWorld>,
    position: Vec2,
    half_extents: Option<Vec2>,
    side: WallSide,
) -> bool {
    let Some(collision_world) = collision_world else {
        return true;
    };
    if !collision_world.is_ready() {
        return true;
    }
    let Some(half_extents) = half_extents else {
        return true;
    };
    let top_y = position.y + half_extents.y - WALL_SLIDE_TOP_SAMPLE_EPS;
    let top_half_mid_y = position.y + half_extents.y * 0.5;
    let sample_x = match side {
        WallSide::Left => {
            position.x - half_extents.x - WALL_SLIDE_SIDE_SAMPLE_EPS
        },
        WallSide::Right => {
            position.x + half_extents.x + WALL_SLIDE_SIDE_SAMPLE_EPS
        },
    };
    collision_world.is_solid_at_world_point(Vec2::new(sample_x, top_y))
        && collision_world
            .is_solid_at_world_point(Vec2::new(sample_x, top_half_mid_y))
}

fn capsule_wall_slide_top_half_contact(
    position: Vec2,
    half_extents: Option<Vec2>,
    side: WallSide,
    capsule_q: &Query<
        (Entity, &Transform, &CapsuleCollisionMask),
        With<Capsule>,
    >,
) -> bool {
    let Some(half_extents) = half_extents else {
        return true;
    };
    let top_y = position.y + half_extents.y - WALL_SLIDE_TOP_SAMPLE_EPS;
    let top_half_mid_y = position.y + half_extents.y * 0.5;
    let sample_x = match side {
        WallSide::Left => {
            position.x - half_extents.x - WALL_SLIDE_SIDE_SAMPLE_EPS
        },
        WallSide::Right => {
            position.x + half_extents.x + WALL_SLIDE_SIDE_SAMPLE_EPS
        },
    };
    capsule_is_solid_at_world_point(Vec2::new(sample_x, top_y), capsule_q)
        && capsule_is_solid_at_world_point(
            Vec2::new(sample_x, top_half_mid_y),
            capsule_q,
        )
}
