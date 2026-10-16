use super::*;

pub(super) fn enemy_gravity(
    mut query: Query<
        (&mut LinearVelocity, &mut Navigation),
        (
            With<Enemy>,
            With<ActorController>,
            With<GroundMotor>,
            Without<crate::game::effects::frozen::Frozen>,
        ),
    >,
) {
    for (mut velocity, mut nav) in query.iter_mut() {
        match *nav {
            Navigation::Falling {
                mut fall_ticks,
                jumping,
            } => {
                velocity.0.y =
                    curve_velocity(ENEMY_FALL_VELOCITIES, fall_ticks);
                fall_ticks += 1;
                *nav = Navigation::Falling {
                    fall_ticks,
                    jumping,
                };
            },
            Navigation::Grounded | Navigation::Blocked => {},
        }
    }
}

pub(super) fn move_collide(
    mut commands: Commands,
    collision_world: Option<Res<TileCollisionWorld>>,
    mut query: Query<
        (
            Entity,
            &mut LinearVelocity,
            &mut Transform,
            &mut Navigation,
            &Collider,
            Has<Knockback>,
            &mut MovementState,
        ),
        (
            With<Enemy>,
            With<GroundMotor>,
            Without<crate::game::effects::frozen::Frozen>,
        ),
    >,
    time: Res<GameTime>,
    mut profiler: TickProfiler,
) {
    let _profile = profiler.scope("enemy_move_collide");
    let collision_world =
        collision_world.as_deref().filter(|world| world.is_ready());

    for (
        entity,
        mut linear_velocity,
        mut transform,
        mut nav,
        collider,
        is_knocked,
        mut movement,
    ) in query.iter_mut()
    {
        let half_extents = collider.half_extents();
        let half_extents = Vec2::new(half_extents.x, half_extents.y);
        if !movement.frame_steps.is_empty() {
            commands.entity(entity).insert(theseeker_engine::presentation::PresentationJump(time.tick()));
        }
        for step in movement.frame_steps.drain(..) {
            resolve_ground_displacement(collision_world, &mut transform, &mut nav, half_extents, step.0, is_knocked);
        }
        linear_velocity.0 = resolve_ground_displacement(
            collision_world, &mut transform, &mut nav, half_extents, linear_velocity.0, is_knocked,
        );
    }
}

fn resolve_ground_displacement(
    collision_world: Option<&TileCollisionWorld>,
    transform: &mut Transform,
    nav: &mut Navigation,
    half_extents: Vec2,
    mut displacement: Vec2,
    is_knocked: bool,
) -> Vec2 {
    let z = transform.translation.z;
    let Some(collision_world) = collision_world.filter(|_| half_extents.x > 0.0 && half_extents.y > 0.0) else {
        transform.translation += displacement.extend(0.0);
        return displacement;
    };
    let was_falling = matches!(*nav, Navigation::Falling { .. });
    let blocked = stop_at_ground_edge(collision_world, &mut displacement, transform, half_extents, nav);
    let result = collision_world.resolve_movement(transform.translation.xy(), displacement, half_extents);
    if (blocked || result.hit_wall) && !is_knocked && !was_falling {
        *nav = Navigation::Blocked;
    }
    if result.grounded && !matches!(*nav, Navigation::Blocked) { *nav = Navigation::Grounded; }
    let mut resolved = result.velocity;
    apply_falling_transition(collision_world, nav, result.position, result.grounded, half_extents, &mut resolved);
    transform.translation = result.position.extend(z);
    resolved
}

pub(super) fn remove_inside(
    mut enemies: Query<
        (Entity, &GlobalTransform, &Collider),
        (With<Inside>, With<Enemy>),
    >,
    players_q: Query<
        (Entity, &GlobalTransform, &Collider),
        With<theseeker_engine::physics::inside::PlayerInsideEnemy>,
    >,
    mut commands: Commands,
    spatial_query: PhysicsWorld,
) {
    for (enemy_entity, transform, collider) in enemies.iter_mut() {
        let intersections = spatial_query.intersect(
            transform.translation().xy(),
            collider,
            theseeker_engine::physics::groups::groups(ENEMY_INSIDE),
            Some(enemy_entity),
        );
        let still_inside: std::collections::HashSet<Entity> =
            intersections.into_iter().collect();

        for player in players_q.iter() {
            clear_player_inside_if_needed(
                enemy_entity,
                player,
                &still_inside,
                &spatial_query,
                &mut commands,
            );
        }
    }
}

fn stop_at_ground_edge(
    collision_world: &TileCollisionWorld,
    velocity: &mut Vec2,
    transform: &Transform,
    half_extents: Vec2,
    nav: &Navigation,
) -> bool {
    if !matches!(*nav, Navigation::Grounded) || velocity.x.abs() <= f32::EPSILON
    {
        return false;
    }

    let dir = velocity.x.signum();
    let ahead_offset = half_extents.x + 2.0;
    let foot_offset = half_extents.y + 1.0;
    let start = Vec2::new(
        transform.translation.x + ahead_offset * dir,
        transform.translation.y - half_extents.y + 0.5,
    );
    let ray_length = foot_offset + 4.0;
    let probe_half_extents = Vec2::new(1.0, 0.0);
    let distance =
        collision_world.distance(ProbeDirection::Down, start, probe_half_extents, ray_length);
    let blocked = distance >= ray_length;
    if blocked {
        velocity.x = 0.0;
    }
    blocked
}

fn apply_falling_transition(
    collision_world: &TileCollisionWorld,
    nav: &mut Navigation,
    position: Vec2,
    grounded: bool,
    half_extents: Vec2,
    final_velocity: &mut Vec2,
) {
    if !matches!(*nav, Navigation::Grounded) || grounded {
        return;
    }

    let ground_dist =
        collision_world.distance(ProbeDirection::Down, position, half_extents, GROUNDED_THRESHOLD);
    if ground_dist < GROUNDED_THRESHOLD {
        return;
    }

    *nav = Navigation::Falling {
        fall_ticks: 0,
        jumping: false,
    };
    final_velocity.y = 0.0;
}

fn clear_player_inside_if_needed(
    enemy_entity: Entity,
    player: (Entity, &GlobalTransform, &Collider),
    still_inside: &std::collections::HashSet<Entity>,
    spatial_query: &PhysicsWorld,
    commands: &mut Commands,
) {
    let (player_entity, transform, collider) = player;
    if still_inside.contains(&player_entity) {
        return;
    }

    let overlap = spatial_query.intersect(
        transform.translation().xy(),
        collider,
        theseeker_engine::physics::groups::groups(ENEMY_INSIDE),
        Some(player_entity),
    );
    if overlap.is_empty() {
        theseeker_engine::physics::inside::clear(
            commands,
            enemy_entity,
            player_entity,
        );
    }
}
