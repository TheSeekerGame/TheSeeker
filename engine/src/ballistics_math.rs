// Credit: translated to rust from, everything except ballistic_speed()
// https://github.com/forrestthewoods/lib_fts/blob/master/code/fts_ballistic_trajectory.cs

use glam::Vec2;

fn is_zero(value: f64) -> bool {
    value.abs() < 1e-6
}

fn get_cubic_root(value: f64) -> f64 {
    if value > 0.0 {
        value.powf(1.0 / 3.0)
    } else if value < 0.0 {
        -(-value).powf(1.0 / 3.0)
    } else {
        0.0
    }
}

fn solve_quadric(c0: f64, c1: f64, c2: f64, s0: &mut f64, s1: &mut f64) -> i32 {
    let mut p;
    let mut q;
    let mut d;

    // normal form: x^2 + px + q = 0
    p = c1 / (2.0 * c0);
    q = c2 / c0;
    d = p * p - q;

    if is_zero(d) {
        *s0 = -p;
        1
    } else if d < 0.0 {
        0
    } else {
        let sqrt_d = d.sqrt();
        *s0 = sqrt_d - p;
        *s1 = -sqrt_d - p;
        2
    }
}

fn solve_cubic(
    c0: f64,
    c1: f64,
    c2: f64,
    c3: f64,
    s0: &mut f64,
    s1: &mut f64,
    s2: &mut f64,
) -> i32 {
    let mut num;
    let mut sub;
    let mut a;
    let mut b;
    let mut c;
    let mut sq_a;
    let mut p;
    let mut q;
    let mut cb_p;
    let mut d;

    // normal form: x^3 + Ax^2 + Bx + C = 0
    a = c1 / c0;
    b = c2 / c0;
    c = c3 / c0;

    // substitute x = y - A/3 to eliminate quadric term: x^3 +px + q = 0
    sq_a = a * a;
    p = 1.0 / 3.0 * (-1.0 / 3.0 * sq_a + b);
    q = 1.0 / 2.0 * (2.0 / 27.0 * a * sq_a - 1.0 / 3.0 * a * b + c);

    // use Cardano's formula
    cb_p = p * p * p;
    d = q * q + cb_p;

    if is_zero(d) {
        if is_zero(q) {
            // one triple solution
            *s0 = 0.0;
            num = 1;
        } else {
            // one single and one double solution
            let u = get_cubic_root(-q);
            *s0 = 2.0 * u;
            *s1 = -u;
            num = 2;
        }
    } else if d < 0.0 {
        // Casus irreducibilis: three real solutions
        let phi = 1.0 / 3.0 * (-q / (-cb_p).sqrt()).acos();
        let t = 2.0 * (-p).sqrt();
        *s0 = t * phi.cos();
        *s1 = -t * (phi + std::f64::consts::PI / 3.0).cos();
        *s2 = -t * (phi - std::f64::consts::PI / 3.0).cos();
        num = 3;
    } else {
        // one real solution
        let sqrt_d = d.sqrt();
        let u = get_cubic_root(sqrt_d - q);
        let v = -get_cubic_root(sqrt_d + q);
        *s0 = u + v;
        num = 1;
    }

    // resubstitute
    sub = 1.0 / 3.0 * a;
    if num > 0 {
        *s0 -= sub;
    }
    if num > 1 {
        *s1 -= sub;
    }
    if num > 2 {
        *s2 -= sub;
    }

    num
}

fn solve_quartic(
    c0: f64,
    c1: f64,
    c2: f64,
    c3: f64,
    c4: f64,
    s0: &mut f64,
    s1: &mut f64,
    s2: &mut f64,
    s3: &mut f64,
) -> i32 {
    let mut coeffs = [0.0; 4];
    let (mut z, mut u, mut v, mut sub);
    let (mut a, mut b, mut c, mut d);
    let (mut sq_a, mut p, mut q, mut r);
    let mut num;

    // normal form: x^4 + Ax^3 + Bx^2 + Cx + D = 0
    a = c1 / c0;
    b = c2 / c0;
    c = c3 / c0;
    d = c4 / c0;

    // substitute x = y - A/4 to eliminate cubic term: x^4 + px^2 + qx + r = 0
    sq_a = a * a;
    p = -0.375 * sq_a + b;
    q = 0.125 * sq_a * a - 0.5 * a * b + c;
    r = -0.01171875 * sq_a * sq_a + 0.0625 * sq_a * b - 0.25 * a * c + d;

    if is_zero(r) {
        // no absolute term: y(y^3 + py + q) = 0
        coeffs[3] = q;
        coeffs[2] = p;
        coeffs[1] = 0.0;
        coeffs[0] = 1.0;
        num = solve_cubic(
            coeffs[0], coeffs[1], coeffs[2], coeffs[3], s0, s1, s2,
        );
    } else {
        // solve the resolvent cubic ...
        coeffs[3] = 0.5 * r * p - 0.125 * q * q;
        coeffs[2] = -r;
        coeffs[1] = -0.5 * p;
        coeffs[0] = 1.0;
        solve_cubic(
            coeffs[0], coeffs[1], coeffs[2], coeffs[3], s0, s1, s2,
        );

        // ... and take the one real solution ...
        z = *s0;

        // ... to build two quadric equations
        u = z * z - r;
        v = 2.0 * z - p;

        if is_zero(u) {
            u = 0.0;
        } else if u > 0.0 {
            u = u.sqrt();
        } else {
            return 0;
        }

        if is_zero(v) {
            v = 0.0;
        } else if v > 0.0 {
            v = v.sqrt();
        } else {
            return 0;
        }

        coeffs[2] = z - u;
        coeffs[1] = if q < 0.0 { -v } else { v };
        coeffs[0] = 1.0;
        num = solve_quadric(coeffs[0], coeffs[1], coeffs[2], s0, s1);

        coeffs[2] = z + u;
        coeffs[1] = if q < 0.0 { v } else { -v };
        coeffs[0] = 1.0;

        if num == 0 {
            num += solve_quadric(coeffs[0], coeffs[1], coeffs[2], s0, s1);
        } else if num == 1 {
            num += solve_quadric(coeffs[0], coeffs[1], coeffs[2], s1, s2);
        } else if num == 2 {
            num += solve_quadric(coeffs[0], coeffs[1], coeffs[2], s2, s3);
        }
    }

    // resubstitute
    sub = 0.25 * a;
    if num > 0 {
        *s0 -= sub;
    }
    if num > 1 {
        *s1 -= sub;
    }
    if num > 2 {
        *s2 -= sub;
    }
    if num > 3 {
        *s3 -= sub;
    }

    num
}

// Calculate the maximum range that a ballistic projectile can be fired on given speed and gravity.
//
// speed (f32): projectile velocity
// gravity (f32): force of gravity, positive is down
// initial_height (f32): distance above flat terrain
//
// return (f32): maximum range
pub fn ballistic_range(speed: f32, gravity: f32, initial_height: f32) -> f32 {
    // Handling these cases is up to your project's coding standards
    if speed > 0.0 && gravity > 0.0 && initial_height >= 0.0 {
    } else {
        return 0.0;
    }

    // Derivation
    // (1) x = speed * time * cos O
    // (2) y = initial_height + (speed * time * sin O) - (0.5 * gravity * time * time)
    // (3) via quadratic: t = (speed * sin O) / gravity + sqrt(speed * speed * sin O + 2 * gravity * initial_height) / gravity [ignore smaller root]
    // (4) solution: range = x = (speed * cos O) / gravity * sqrt(speed * speed * sin O + 2 * gravity * initial_height) [plug t back into x = speed * time * cos O]

    let angle = std::f32::consts::PI; // no air resistance, so 45 degrees provides maximum range
    let cos = angle.cos();
    let sin = angle.sin();

    let range = (speed * cos / gravity)
        * (speed * sin
            + (speed * speed * sin * sin + 2.0 * gravity * initial_height)
                .sqrt());

    range
}

/// Given a range and gravity, finds the needed speed
pub fn ballistic_speed(range: f32, gravity: f32, initial_height: f32) -> f32 {
    // Handling these cases is up to your project's coding standards
    if initial_height.abs() < 0.000001 && range.abs() < 0.000001 {
        return 0.0;
    }

    // Solving the ballistic_range equation for speed gives:
    let speed = gravity.sqrt() * range / (initial_height + range).sqrt();

    speed
}

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

/// For a moving target
///
/// Note: still returns a solution even if none exists for some reason.
/// make sure to check range first and clamp velocity to max if used.
pub fn solve_ballistic_arc_moving(
    proj_pos: Vec2,
    proj_speed: f32,
    target_pos: Vec2,
    target_velocity: Vec2,
    gravity: f32,
) -> (Vec2, Vec2, usize) {
    let mut s0 = Vec2::ZERO;
    let mut s1 = Vec2::ZERO;

    let g = gravity as f64;

    let a = proj_pos.x as f64;
    let b = proj_pos.y as f64;
    let m = target_pos.x as f64;
    let n = target_pos.y as f64;
    let p = target_velocity.x as f64;
    let q = target_velocity.y as f64;
    let s = proj_speed as f64;

    let h = m - a;
    let k = n - b;
    let l = -0.5 * g;

    let c0 = l * l;
    let c1 = -2.0 * q * l;
    let c2 = q * q - 2.0 * k * l - s * s + p * p;
    let c3 = 2.0 * k * q + 2.0 * h * p;
    let c4 = k * k + h * h;

    let mut t1 = 0.0;
    let mut t2 = 0.0;
    let mut t3 = 0.0;
    let mut t4 = 0.0;

    let num_times = solve_quartic(
        c0, c1, c2, c3, c4, &mut t1, &mut t2, &mut t3, &mut t4,
    );
    let mut times = [t1, t2, t3, t4];

    times.sort_by(|a, b| a.partial_cmp(b).unwrap());

    let mut solutions = [Vec2::ZERO; 2];
    let mut num_solutions = 0;

    for i in 0..times.len() {
        if num_solutions >= 2 {
            break;
        }

        let t = times[i];
        if t <= 0.0 || t.is_nan() {
            continue;
        }

        solutions[num_solutions].x = ((h + p * t) / t) as f32;
        solutions[num_solutions].y = ((k + q * t - l * t * t) / t) as f32;
        num_solutions += 1;
    }

    if num_solutions > 0 {
        s0 = solutions[0];
    }
    if num_solutions > 1 {
        s1 = solutions[1];
    }

    (s0, s1, num_solutions)
}
