use std::cmp::Ordering;

use theseeker_engine::{physics::{PhysicsWorld, ShapeCaster}, prelude::*};

use super::*;

// Timing diagnostic
#[derive(Resource)]
struct SystemProfiler {
    total_duration: f32,
}

pub fn update_camera_rig_debug_print(
    rig_data: Res<RigData>,
    query: Query<&Rig, With<MainCamera>>,
) {
    let rig = match query.get_single() {
        Ok(rig) => rig, 
        Err(_) => return,
    };
    print!("\x1B[2J\x1B[1;1H");
    println!("Rig Debug:");
    println!("  Target: {}", rig_data.target);
    println!("  Camera Position: {}", rig.next_position);
    println!("  Displacement: {}", rig_data.displacement);
    println!("  Equilibrium_y: {}", rig_data.equilibrium_y);
}

pub fn update_camera_spring_debug_print(
    spatial_query: Res<PhysicsWorld>,
    ground_query: Query<(Entity, &ShapeCaster, &Transform), With<Player>>,
    spring_data: Res<SpringData>,
    query: Query<(&SpringPhaseX, &SpringPhaseY), With<MainCamera>>,
    follow_query: Query<&FollowStrategy, With<MainCamera>>,
) {
    let (phase_x, phase_y) = match query.get_single() {
        Ok((phase_x, phase_y)) => (phase_x, phase_y), 
        Err(_) => return,
    };
    let follow_strategy = match follow_query.get_single() {
        Ok(follow_strategy) => follow_strategy, 
        Err(_) => return, 
    };
    for (entity, ray_cast_info, position) in ground_query.iter() {
        let ground_distance = ray_cast_info
            .cast(&spatial_query, &position, Some(entity))
            .into_iter().filter_map(|hit| {
                // Extract the time of impact (toi) directly as the ground distance
                Some(hit.1.toi)
            })
            .min_by(|a, b| a.partial_cmp(b).unwrap_or(Ordering::Equal))
            .unwrap_or_else(|| {
                println!("Warning: No ground distance found for entity {:?}", entity);
                f32::INFINITY 
            });
            dbg!(ground_distance);
        }
    println!("---------------------------------");
    println!("CameraSpring Debug:");
    println!("---------------------------------");
    println!("  Floor: {}", FLOOR);
    println!("  Ceiling: {}", CEILING);
    println!("  Fall Buffer: {}", FALL_BUFFER);
    //println!("  Limit Override: {}", self.limit_override);
    println!("  Spring Constant (k): {}", spring_data.k);
    println!("  Fast Spring Constant (k_fast): {}", K_FAST);
    println!("  Regular Spring Constant (k_reg): {}", K_REG);
    println!("  Damping Coefficient: {}", DAMPING_RATIO);
    println!("  Velocity: {}", spring_data.velocity);
    // println!("  Vertical Reset: {}", spring_data.vertical_reset);
    // println!("  Horizontal Snapped: {}", spring_data.horizontal_snapped);
    println!("  Vertical Snapped: {}", spring_data.vertical_snapped);
    println!("  Reset Threshold: {}", RESET_THRESHOLD);
    println!("  Snap Threshold: {}", SNAP_THRESHOLD);
    println!("  Equalize Threshold: {}", EQUALIZE_THRESHOLD);
    println!("  Displacement Factor: {}", spring_data.fall_factor);
    println!("  Horizontal Phase: {:?}", *phase_x);
    println!("  Vertical Phase: {:?}", *phase_y);
    println!("  Follow Strategy: {:?}", *follow_strategy);
    println!("---------------------------------");
}

pub fn update_player_info_debug_print(
    player_info_query: Query<&PlayerInfo, With<MainCamera>>,
    dashed_query: Query<Entity, (With<Player>, Added<CanDash>)>,
    dash_timer_query: Query<&DashTimer, With<MainCamera>>,
) {
    let player_info = match player_info_query.get_single() {
        Ok(player_info) => player_info,
        Err(_) => return,
    };
    let just_dashed = match dashed_query.get_single() {
        Ok(_) => true,
        Err(_) => false,
    };
    let dash_timer = match dash_timer_query.get_single() {
        Ok(dash_timer) => dash_timer, 
        Err(_) => return,
    };
    //print!("\x1B[2J\x1B[1;1H");
    println!("PlayerTracker Debug:");
    println!("  Last Grounded Y: {}", player_info.previous_grounded_y);
    println!("  Current Grounded Y: {}", player_info.grounded_y);
    //println!("  Ground Distance: {}", player_info.ground_distance);
    println!("  Ground?: {}", player_info.is_grounded);
    //println!("  Velocity: {}", player_info.velocity);
    //println!("  Position: {}", player_info.position);
    println!("  Just Dashed: {:?}", dash_timer.just_dashed);
    println!(" Dash Timer {}", dash_timer.remaining);
}
pub(super) fn draw_debug_gizmos(
    rig_query: Query<&Rig, With<MainCamera>>,
    rig_data: Res<RigData>,
    mut gizmos: Gizmos,
) {

    let rig = match rig_query.get_single() {
        Ok(rig) => rig,
        Err(_) => return,
    };
    gizmos.circle_2d(
        Vec2::new(rig.next_position.x, rig.next_position.y),
          4.0,
          Color::GREEN,
      );
    gizmos.rect(
        Vec3::new(rig_data.target.x, rig_data.target.y, 0.),
        Quat::from_rotation_y(0.0),
        Vec2::splat(3.),
        Color::RED,
    );
    

}