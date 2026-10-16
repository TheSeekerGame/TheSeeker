//! The controls reference, a pixel panel toggled by the controls action.
use leafwing_input_manager::prelude::ActionState;

use super::pixel_canvas::{rect, text_width, Art, ArtLibrary, Canvas, Raster};
use super::pixel_panel::{draw_frame, key_hint_width, keycap, show_raster, Accent, PanelScroll, UiPixelGrid, MUTED, PANEL_Z_INDEX, TEXT};
use crate::game::player::PlayerAction;
use crate::prelude::*;

const MARGIN: i32 = 7;
const ROW: i32 = 11;
const KEYS_LEFT: i32 = 66;
const ACCENT: Accent = Accent { bright: Color::srgb(0.80, 0.84, 0.88), dim: Color::srgb(0.24, 0.27, 0.32) };

/// Each action, its keys, and its gamepad buttons.
const CONTROLS: [(&str, &[&str], &str); 12] = [
    ("MOVE", &["A", "D", "<", ">"], "L STICK"),
    ("JUMP", &["W", "^", "SPACE"], "L2"),
    ("FALL", &["S", "v"], "D-PAD DOWN"),
    ("SKILLS", &["J", "K", "L", ";", "'"], "X R2 A B"),
    ("", &["1", "2", "3", "4", "5"], ""),
    ("UP VARIANT", &["I"], ""),
    ("INTERACT", &["F"], "Y"),
    ("INVENTORY", &["M"], ""),
    ("CONTROLS", &["C"], "START"),
    ("MENU", &["ESC"], ""),
    ("PAUSE", &["P"], ""),
    ("SCROLL UI", &["PGUP", "PGDN"], "SHIFT: SIDE"),
];

pub(super) fn plugin(app: &mut App) {
    app.init_resource::<ControlsShown>()
        .add_systems(OnEnter(AppState::InGame), |mut shown: ResMut<ControlsShown>| shown.0 = false)
        .add_systems(Update, (toggle_controls.after(GameTickSet::Post), present_controls.after(super::sync_ui_scale)).chain().run_if(in_state(AppState::InGame)));
}

#[derive(Resource, Default)]
struct ControlsShown(bool, PanelScroll);

#[derive(Component)]
struct ControlsPanel;

fn toggle_controls(actions: Query<&ActionState<PlayerAction>>, mut shown: ResMut<ControlsShown>) {
    if actions.single().is_ok_and(|actions| actions.just_pressed(&PlayerAction::ToggleControlOverlay)) {
        shown.0 = !shown.0;
    }
}

fn present_controls(
    mut commands: Commands,
    mut shown: ResMut<ControlsShown>,
    keys: Res<ButtonInput<KeyCode>>,
    library: Res<ArtLibrary>,
    mut images: ResMut<Assets<Image>>,
    mut panel: Query<(Entity, &ImageNode, &mut Node), With<ControlsPanel>>,
    grid: UiPixelGrid,
) {
    let content = shown.0.then(|| draw_controls(&library.art(&images))).and_then(|raster| {
        grid.scrollable(raster, &mut shown.1, &keys, None, Vec2::splat(0.5))
    });
    show_raster(&mut commands, &mut images, panel.single_mut().ok(), content, |image, node| {
        (Name::new("ControlsPanel"), ControlsPanel, image, node, ZIndex(PANEL_Z_INDEX), StateDespawnMarker)
    });
}

fn draw_controls(art: &Art) -> Raster {
    let keys_width = |keys: &[&str]| keys.iter().map(|key| key_hint_width(key, "") - 1).sum::<i32>();
    let pad_left = KEYS_LEFT + CONTROLS.iter().map(|(_, keys, _)| keys_width(keys)).max().unwrap_or(0) + 6;
    let width = pad_left + CONTROLS.iter().map(|(.., pad)| text_width(pad)).max().unwrap_or(0) + MARGIN;
    let height = 18 + CONTROLS.len() as i32 * ROW + 4;
    let mut canvas = Canvas::new(UVec2::new(width as u32, height as u32), art);
    draw_frame(&mut canvas, IVec2::new(width, height), ACCENT, &[14]);
    canvas.text(IVec2::new(MARGIN, 4), "CONTROLS", ACCENT.bright);
    canvas.text(IVec2::new(pad_left, 4), "GAMEPAD", MUTED);
    for (index, (action, keys, pad)) in CONTROLS.into_iter().enumerate() {
        let y = 18 + index as i32 * ROW;
        canvas.text(IVec2::new(MARGIN, y + 1), action, TEXT);
        let mut x = KEYS_LEFT;
        for key in keys {
            x += keycap(&mut canvas, IVec2::new(x, y), key) + 2;
        }
        canvas.text(IVec2::new(pad_left, y + 1), pad, MUTED);
    }
    canvas.fill(rect(pad_left - 4, 15, 1, height - 16), ACCENT.dim);
    canvas.finish()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::pixel_canvas::tests::{load_art, save_snapshot, snapshot_directory};
    use crate::ui::pixel_font::FONT_IMAGE_PATH;

    #[test]
    fn controls_systems_have_compatible_parameters() {
        let mut schedule = Schedule::default();
        schedule.add_systems((toggle_controls, present_controls));
        schedule.initialize(&mut World::new()).expect("controls systems initialize");
    }

    /// Writes the panel as a PNG file when `SEEKER_UI_SNAPSHOTS` names a directory.
    #[test]
    fn controls_snapshot() {
        let Some(directory) = snapshot_directory() else { return };
        let (path, font) = load_art(FONT_IMAGE_PATH);
        save_snapshot(&directory, "controls", draw_controls(&Art::from_iter([(path, &font)])));
    }
}
