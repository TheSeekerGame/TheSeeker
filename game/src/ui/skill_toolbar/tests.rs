use bevy::asset::RenderAssetUsages;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};

use super::*;
use crate::ui::pixel_canvas::tests::{load_art, save_snapshot, snapshot_directory};

fn slot(icon: &'static str) -> Slot {
    Slot { icon, pressed: false, gain: 1.0, shade: 0, energy: false, stock: None }
}

fn toolbar(slots: usize, skills: Vec<Slot>) -> Toolbar {
    Toolbar { slots, skills, health: 1.0, alignment: 1.0, elapsed_secs: 0.0, frame_pulse: 0.0 }
}

fn pixel(raster: Raster, at: IVec2) -> Srgba {
    raster.into_image().get_color_at(at.x as u32, at.y as u32).unwrap().to_srgba()
}

#[test]
fn toolbar_systems_have_compatible_parameters() {
    let mut schedule = Schedule::default();
    schedule.add_systems((spawn_toolbar, advance_toolbar_icons, present_toolbar, queue_level_up_captions, present_captions));
    schedule.initialize(&mut World::new()).expect("toolbar systems initialize");
}

#[test]
fn icons_stay_pressed_after_use_and_release_with_their_channel() {
    let mut state = IconState::default();
    state.advance(true, false, false, true, None);
    for _ in 1..PRESS_TICKS {
        assert!(state.pressed());
        state.advance(false, false, false, true, None);
    }
    assert!(state.pressed());
    state.advance(false, false, false, true, None);
    assert!(!state.pressed());

    state.advance(true, true, false, true, None);
    assert!(state.pressed());
    state.advance(false, false, false, true, None);
    assert!(!state.pressed(), "ending a channel releases the icon at once");
}

#[test]
fn becoming_usable_flashes_unless_the_skill_recharges_through_energy() {
    let mut state = IconState::default();
    state.advance(false, false, true, true, None);
    assert_eq!(state.gain(), UNAVAILABLE_GAIN);
    state.advance(false, false, false, true, None);
    assert_eq!(state.gain(), READY_FLASH_GAIN);

    let mut channeled = IconState::default();
    channeled.advance(false, false, true, false, Some(false));
    channeled.advance(false, false, false, false, Some(false));
    assert_eq!(channeled.gain(), 1.0);
    channeled.advance(false, false, false, false, Some(true));
    assert_eq!(channeled.gain(), READY_FLASH_GAIN, "filling up with energy flashes");

    let mut first_seen_full = IconState::default();
    first_seen_full.advance(false, false, false, false, Some(true));
    assert_eq!(first_seen_full.gain(), 1.0);
}

#[test]
fn cooldowns_shade_from_the_bottom_and_missing_energy_from_the_top() {
    let white = Image::new_fill(
        Extent3d { width: 12, height: 12, depth_or_array_layers: 1 },
        TextureDimension::D2,
        &[255; 4],
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::default(),
    );
    let art = Art::from_iter([("icon", &white)]);
    let (top, bottom) = (FIRST_SLOT, FIRST_SLOT + IVec2::Y * (ICON - 1));
    let cooling = toolbar(4, vec![Slot { shade: 3, ..slot("icon") }]);
    let draining = toolbar(4, vec![Slot { shade: 3, energy: true, ..slot("icon") }]);
    assert_eq!(pixel(draw_toolbar(&cooling, &art), top), Srgba::WHITE);
    assert_ne!(pixel(draw_toolbar(&cooling, &art), bottom), Srgba::WHITE);
    assert_ne!(pixel(draw_toolbar(&draining, &art), top), Srgba::WHITE);
    assert_eq!(pixel(draw_toolbar(&draining, &art), bottom), Srgba::WHITE);

    let pressed = toolbar(4, vec![Slot { pressed: true, ..slot("icon") }]);
    assert_eq!(pixel(draw_toolbar(&pressed, &art), top).alpha, 0.0, "a pressed icon gives up its outer ring");
}

#[test]
fn stock_labels_sit_where_the_toolbar_always_placed_them() {
    let node = stock_label_node(0, UVec2::new(72, FRAME_HEIGHT as u32), 5.0);
    assert_eq!((node.right, node.bottom), (px(259.0), px(47.0)));
}

#[test]
fn low_alignment_pulses_toward_the_warning_color() {
    assert_eq!(alignment_colors(0.5, 0.3), (ALIGNMENT_TRACK, ALIGNMENT_FILL));
    let peak = 0.25 / ALIGNMENT_LOW_PULSE_HZ;
    let distance = |fraction| {
        let (_, fill) = alignment_colors(fraction, peak);
        (fill.to_srgba().blue - ALIGNMENT_FILL.to_srgba().blue).abs()
    };
    assert!(distance(0.01) > distance(ALIGNMENT_LOW_THRESHOLD) && distance(ALIGNMENT_LOW_THRESHOLD) > 0.0);
    let (track, fill) = alignment_colors(0.0, 0.0);
    assert_ne!(track, ALIGNMENT_TRACK);
    assert_eq!(fill, ALIGNMENT_FILL);
}

#[test]
fn captions_wait_hold_and_fade() {
    let at = |age| Caption { lines: Vec::new(), age }.opacity();
    let near = |age, expected: f32| (at(age).unwrap() - expected).abs() < 1e-4;
    assert_eq!(at(0.0), None);
    assert!(near(CAPTION_DELAY_SECS + CAPTION_HOLD_SECS / 2.0, 1.0));
    assert!(near(CAPTION_DELAY_SECS + (CAPTION_HOLD_SECS + CAPTION_TOTAL_SECS) / 2.0, 0.5));
    assert!(near(CAPTION_DELAY_SECS + CAPTION_TOTAL_SECS, 0.0));
}

/// Writes representative toolbars and captions as PNG files when `SEEKER_UI_SNAPSHOTS` names a directory.
#[test]
fn toolbar_snapshots() {
    let Some(directory) = snapshot_directory() else { return };
    let skills = [SkillId::Attack, SkillId::Whirl, SkillId::Dash, SkillId::RedBerry, SkillId::TetherNode];
    let icons = skills.map(|skill| skill.icon_path_with_melee_weapon(PlayerMeleeWeapon::Sword));
    let paths = [frame_path(4), frame_path(5), FONT_IMAGE_PATH].into_iter().chain(icons);
    let art: Vec<_> = paths.map(load_art).collect();
    let art = Art::from_iter(art.iter().map(|(path, image)| (*path, image)));
    let slots = vec![
        Slot { pressed: true, ..slot(icons[0]) },
        Slot { shade: 7, energy: true, ..slot(icons[1]) },
        Slot { shade: 4, gain: UNAVAILABLE_GAIN, ..slot(icons[2]) },
        Slot { gain: READY_FLASH_GAIN, stock: Some(3), ..slot(icons[3]) },
        slot(icons[4]),
    ];
    let full = Toolbar { health: 0.62, alignment: 0.8, ..toolbar(5, slots.clone()) };
    let low = Toolbar { health: 0.18, alignment: 0.08, elapsed_secs: 0.17, frame_pulse: 1.0, ..toolbar(4, slots) };
    save_snapshot(&directory, "toolbar-five-slots", draw_toolbar(&full, &art));
    save_snapshot(&directory, "toolbar-low-alignment", draw_toolbar(&low, &art));
    let unlocked = ["ADAPTATION INCREASED".to_owned(), "NEW ABILITY UNLOCKED: WHIRLING STRIKE".to_owned()];
    let adapted = ["ADAPTATION INCREASED".to_owned()];
    save_snapshot(&directory, "captions", draw_captions(&[(&unlocked[..], 1.0), (&adapted[..], 0.5)], &art));
}
