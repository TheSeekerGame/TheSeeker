use super::*;
use crate::camera::CameraPlugin;
use crate::game::player::PlayerStateSet;

const VIEW: Vec2 = Vec2::new(320.0, 180.0);

fn assert_vec2_near(actual: Vec2, expected: Vec2) {
    assert!((actual - expected).abs().max_element() < 0.001, "expected {expected:?}, got {actual:?}");
}

fn level_with_terrain(width: u32, height: u32, terrain: Vec<bool>) -> CompiledLevel {
    assert_eq!(terrain.len(), (width * height) as usize);
    CompiledLevel { bounds: Rect::new(0.0, 0.0, width as f32, height as f32), width, height, terrain, ..default() }
}

/// Frames `player` for a tick with no open space to lean toward, returning the direction in
/// which the player left the dead zone, if they did.
fn frame_tick(rig: &mut CameraRig, player: PlayerMotion, dt: f32) -> Vec2 {
    let mut left = Vec2::ZERO;
    rig.frame(player, VIEW, dt, |direction| {
        left = direction;
        player.position
    });
    left
}

#[test]
fn camera_spring_retargets_without_resetting_velocity() {
    let mut motion = AxisMotion::default();
    let mut position = 0.0;
    for _ in 0..20 {
        motion.advance(&mut position, 30.0, 1.0 / 96.0, SMOOTH_TIME.x);
    }
    let velocity_before_retarget = motion.velocity;
    motion.advance(&mut position, 60.0, 1.0 / 96.0, SMOOTH_TIME.x);
    assert!(velocity_before_retarget > 0.0);
    assert!(motion.velocity > 0.0);
    assert!(position > 0.0 && position < 60.0);
}

#[test]
fn sustained_travel_has_no_periodic_slowdowns_or_geometry_resampling() {
    for speed in [24.0_f32, 96.0, -96.0] {
        let mut rig = CameraRig::default();
        let mut player = PlayerMotion { grounded: true, ..default() };
        let dt = 1.0 / 96.0;
        frame_tick(&mut rig, player, dt);
        let mut departures = 0;
        for tick in 0..960 {
            player.position.x += speed * dt;
            player.velocity.x = speed;
            let previous_x = rig.position.x;
            departures += usize::from(frame_tick(&mut rig, player, dt).x != 0.0);
            let actual_speed = (rig.position.x - previous_x) / dt;
            assert!(tick <= 480 || (actual_speed - speed).abs() < 0.25, "{actual_speed} vs {speed}");
        }
        assert_eq!(departures, 1);
    }
}

#[test]
fn stopping_then_retreating_inside_the_dead_zone_keeps_the_anchor() {
    let mut rig = CameraRig::default();
    let dt = 1.0 / 96.0;
    let mut player = PlayerMotion { grounded: true, ..default() };
    frame_tick(&mut rig, player, dt);
    for _ in 0..192 {
        player.position.x += 1.0;
        player.velocity.x = 96.0;
        frame_tick(&mut rig, player, dt);
    }
    let anchor = rig.anchor;
    player.position.x -= 20.0;
    player.velocity = Vec2::ZERO;
    for _ in 0..960 {
        assert_eq!(frame_tick(&mut rig, player, dt), Vec2::ZERO);
        assert_eq!(rig.anchor, anchor);
    }
    assert_vec2_near(rig.position, anchor);
    assert_eq!(rig.axes[0].velocity, 0.0);
}

#[test]
fn camera_schedule_reads_player_position_after_collision_resolution() {
    let mut app = App::new();
    app.add_plugins(CameraPlugin);
    app.init_resource::<GameTime>();
    app.insert_resource(level_with_terrain(16, 16, vec![false; 256]));
    app.add_systems(GameTickUpdate, simulate_player_collision.in_set(PlayerStateSet::Collisions));
    app.world_mut().spawn((Player, Grounded, Transform::from_xyz(8.0, 8.0, 0.0), LinearVelocity(Vec2::new(0.0, -2.0))));
    app.world_mut().run_schedule(GameTickUpdate);
    let rig = app.world().resource::<CameraRig>();
    assert_eq!(rig.anchor, Vec2::new(8.0, 6.0));
    assert_eq!(rig.fall.takeoff_height, 6.0);
}

fn simulate_player_collision(mut players: Query<(&mut Transform, &LinearVelocity), With<Player>>) {
    for (mut transform, velocity) in &mut players {
        transform.translation += velocity.0.extend(0.0);
    }
}

#[test]
fn ordinary_jump_has_little_camera_travel_and_no_fall_anticipation() {
    use crate::game::physics::{GRAVITY_VELOCITIES, JUMP_VELOCITIES};
    let mut rig = CameraRig::default();
    let dt = 1.0 / 96.0;
    let mut player = PlayerMotion { grounded: true, ..default() };
    frame_tick(&mut rig, player, dt);
    let mut max_camera_height = 0.0_f32;
    for displacement in JUMP_VELOCITIES.iter().chain(GRAVITY_VELOCITIES).copied() {
        player.position.y = (player.position.y + displacement).max(0.0);
        player.grounded = player.position.y == 0.0;
        player.velocity.y = displacement / dt * f32::from(!player.grounded);
        frame_tick(&mut rig, player, dt);
        max_camera_height = max_camera_height.max(rig.position.y.abs());
        assert_eq!(rig.fall.strength, 0.0);
    }
    assert!(max_camera_height < 16.0, "jump camera travel: {max_camera_height}");
}

#[test]
fn falling_shows_space_below_and_stays_visible_at_multiple_tick_rates() {
    for tick_rate in [60, 96, 144] {
        simulate_long_fall(tick_rate, -1.705 * 96.0);
        simulate_long_fall(tick_rate, -4.167 * 96.0);
    }
}

fn simulate_long_fall(tick_rate: u32, speed: f32) {
    let mut rig = CameraRig::default();
    let dt = 1.0 / tick_rate as f32;
    let mut player = PlayerMotion { grounded: true, ..default() };
    frame_tick(&mut rig, player, dt);
    player.grounded = false;
    player.velocity.y = speed;
    for tick in 0..tick_rate * 5 {
        player.position.y += speed * dt;
        frame_tick(&mut rig, player, dt);
        let screen_y = player.position.y - rig.position.y;
        assert!(screen_y >= -VIEW.y * VISIBLE_BOTTOM_SHARE - 0.001);
        assert!(screen_y <= VIEW.y * VISIBLE_SIDE_SHARE + 0.001);
        assert!(tick <= tick_rate * 2 || screen_y > 0.0, "falling player below center: {screen_y}, {speed}");
    }
    player.grounded = true;
    player.velocity = Vec2::ZERO;
    let landed_anchor = rig.anchor;
    for _ in 0..tick_rate * 8 {
        frame_tick(&mut rig, player, dt);
        assert_eq!(rig.anchor, landed_anchor);
    }
    assert_vec2_near(rig.position, landed_anchor);
}

#[test]
fn open_space_behind_a_solid_divider_draws_no_camera() {
    let (width, height) = (9, 5);
    let terrain = (0..width * height).map(|index| index % width == 4).collect();
    let level = level_with_terrain(width, height, terrain);
    let focus = OpenSpaceSearch::default().centroid(&level, None, Vec2::new(1.5, 2.5), Vec2::new(9.0, 5.0), Vec2::ZERO).unwrap();
    assert!(focus.x < 4.0, "unexpected focus behind wall: {focus:?}");
}

#[test]
fn open_space_behind_a_secret_room_cover_draws_no_camera() {
    let level = level_with_terrain(7, 5, vec![false; 35]);
    let mut search = OpenSpaceSearch::default();
    search.covers.extend((0..5).map(|y| IVec2::new(3, y)));
    let focus = search.centroid(&level, None, Vec2::new(1.5, 2.5), Vec2::new(7.0, 5.0), Vec2::ZERO).unwrap();
    assert!(focus.x < 3.0, "unexpected focus behind cover: {focus:?}");
}

#[test]
fn heading_biases_the_open_space_centroid_symmetrically() {
    let level = level_with_terrain(9, 5, vec![false; 45]);
    let mut search = OpenSpaceSearch::default();
    let player = Vec2::new(4.5, 2.5);
    let view = Vec2::new(9.0, 5.0);
    let right = search.centroid(&level, None, player, view, Vec2::X).unwrap();
    let left = search.centroid(&level, None, player, view, Vec2::NEG_X).unwrap();
    assert!(right.x > player.x);
    assert!(left.x < player.x);
    assert!(((right.x - player.x) + (left.x - player.x)).abs() < 0.001);
}

#[test]
fn open_space_centroid_pins_its_weighting_on_an_irregular_cave() {
    // A cave with ledges, pillars, and a cover, whose rows are listed from the top.
    let rows = [
        "############",
        "#....#.....#",
        "#.##.#..#..#",
        "#......##..#",
        "#.#.......##",
        "#.#..#.....#",
        "#....#..#..#",
        "############",
    ];
    let terrain = rows.iter().flat_map(|row| row.chars().map(|tile| tile == '#')).collect();
    let level = level_with_terrain(12, 8, terrain);
    let mut search = OpenSpaceSearch::default();
    search.covers.extend([IVec2::new(9, 4), IVec2::new(9, 5)]);
    let focus = search.centroid(&level, None, Vec2::new(3.2, 4.7), Vec2::new(9.0, 6.0), Vec2::new(1.0, -1.0)).unwrap();
    // About (4.64, 4.11), recorded bit for bit so reshaping the search cannot shift it.
    assert_eq!(focus, Vec2::new(f32::from_bits(1_083_473_428), f32::from_bits(1_082_356_048)));
}

#[test]
fn crossed_axes_cannot_frame_backwards() {
    let player = Vec2::new(10.0, 20.0);
    let geometry_focus = Vec2::new(4.0, 28.0);
    assert_eq!(constrain_focus_to_crossed_sides(geometry_focus, player, Vec2::new(1.0, -1.0)), player);
    assert_eq!(constrain_focus_to_crossed_sides(geometry_focus, player, Vec2::ZERO), geometry_focus);
}
