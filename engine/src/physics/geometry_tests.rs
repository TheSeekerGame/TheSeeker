use super::*;

#[test]
fn directional_probes_preserve_edge_and_center_distance_conventions() {
    let position = Vec2::new(-10.0, -10.0);
    let half = Vec2::new(2.0, 3.0);
    let obstacles = [
        (ProbeDirection::Down, Rect::from_corners(Vec2::new(-20.0, -20.0), Vec2::new(0.0, -13.0)), 0.0),
        (ProbeDirection::Up, Rect::from_corners(Vec2::new(-20.0, -7.0), Vec2::new(0.0, 0.0)), 3.0),
        (ProbeDirection::Left, Rect::from_corners(Vec2::new(-20.0, -20.0), Vec2::new(-12.0, 0.0)), 2.0),
        (ProbeDirection::Right, Rect::from_corners(Vec2::new(-8.0, -20.0), Vec2::new(0.0, 0.0)), 2.0),
    ];
    for (direction, rect, expected) in obstacles {
        assert_eq!(distance_to_rects(std::iter::once(rect), position, half, 50.0, direction, 0.0001), expected);
    }
}

#[test]
fn endpoint_resolution_keeps_edge_contact_zero_motion_and_both_signs() {
    let rect = Rect::from_corners(Vec2::ZERO, Vec2::splat(10.0));
    let half = Vec2::ONE;
    let cases = [(Vec2::new(-2.0, 5.0), 1.0, (-1.0, false)),
        (Vec2::new(-2.0, 5.0), 2.0, (-1.0, true)),
        (Vec2::new(12.0, 5.0), -2.0, (11.0, true)),
        (Vec2::new(5.0, 5.0), 0.0, (5.0, false))];
    for (position, delta, expected) in cases {
        assert_eq!(resolve_rects_axis(std::iter::once(rect), position, delta, half, Axis::X, 0.0001), expected);
    }
    // Ending beyond a thin obstacle is deliberately not a swept collision query.
    assert_eq!(resolve_rects_axis(std::iter::once(rect), Vec2::new(-2.0, 5.0), 20.0, half, Axis::X, 0.0001), (18.0, false));
}

#[test]
fn touching_only_at_a_corner_does_not_block() {
    let rect = Rect::from_corners(Vec2::ZERO, Vec2::splat(10.0));
    assert_eq!(resolve_rects_axis(std::iter::once(rect), Vec2::new(-2.0, -1.0), 3.0, Vec2::ONE, Axis::X, 0.0001), (1.0, false));
}
