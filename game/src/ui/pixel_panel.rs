//! The shared look and handling of keyboard-driven pixel panels: palette, frames, item
//! wells, tabs, key hints, placement on the gameplay pixel grid, and navigation keys.
use bevy::ecs::system::SystemParam;
use bevy::ui::UiScale;
use bevy::window::PrimaryWindow;

use super::pixel_canvas::{rect, text_width, Art, Canvas, Raster};
use crate::camera::GameplayPixelScale;
use crate::prelude::*;

pub(crate) const PANEL_Z_INDEX: i32 = 23;
/// An item well: a 12-pixel icon with a one-pixel margin.
pub(crate) const WELL: i32 = 14;
/// Wells repeat every 16 pixels, leaving room for a selection outline.
pub(crate) const STRIDE: i32 = 16;
const PROMPT_HEIGHT: i32 = 13;

pub(crate) const FILL: Color = Color::srgba(0.016, 0.024, 0.039, 0.92);
pub(crate) const WELL_FILL: Color = Color::srgb(0.071, 0.082, 0.114);
pub(crate) const EMPTY_WELL: Color = Color::srgb(0.043, 0.051, 0.075);
pub(crate) const LINE: Color = Color::srgb(0.161, 0.188, 0.243);
pub(crate) const TEXT: Color = Color::srgb(0.94, 0.95, 0.91);
pub(crate) const MUTED: Color = Color::srgb(0.54, 0.57, 0.62);
pub(crate) const DISABLED: Color = Color::srgb(0.27, 0.29, 0.31);
/// Multiplies the icons of items that cannot be used now.
pub(crate) const UNAVAILABLE_ICON: Color = Color::srgb(0.5, 0.5, 0.55);
pub(crate) const GOOD: Color = Color::srgb(0.67, 0.89, 0.85);
pub(crate) const BAD: Color = Color::srgb(0.91, 0.30, 0.24);

/// A panel's highlight color and its dim variant for frames and rules.
#[derive(Clone, Copy)]
pub(crate) struct Accent {
    pub bright: Color,
    pub dim: Color,
}

/// A chamfered panel of `size` with full-width rules at the rows in `rules`.
pub(crate) fn draw_frame(canvas: &mut Canvas, size: IVec2, accent: Accent, rules: &[i32]) {
    canvas.plate(rect(0, 0, size.x, size.y), 2, FILL, accent.dim);
    for &y in rules {
        canvas.fill(rect(1, y, size.x - 2, 1), accent.dim);
    }
}

/// Text tabs from `at`; the active one is underlined and turns white while the tabs have focus.
pub(crate) fn draw_tabs(canvas: &mut Canvas, at: IVec2, labels: &[&str], active: usize, focused: bool, accent: Accent) {
    let mut x = at.x;
    for (index, label) in labels.iter().enumerate() {
        let color = match (index == active, focused) {
            (true, true) => TEXT,
            (true, false) => accent.bright,
            (false, _) => MUTED,
        };
        let width = canvas.text(IVec2::new(x, at.y), label, color);
        if index == active {
            canvas.fill(rect(x, at.y + 8, width - 1, 1), color);
        }
        x += width + 7;
    }
}

/// The well at `index` of a grid `columns` wide whose first well is at `origin`.
pub(crate) fn well_at(origin: IVec2, index: usize, columns: usize) -> IRect {
    let cell = IVec2::new((index % columns) as i32, (index / columns) as i32);
    rect(origin.x + cell.x * STRIDE, origin.y + cell.y * STRIDE, WELL, WELL)
}

/// A filled well; a selection outline occupies the gap around it.
pub(crate) fn draw_well(canvas: &mut Canvas, well: IRect, outline: Option<Color>) {
    canvas.fill(well, WELL_FILL);
    if let Some(color) = outline {
        canvas.outline(IRect::from_corners(well.min - 1, well.max + 1), color);
    }
}

/// A stack count in the bottom-right corner of an icon's `area`.
pub(crate) fn draw_count(canvas: &mut Canvas, area: IRect, count: u32) {
    let label = if count > 99 { "99+".to_owned() } else { count.to_string() };
    canvas.number(area.max, &label, TEXT);
}

fn keycap_width(key: &str) -> i32 {
    (text_width(key) + 2).max(7)
}

/// A key's name centered on a small plate; returns its width.
pub(crate) fn keycap(canvas: &mut Canvas, at: IVec2, key: &str) -> i32 {
    let width = keycap_width(key);
    canvas.plate(rect(at.x, at.y, width, 9), 1, WELL_FILL, LINE);
    canvas.text(at + IVec2::new(2 + (width - 2 - text_width(key)) / 2, 1), key, TEXT);
    width
}

pub(crate) fn key_hint_width(key: &str, label: &str) -> i32 {
    keycap_width(key) + 3 + text_width(label)
}

/// A keycap followed by what the key does.
pub(crate) fn key_hint(canvas: &mut Canvas, at: IVec2, key: &str, label: &str, color: Color) {
    let width = keycap(canvas, at, key);
    canvas.text(at + IVec2::new(width + 3, 1), label, color);
}

/// What the interact key does, on a plate of even width so it centers on the pixel grid.
pub(crate) fn draw_prompt(label: &str, color: Color, art: &Art) -> Raster {
    let content = key_hint_width("F", label);
    let width = (content + 7) & !1;
    let mut canvas = Canvas::new(UVec2::new(width as u32, PROMPT_HEIGHT as u32), art);
    canvas.plate(rect(0, 0, width, PROMPT_HEIGHT), 1, Color::BLACK, LINE);
    key_hint(&mut canvas, IVec2::new((width - content) / 2, 2), "F", label, color);
    canvas.finish()
}

/// The primary action, if any, from `left` and the closing key ending at `right`.
pub(crate) fn draw_key_hints(
    canvas: &mut Canvas,
    y: i32,
    (left, right): (i32, i32),
    action: Option<(&str, bool)>,
    close: &str,
    accent: Accent,
) {
    if let Some((label, ready)) = action {
        key_hint(canvas, IVec2::new(left, y), "F", label, if ready { accent.bright } else { MUTED });
    }
    key_hint(canvas, IVec2::new(right - key_hint_width("X", close), y), "X", close, MUTED);
}

/// Converts art pixels to logical UI pixels: one art pixel spans `divisor` physical pixels.
#[derive(SystemParam)]
pub(crate) struct UiPixelGrid<'w, 's> {
    pixel_scale: Res<'w, GameplayPixelScale>,
    ui_scale: Res<'w, UiScale>,
    pub window: Query<'w, 's, &'static Window, With<PrimaryWindow>>,
}

impl UiPixelGrid<'_, '_> {
    pub(crate) fn unit(&self) -> Option<f32> {
        let window = self.window.single().ok()?;
        Some(self.pixel_scale.divisor().max(1) as f32 / (window.scale_factor().max(f32::EPSILON) * self.ui_scale()))
    }

    pub(crate) fn ui_scale(&self) -> f32 {
        self.ui_scale.0.max(f32::EPSILON)
    }

    /// The UI's logical size: the gameplay viewport, which the UI camera lays the UI out in.
    pub(crate) fn area(&self) -> Option<Vec2> {
        Some(self.pixel_scale.visible_game_size() * self.unit()?)
    }

    /// Space available to a panel, leaving a one-pixel margin inside the viewport.
    pub(crate) fn available_art(&self) -> Option<UVec2> {
        Some((self.area()? / self.unit()?).floor().as_uvec2().saturating_sub(UVec2::splat(2)).max(UVec2::ONE))
    }

    pub(crate) fn cursor_art(&self) -> Option<Vec2> {
        let window = self.window.single().ok()?;
        let divisor = self.pixel_scale.divisor().max(1) as f32;
        let viewport_size = self.pixel_scale.visible_game_size() * divisor;
        let origin = (window.physical_size().as_vec2() - viewport_size) / 2.0;
        Some((window.cursor_position()? * window.scale_factor() - origin) / divisor - Vec2::ONE)
    }

    pub(crate) fn scrollable(
        &self, raster: Raster, scroll: &mut PanelScroll, keys: &ButtonInput<KeyCode>, focus: Option<IRect>, anchor: Vec2,
    ) -> Option<(Node, Raster)> {
        let raster = scroll.view(raster, self.available_art()?, keys, focus);
        Some((self.anchored(raster.size, anchor)?, raster))
    }

    /// A node of `size` art pixels on the physical pixel grid, placed in the UI by `anchor`:
    /// `(0.5, 0.5)` centers it and `(0.5, 1.0)` rests it on the bottom edge.
    pub(crate) fn anchored(&self, size: UVec2, anchor: Vec2) -> Option<Node> {
        let unit = self.unit()?;
        let size = size.as_vec2() * unit;
        let position = ((self.area()? - size) * anchor / unit).floor().max(Vec2::ZERO) * unit;
        Some(absolute_node(position, size))
    }
}

pub(crate) fn absolute_node(position: Vec2, size: Vec2) -> Node {
    Node {
        position_type: PositionType::Absolute,
        left: px(position.x),
        top: px(position.y),
        width: px(size.x),
        height: px(size.y),
        ..default()
    }
}

/// Updates the node showing a raster in place, spawns it, or despawns it when there is nothing to show.
pub(crate) fn show_raster<B: Bundle>(
    commands: &mut Commands,
    images: &mut Assets<Image>,
    shown: Option<(Entity, &ImageNode, Mut<Node>)>,
    content: Option<(Node, Raster)>,
    bundle: impl FnOnce(ImageNode, Node) -> B,
) {
    match (shown, content) {
        (Some((_, image, mut node)), Some((layout, raster))) => {
            node.set_if_neq(layout);
            raster.publish(images, &image.image);
        },
        (None, Some((layout, raster))) => {
            commands.spawn(bundle(ImageNode::new(images.add(raster.into_image())), layout));
        },
        (Some((entity, ..)), None) => commands.entity(entity).despawn(),
        (None, None) => {},
    }
}

pub(crate) fn horizontal_delta(keys: &ButtonInput<KeyCode>) -> i32 {
    axis_delta(keys, [KeyCode::KeyA, KeyCode::ArrowLeft], [KeyCode::KeyD, KeyCode::ArrowRight])
}

pub(crate) fn vertical_delta(keys: &ButtonInput<KeyCode>) -> i32 {
    axis_delta(keys, [KeyCode::KeyW, KeyCode::ArrowUp], [KeyCode::KeyS, KeyCode::ArrowDown])
}

fn axis_delta(keys: &ButtonInput<KeyCode>, negative: [KeyCode; 2], positive: [KeyCode; 2]) -> i32 {
    i32::from(keys.any_just_pressed(positive)) - i32::from(keys.any_just_pressed(negative))
}

pub(crate) fn close_pressed(keys: &ButtonInput<KeyCode>) -> bool {
    keys.any_just_pressed([KeyCode::KeyX, KeyCode::KeyQ, KeyCode::Escape])
}

/// Whether a key that navigates or acts in a panel was pressed.
pub(crate) fn panel_key_pressed(keys: &ButtonInput<KeyCode>) -> bool {
    keys.any_just_pressed([
        KeyCode::KeyA,
        KeyCode::ArrowLeft,
        KeyCode::KeyD,
        KeyCode::ArrowRight,
        KeyCode::KeyW,
        KeyCode::ArrowUp,
        KeyCode::KeyS,
        KeyCode::ArrowDown,
        KeyCode::KeyF,
    ])
}

/// A viewport into oversized pixel panels. Selection follows navigation; Page Up/Down
/// inspect the remaining content, and Shift pans horizontally, at the same native pixel scale.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct PanelScroll {
    pub(crate) offset: UVec2,
    focus: Option<IRect>,
    viewport: UVec2,
}

impl PanelScroll {
    pub(crate) fn view(&mut self, raster: Raster, available: UVec2, keys: &ButtonInput<KeyCode>, focus: Option<IRect>) -> Raster {
        let visible = raster.size.min(available.max(UVec2::ONE));
        let maximum = raster.size - visible;
        if let Some(focus) = focus.filter(|_| self.focus != focus || self.viewport != visible || panel_key_pressed(keys)) { self.reveal(focus, visible); }
        self.focus = focus;
        self.viewport = visible;
        let direction = i32::from(keys.just_pressed(KeyCode::PageDown)) - i32::from(keys.just_pressed(KeyCode::PageUp));
        if keys.any_pressed([KeyCode::ShiftLeft, KeyCode::ShiftRight]) {
            self.offset.x = self.offset.x.saturating_add_signed(direction * visible.x.saturating_sub(8).max(1) as i32);
        } else {
            self.offset.y = self.offset.y.saturating_add_signed(direction * visible.y.saturating_sub(8).max(1) as i32);
        }
        self.offset = self.offset.min(maximum);
        let total = raster.size;
        let mut raster = raster.window(self.offset, visible);
        raster.scroll_markers(self.offset, total, TEXT);
        raster
    }

    fn reveal(&mut self, focus: IRect, visible: UVec2) {
        let minimum = focus.min.max(IVec2::ZERO).as_uvec2();
        let maximum = focus.max.max(IVec2::ZERO).as_uvec2();
        self.offset = self.offset.min(minimum).max(maximum.saturating_sub(visible));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pixel_panel_scroll_exposes_content_without_rescaling_and_keeps_focus_visible() {
        let art = Art::default();
        let raster = Canvas::new(UVec2::new(226, 118), &art).finish();
        let mut scroll = PanelScroll::default();
        let mut keys = ButtonInput::default();
        let selected = rect(96, 90, 14, 14);
        let visible = scroll.view(raster.clone(), UVec2::new(100, 80), &keys, Some(selected));
        assert_eq!(visible.size, UVec2::new(100, 80));
        assert_eq!(scroll.offset, UVec2::new(10, 24));
        keys.press(KeyCode::PageDown);
        scroll.view(raster.clone(), UVec2::new(100, 80), &keys, Some(selected));
        assert_eq!(scroll.offset.y, 38);
        keys.reset_all();
        scroll.view(raster, UVec2::new(300, 200), &keys, Some(selected));
        assert_eq!(scroll.offset, UVec2::ZERO);
    }
}
