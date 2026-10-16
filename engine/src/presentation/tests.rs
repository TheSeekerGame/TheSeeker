use super::*;

#[test]
fn refresh_rates_sample_the_same_timeline_without_accumulating_phase_error() {
    let mut history = TickHistory::default();
    for tick in 0..64 { history.push(tick, tick as f32); }
    for rate in [59.94, 60.0, 75.0, 120.0, 144.0] {
        for frame in 0..(rate * 0.6) as u64 {
            let position = frame as f64 * 96.0 / rate;
            let (a, b, alpha) = history.pair(position).unwrap();
            assert!((f64::from(a + (b - a) * alpha) - position).abs() < 0.00001);
        }
    }
    for position in [0.0, 0.9, 1.1, 4.99, 15.0, 15.3, 39.8, 62.7] {
        let (a, b, alpha) = history.pair(position).unwrap();
        assert!((f64::from(a + (b - a) * alpha) - position).abs() < 0.00001);
    }
}

#[test]
fn history_is_bounded_and_never_displays_a_pose_before_birth() {
    let mut history = TickHistory::default();
    history.push(10, 3);
    assert!(history.pair(9.999).is_none());
    assert_eq!(history.pair(10.0), Some((&3, &3, 0.0)));
    for tick in 11..200 { history.push(tick, tick); }
    assert_eq!(history.len(), 64);
    assert!(history.pair(100.0).is_none());
    history.push(199, 42);
    assert_eq!(history.pair(200.0), Some((&42, &42, 0.0)));
}

#[test]
fn delayed_sprite_preserves_teleport_and_despawn_boundaries() {
    let mut app = App::new();
    app.init_resource::<SpriteTracks>().init_resource::<PresentationDebug>();
    app.insert_resource(RenderInterpolation::at(4.5));
    app.add_systems(Update, present_sprites);
    let proxy = app.world_mut().spawn((PresentationProxy, Sprite::default(), Transform::default(), Visibility::Hidden)).id();
    let owner = app.world_mut().spawn_empty().id();
    let pose = SpritePose { sprite: Sprite::default(), transform: Transform::from_xyz(1.2, 2.6, 0.0),
        sprite_anchor: Anchor::BOTTOM_CENTER, visible: true, layers: RenderLayers::layer(2), pixel_aligned: true, alignment: PresentationAlignment::None, anchor: None, stepped: false, jump: false };
    let mut history = TickHistory::default();
    history.push(4, pose.clone());
    history.push(5, SpritePose { transform: Transform::from_xyz(100.3, 0.0, 0.0), layers: RenderLayers::layer(3), sprite_anchor: Anchor::TOP_LEFT, jump: true, ..pose });
    app.world_mut().resource_mut::<SpriteTracks>().tracks.insert(owner, SpriteTrack { proxy, history, dead_at: Some(6) });
    app.world_mut().despawn(owner);
    app.update();
    assert_eq!(app.world().get::<Transform>(proxy).unwrap().translation, Vec3::new(1.0, 3.0, 0.0));
    assert_eq!(app.world().get::<RenderLayers>(proxy), Some(&RenderLayers::layer(2)));
    assert_eq!(app.world().get::<Anchor>(proxy), Some(&Anchor::BOTTOM_CENTER));
    *app.world_mut().resource_mut::<RenderInterpolation>() = RenderInterpolation::at(5.0);
    app.update();
    assert_eq!(app.world().get::<Transform>(proxy).unwrap().translation, Vec3::new(100.0, 0.0, 0.0));
    assert_eq!(app.world().get::<RenderLayers>(proxy), Some(&RenderLayers::layer(3)));
    assert_eq!(app.world().get::<Anchor>(proxy), Some(&Anchor::TOP_LEFT));
    *app.world_mut().resource_mut::<RenderInterpolation>() = RenderInterpolation::at(6.0);
    app.update();
    assert!(app.world().get_entity(proxy).is_err());
}
