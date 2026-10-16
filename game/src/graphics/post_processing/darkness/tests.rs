use super::*;

#[test]
fn light_atlas_gutters_cross_pages_and_empty_pages_share_storage() {
    let size = UVec2::new(atlas::PAGE_EDGE + 2, 2);
    let mut pixels = vec![0; (size.x * size.y) as usize];
    pixels[(atlas::PAGE_EDGE - 1) as usize] = 120;
    pixels[atlas::PAGE_EDGE as usize] = 240;
    let (pages, table) = atlas::pack(size, &pixels, 3).unwrap();
    let table = table.data.unwrap();
    assert_eq!(u32::from_le_bytes(table[0..4].try_into().unwrap()), 1);
    assert_eq!(u32::from_le_bytes(table[4..8].try_into().unwrap()), 2);
    let bytes = pages.data.unwrap();
    let edge = atlas::PAGE_EDGE as usize + 2;
    assert_eq!(bytes[edge * edge + edge + atlas::PAGE_EDGE as usize + 1], 240);
    assert_eq!(bytes[2 * edge * edge + edge], 120);
    assert!(atlas::pack(size, &pixels, 2).is_err());
    assert_eq!(atlas::pack(size, &vec![0; pixels.len()], 1).unwrap().0.texture_descriptor.size.depth_or_array_layers, 1);
}

/// Renders white with the camera at depths below where depth darkness starts, and checks the
/// darkness at each against its curve: none above the start, eased in over the ramp, then
/// deepening steadily.
#[test]
#[ignore = "requires a GPU adapter; run explicitly without a window or audio"]
fn darkness_eases_in_below_the_surface_then_deepens_steadily() {
    use crate::graphics::tests::{cleared_to, Headless};
    use crate::levelgen::SpawnSpec;
    use bevy::core_pipeline::{Core2d, Core2dSystems};

    // The capsule sits at the origin, above the camera's depths, which lie below the level
    // where no baked light reaches, and no player lights them.
    let level = CompiledLevel { width: 96, height: 96, bounds: Rect::new(0.0, 0.0, 576.0, 576.0),
        spawns: vec![SpawnSpec { kind: SpawnKind::Capsule, position: Vec2::ZERO, chunk: IVec2::ZERO }], ..default() };
    let (start, ramp) = depth_darkness(&level).unwrap();
    let mut headless = Headless::new(cleared_to(Color::WHITE), |app| {
        app.add_plugins(DarknessPlugin).insert_resource(level);
        app.sub_app_mut(RenderApp).add_systems(Core2d, darkness_pass.in_set(Core2dSystems::PostProcess));
    });
    let beyond = 1000.0;
    for (depth, darkness) in [
        (-100.0, 0.0),
        (ramp / 2.0, DARKNESS_DEPTH_BASE_STRENGTH / 2.0),
        (ramp + beyond, DARKNESS_DEPTH_BASE_STRENGTH + beyond * DARKNESS_DEPTH_STRENGTH_PER_WORLD_PX),
    ] {
        let world = headless.app.world_mut();
        let mut cameras = world.query_filtered::<&mut Transform, With<MainCamera>>();
        cameras.single_mut(world).unwrap().translation.y = start - depth;
        headless.run(20);
        let expected = (Color::linear_rgb(1.0 - darkness, 0.0, 0.0).to_srgba().red * 255.0).round();
        let rendered = f32::from(headless.center()[0]);
        assert!((rendered - expected).abs() <= 2.0, "{rendered} at depth {depth}, expected {expected}");
    }
}
