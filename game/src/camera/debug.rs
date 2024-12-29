// Timing diagnostic
#[derive(Resource)]
struct SystemProfiler {
    total_duration: f32,
}

pub fn camera_rig_debug_print(&self) {
    println!("CameraRig Debug:");
    println!("  Target: {}", self.target);
    println!("  Camera Position: {}", self.camera_position);
    println!("  Displacement: {}", self.displacement);
    println!("  Equilibrium_y: {}", self.equilibrium_y);
}

pub fn camera_spring_debug_print(&self) {
    println!("---------------------------------");
    println!("CameraSpring Debug:");
    println!("---------------------------------");
    println!("  Floor: {}", FLOOR);
    println!("  Ceiling: {}", CEILING);
    println!("  Fall Buffer: {}", FALL_BUFFER);
    //println!("  Limit Override: {}", self.limit_override);
    println!("  Spring Constant (k): {}", self.k);
    println!("  Fast Spring Constant (k_fast): {}", K_FAST);
    println!("  Regular Spring Constant (k_reg): {}", K_REG);
    println!("  Damping Coefficient: {}", DAMPING_RATIO);
    println!("  Velocity: {}", self.velocity);
    // println!("  Vertical Reset: {}", self.vertical_reset);
    // println!("  Horizontal Snapped: {}", self.horizontal_snapped);
    println!("  Vertical Snapped: {}", self.vertical_snapped);
    println!("  Reset Threshold: {}", RESET_THRESHOLD);
    println!("  Snap Threshold: {}", SNAP_THRESHOLD);
    println!("  Equalize Threshold: {}", EQUALIZE_THRESHOLD);
    println!("  Displacement Factor: {}", self.fall_factor);
    println!("  Horizontal Phase: {}", self.x_phase);
    println!("  Vertical Phase: {}", self.y_phase);
    println!("  Follow Strategy: {}", self.follow_strategy);
    println!("---------------------------------");
}
fn draw_debug_gizmos(
    rig: Res<CameraRig>,
    mut gizmos: Gizmos,
) {
    gizmos.circle_2d(
        Vec2::new(rig.camera_position.x, rig.camera_position.y),
          4.0,
          Color::GREEN,
      );
    gizmos.rect(
        Vec3::new(rig.target.x, rig.target.y, 0.),
        Quat::from_rotation_y(0.0),
        Vec2::splat(3.),
        Color::RED,
    );
    

}