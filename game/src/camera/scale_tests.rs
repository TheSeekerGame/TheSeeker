use super::*;

#[test]
fn fits_common_windows() {
    for (window, divisor, visible) in [
        ((1920, 1080), 6, (320, 180)),
        ((2560, 1440), 7, (364, 204)),
        ((3440, 1440), 8, (430, 180)),
        ((1366, 768), 4, (340, 192)),
        ((2940, 1838), 9, (326, 204)),
    ] {
        let scale = GameplayPixelScale::fit(UVec2::from(window));
        assert_eq!((scale.divisor(), scale.visible_game_size()), (divisor, UVec2::from(visible).as_vec2()), "{window:?}");
    }
}

#[test]
fn prefers_larger_pixels_when_two_scales_miss_the_target_equally() {
    // At 672×1200, 224×400 and 168×300 game pixels miss the target by the same count.
    let scale = GameplayPixelScale::fit(UVec2::new(672, 1200));
    assert_eq!((scale.divisor(), scale.visible_game_size()), (4, Vec2::new(168.0, 300.0)));
}

#[test]
fn viewport_centers_an_even_count_of_whole_game_pixels() {
    for width in 1200..1300 {
        let physical_size = UVec2::new(width, width * 5 / 8);
        let scale = GameplayPixelScale::fit(physical_size);
        let pixels = scale.viewport_size / scale.divisor;
        assert_eq!(pixels * scale.divisor, scale.viewport_size);
        assert_eq!(pixels % 2, UVec2::ZERO, "odd game pixel count at {physical_size}");
        assert!(physical_size.cmplt(scale.viewport_size + 2 * scale.divisor).all());
        assert_eq!(scale.viewport_position, (physical_size - scale.viewport_size) / 2);
    }
}

#[test]
fn a_window_too_small_for_a_viewport_leaves_the_camera_the_whole_window() {
    // One screen pixel of height holds no pair of game pixels.
    assert!(GameplayPixelScale::fit(UVec2::new(1920, 1)).viewport().is_none());
}
