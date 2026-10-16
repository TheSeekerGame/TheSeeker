// Stationary trajectory solver translated from:
// https://github.com/forrestthewoods/lib_fts/blob/master/code/fts_ballistic_trajectory.cs

use glam::Vec2;

/// For a stationary target
pub fn solve_ballistic_arc(
    proj_pos: Vec2,
    proj_speed: f32,
    target: Vec2,
    gravity: f32,
) -> (Vec2, Vec2, i32) {
    if proj_pos != target && proj_speed > 0.0 && gravity > 0.0 {
    } else {
        return (Vec2::ZERO, Vec2::ZERO, 0);
    }

    let mut s0 = Vec2::ZERO;
    let mut s1 = Vec2::ZERO;

    let diff = target - proj_pos;
    let speed2 = proj_speed * proj_speed;
    let speed4 = proj_speed * proj_speed * proj_speed * proj_speed;
    let y = diff.y;
    let x = diff.x.abs();
    let gx = gravity * x;
    let root = speed4 - gravity * (gravity * x * x + 2.0 * y * speed2);

    // No solution
    if root < 0.0 {
        return (s0, s1, 0);
    }

    let root = root.sqrt();
    let low_ang = f32::atan2(speed2 - root, gx);
    let high_ang = f32::atan2(speed2 + root, gx);
    let num_solutions = if low_ang != high_ang { 2 } else { 1 };

    let ground_dir = Vec2::X * diff.x.signum();
    s0 = ground_dir * f32::cos(low_ang) * proj_speed
        + Vec2::new(0.0, f32::sin(low_ang) * proj_speed);
    if num_solutions > 1 {
        s1 = ground_dir * f32::cos(high_ang) * proj_speed
            + Vec2::new(0.0, f32::sin(high_ang) * proj_speed);
    }

    (s0, s1, num_solutions)
}
