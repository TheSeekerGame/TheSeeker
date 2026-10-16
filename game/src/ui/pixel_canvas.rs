//! Pixel-art UI drawn on the CPU at art resolution and shown as one image.
//!
//! A canvas is a pure function of the state it presents: callers redraw it and
//! publish the result, which replaces the displayed image only when a pixel
//! changes. Icons and glyphs are copied at their native size, and the image
//! node spans a whole number of physical pixels per art pixel, so every UI
//! pixel lies on the gameplay pixel grid.
use bevy::asset::RenderAssetUsages;
use bevy::platform::collections::HashMap;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};

use super::pixel_font;
use crate::prelude::*;

/// Loaded images addressed by asset path.
pub(crate) type Art<'a> = HashMap<&'static str, &'a Image>;

/// Keeps the images that canvases draw loaded.
#[derive(Resource, Default)]
pub(crate) struct ArtLibrary(HashMap<&'static str, Handle<Image>>);

impl ArtLibrary {
    pub(crate) fn load(&mut self, server: &AssetServer, paths: impl IntoIterator<Item = &'static str>) {
        for path in paths {
            self.0.entry(path).or_insert_with(|| server.load(path));
        }
    }

    /// Whether every requested icon and bitmap glyph image is ready to draw.
    pub(crate) fn is_ready(&self, server: &AssetServer) -> bool {
        self.0.values().all(|handle| server.is_loaded_with_dependencies(handle.id()))
    }

    pub(crate) fn art<'a>(&self, images: &'a Assets<Image>) -> Art<'a> {
        self.0.iter().filter_map(|(path, handle)| Some((*path, images.get(handle)?))).collect()
    }
}

/// Integer rectangle from its top-left corner and size.
pub(crate) fn rect(x: i32, y: i32, width: i32, height: i32) -> IRect {
    IRect::new(x, y, x + width, y + height)
}

pub(crate) fn text_width(text: &str) -> i32 {
    pixel_font::text_width(text) as i32
}

pub(crate) struct Canvas<'a> {
    size: IVec2,
    pixels: Vec<[u8; 4]>,
    art: &'a Art<'a>,
}

impl<'a> Canvas<'a> {
    pub(crate) fn new(size: UVec2, art: &'a Art<'a>) -> Self {
        Self { size: size.as_ivec2(), pixels: vec![[0; 4]; (size.x * size.y) as usize], art }
    }

    pub(crate) fn fill(&mut self, area: IRect, color: Color) {
        let color = rgba(color);
        for point in points(area.intersect(IRect::from_corners(IVec2::ZERO, self.size))) {
            self.plot(point, color);
        }
    }

    /// Fills `area` with its corners cut diagonally by `cut` pixels and a one-pixel `border`.
    pub(crate) fn plate(&mut self, area: IRect, cut: i32, fill: Color, border: Color) {
        for y in area.min.y..area.max.y {
            let row = chamfered_row(area, cut, y);
            self.fill(row, fill);
            self.fill(rect(row.min.x, y, 1, 1), border);
            self.fill(rect(row.max.x - 1, y, 1, 1), border);
        }
        self.fill(chamfered_row(area, cut, area.min.y), border);
        self.fill(chamfered_row(area, cut, area.max.y - 1), border);
    }

    pub(crate) fn outline(&mut self, area: IRect, color: Color) {
        self.fill(IRect::new(area.min.x, area.min.y, area.max.x, area.min.y + 1), color);
        self.fill(IRect::new(area.min.x, area.max.y - 1, area.max.x, area.max.y), color);
        self.fill(IRect::new(area.min.x, area.min.y + 1, area.min.x + 1, area.max.y - 1), color);
        self.fill(IRect::new(area.max.x - 1, area.min.y + 1, area.max.x, area.max.y - 1), color);
    }

    /// Plots the `#` cells of a row-major mask.
    pub(crate) fn mask(&mut self, at: IVec2, rows: &[&str], color: Color) {
        let color = rgba(color);
        let cells = rows.iter().enumerate().flat_map(|(y, row)| {
            row.bytes().enumerate().filter(|(_, cell)| *cell == b'#').map(move |(x, _)| IVec2::new(x as i32, y as i32))
        });
        for cell in cells {
            self.plot(at + cell, color);
        }
    }

    /// Draws an image at its native size centered in `area`, multiplied by `tint`.
    pub(crate) fn image(&mut self, path: &str, area: IRect, tint: Color) {
        let tint = rgba(tint);
        self.shaded_image(path, area, |texel| multiply(texel, tint));
    }

    /// Draws an image at its native size centered in `area`, shading each texel.
    pub(crate) fn shaded_image(&mut self, path: &str, area: IRect, shade: impl Fn([u8; 4]) -> [u8; 4]) {
        let Some(image) = self.art.get(path).copied() else { return };
        let at = area.min + (area.size() - image.size().as_ivec2()) / 2;
        self.blit(image, URect::from_corners(UVec2::ZERO, image.size()), at, shade);
    }

    /// Copies the `source` part of an image to `at`, shading each texel.
    pub(crate) fn sprite(&mut self, path: &str, source: URect, at: IVec2, shade: impl Fn([u8; 4]) -> [u8; 4]) {
        if let Some(image) = self.art.get(path).copied() {
            self.blit(image, source, at, shade);
        }
    }

    /// Draws digits and `+` in the 3x5 micro font with a dark outline, ending just before `bottom_right`.
    pub(crate) fn number(&mut self, bottom_right: IVec2, text: &str, color: Color) {
        let glyphs = text.bytes().filter_map(|byte| MICRO_GLYPHS.get(usize::from(byte.checked_sub(b'+')?)).copied());
        let glyphs: Vec<_> = glyphs.flatten().collect();
        let at = bottom_right - IVec2::new(glyphs.len() as i32 * 4 - 1, 5);
        let outline = (-1..=1).flat_map(|y| (-1..=1).map(move |x| (IVec2::new(x, y), OUTLINE)));
        let passes = outline.chain([(IVec2::ZERO, color)]).flat_map(|(offset, color)| {
            glyphs.iter().enumerate().map(move |(index, glyph)| (at + offset + IVec2::X * 4 * index as i32, glyph, color))
        });
        for (at, glyph, color) in passes {
            self.mask(at, glyph, color);
        }
    }

    /// Draws one line whose capital letters start at `at.y` and returns its width.
    pub(crate) fn text(&mut self, at: IVec2, text: &str, color: Color) -> i32 {
        let Some(font) = self.art.get(pixel_font::FONT_IMAGE_PATH).copied() else { return text_width(text) };
        let mut x = at.x;
        let color = rgba(color);
        for glyph in text.chars().map(pixel_font::glyph_rect) {
            self.blit(font, glyph, IVec2::new(x, at.y - pixel_font::CAP_TOP as i32), |texel| multiply(texel, color));
            x += glyph.width() as i32;
        }
        x - at.x
    }

    /// Wraps `text` to `width`, draws at most `max_lines` lines `advance` pixels apart, and returns how many it drew.
    pub(crate) fn paragraph(&mut self, at: IVec2, width: i32, advance: i32, max_lines: usize, text: &str, color: Color) -> usize {
        let lines = pixel_font::wrap_text(text, width.max(0) as u32);
        let shown = lines.len().min(max_lines);
        for (index, line) in lines.iter().take(shown).enumerate() {
            self.text(at + IVec2::Y * advance * index as i32, line, color);
        }
        shown
    }

    pub(crate) fn finish(self) -> Raster {
        Raster { size: self.size.as_uvec2(), data: self.pixels.into_flattened() }
    }

    fn blit(&mut self, image: &Image, source: URect, at: IVec2, shade: impl Fn([u8; 4]) -> [u8; 4]) {
        let texels = points(source.as_irect())
            .filter_map(|point| Some((point, image.get_color_at(point.x as u32, point.y as u32).ok()?)));
        for (point, color) in texels {
            self.plot(at + point - source.min.as_ivec2(), shade(rgba(color)));
        }
    }

    fn plot(&mut self, at: IVec2, color: [u8; 4]) {
        if color[3] == 0 || at.cmplt(IVec2::ZERO).any() || at.cmpge(self.size).any() {
            return;
        }
        let pixel = &mut self.pixels[(at.y * self.size.x + at.x) as usize];
        *pixel = over(color, *pixel);
    }
}

const OUTLINE: Color = Color::srgb(0.02, 0.02, 0.04);

/// The 3x5 micro font from `+` through `9`; characters without a glyph are skipped.
const MICRO_GLYPHS: [Option<[&str; 5]>; 15] = [
    Some(["   ", " # ", "###", " # ", "   "]),
    None,
    None,
    None,
    None,
    Some(["###", "# #", "# #", "# #", "###"]),
    Some([" # ", "## ", " # ", " # ", "###"]),
    Some(["###", "  #", "###", "#  ", "###"]),
    Some(["###", "  #", " ##", "  #", "###"]),
    Some(["# #", "# #", "###", "  #", "  #"]),
    Some(["###", "#  ", "###", "  #", "###"]),
    Some(["###", "#  ", "###", "# #", "###"]),
    Some(["###", "  #", "  #", " # ", " # "]),
    Some(["###", "# #", "###", "# #", "###"]),
    Some(["###", "# #", "###", "  #", "###"]),
];

/// Multiplies a texel's color by `factor`; above one it brightens.
pub(crate) fn gain(texel: [u8; 4], factor: f32) -> [u8; 4] {
    let scale = |channel: u8| (f32::from(channel) * factor).round().min(255.0) as u8;
    [scale(texel[0]), scale(texel[1]), scale(texel[2]), texel[3]]
}

/// Adds `amount` of linear light to a texel's color.
pub(crate) fn lift(texel: [u8; 4], amount: f32) -> [u8; 4] {
    let [red, green, blue, alpha] = texel;
    let linear = Color::srgba_u8(red, green, blue, alpha).to_linear();
    let add = |channel: f32| (channel + amount).min(1.0);
    rgba(LinearRgba::new(add(linear.red), add(linear.green), add(linear.blue), linear.alpha).into())
}

/// Finished canvas pixels in `Rgba8UnormSrgb` layout.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Raster {
    pub size: UVec2,
    data: Vec<u8>,
}

impl Raster {
    pub(crate) fn into_image(self) -> Image {
        let size = Extent3d { width: self.size.x, height: self.size.y, depth_or_array_layers: 1 };
        Image::new(size, TextureDimension::D2, self.data, TextureFormat::Rgba8UnormSrgb, RenderAssetUsages::default())
    }

    /// Copies a visible rectangle without resampling its art pixels.
    pub(crate) fn window(self, offset: UVec2, size: UVec2) -> Self {
        let mut data = Vec::with_capacity((size.x * size.y * 4) as usize);
        for y in offset.y..offset.y + size.y {
            let first = ((y * self.size.x + offset.x) * 4) as usize;
            data.extend_from_slice(&self.data[first..first + (size.x * 4) as usize]);
        }
        Self { size, data }
    }

    /// Small edge arrows make an oversized panel's scroll direction visible.
    pub(crate) fn scroll_markers(&mut self, offset: UVec2, total: UVec2, color: Color) {
        if self.size.min_element() < 8 { return; }
        let width = self.size.x as i32;
        let height = self.size.y as i32;
        let arrows = [
            (offset.y > 0, IVec2::new(width / 2, 1), IVec2::Y),
            (offset.y + self.size.y < total.y, IVec2::new(width / 2, height - 2), -IVec2::Y),
            (offset.x > 0, IVec2::new(1, height / 2), IVec2::X),
            (offset.x + self.size.x < total.x, IVec2::new(width - 2, height / 2), -IVec2::X),
        ];
        let color = rgba(color);
        for (_, tip, inward) in arrows.into_iter().filter(|(shown, ..)| *shown) {
            self.scroll_arrow(tip, inward, color);
        }
    }

    fn scroll_arrow(&mut self, tip: IVec2, inward: IVec2, color: [u8; 4]) {
        let side = IVec2::new(inward.y, inward.x);
        let cells = (0..3).flat_map(|row| (-row..=row).map(move |column| (row, column)));
        for (row, column) in cells {
            let at = tip + inward * row + side * column;
            let index = ((at.y as u32 * self.size.x + at.x as u32) * 4) as usize;
            self.data[index..index + 4].copy_from_slice(&color);
        }
    }

    /// Replaces the image behind `handle` only when its pixels differ.
    pub(crate) fn publish(self, images: &mut Assets<Image>, handle: &Handle<Image>) {
        let unchanged = images.get(handle).is_some_and(|image| {
            image.size() == self.size && image.data.as_deref() == Some(self.data.as_slice())
        });
        if unchanged {
            return;
        }
        if let Some(mut image) = images.get_mut(handle) {
            *image = self.into_image();
        }
    }
}

fn points(area: IRect) -> impl Iterator<Item = IVec2> {
    (area.min.y..area.max.y).flat_map(move |y| (area.min.x..area.max.x).map(move |x| IVec2::new(x, y)))
}

fn chamfered_row(area: IRect, cut: i32, y: i32) -> IRect {
    let inset = (cut - (y - area.min.y).min(area.max.y - 1 - y)).max(0);
    IRect::new(area.min.x + inset, y, area.max.x - inset, y + 1)
}

fn rgba(color: Color) -> [u8; 4] {
    color.to_srgba().to_u8_array()
}

fn multiply(texel: [u8; 4], tint: [u8; 4]) -> [u8; 4] {
    std::array::from_fn(|channel| ((u16::from(texel[channel]) * u16::from(tint[channel]) + 127) / 255) as u8)
}

/// Composites `source` over `target` with straight alpha.
fn over(source: [u8; 4], target: [u8; 4]) -> [u8; 4] {
    if source[3] == u8::MAX {
        return source;
    }
    let alpha = f32::from(source[3]) / 255.0;
    let below = f32::from(target[3]) / 255.0 * (1.0 - alpha);
    let total = alpha + below;
    let mix = |channel: usize| {
        ((f32::from(source[channel]) * alpha + f32::from(target[channel]) * below) / total).round() as u8
    };
    [mix(0), mix(1), mix(2), (total * 255.0).round() as u8]
}

#[cfg(test)]
pub(crate) mod tests {
    use bevy::image::{CompressedImageFormats, ImageSampler, ImageType};

    use super::*;

    /// Decodes an image from `assets/` for canvas tests.
    pub(crate) fn load_art(path: &'static str) -> (&'static str, Image) {
        let bytes = std::fs::read(format!("{}/assets/{path}", env!("CARGO_MANIFEST_DIR"))).unwrap();
        let image = Image::from_buffer(&bytes, ImageType::Extension("png"), CompressedImageFormats::NONE, true,
            ImageSampler::Default, RenderAssetUsages::default()).unwrap();
        (path, image)
    }

    /// The directory `SEEKER_UI_SNAPSHOTS` names, where snapshot tests write panels as PNG files.
    pub(crate) fn snapshot_directory() -> Option<std::path::PathBuf> {
        let directory = std::path::PathBuf::from(std::env::var_os("SEEKER_UI_SNAPSHOTS")?);
        std::fs::create_dir_all(&directory).unwrap();
        Some(directory)
    }

    pub(crate) fn save_snapshot(directory: &std::path::Path, name: &str, raster: Raster) {
        raster.into_image().try_into_dynamic().unwrap().save(directory.join(format!("{name}.png"))).unwrap();
    }

    fn pixel(raster: &Raster, x: u32, y: u32) -> [u8; 4] {
        let start = ((y * raster.size.x + x) * 4) as usize;
        raster.data[start..start + 4].try_into().unwrap()
    }

    #[test]
    fn plates_cut_one_pixel_diagonals_and_keep_the_interior_filled() {
        let art = Art::default();
        let mut canvas = Canvas::new(UVec2::new(8, 8), &art);
        canvas.plate(rect(0, 0, 8, 8), 2, Color::BLACK, Color::WHITE);
        let raster = canvas.finish();
        assert_eq!(pixel(&raster, 0, 0), [0; 4]);
        assert_eq!(pixel(&raster, 1, 0), [0; 4]);
        assert_eq!(pixel(&raster, 2, 0), [255; 4]);
        assert_eq!(pixel(&raster, 1, 1), [255; 4]);
        assert_eq!(pixel(&raster, 0, 2), [255; 4]);
        assert_eq!(pixel(&raster, 3, 3), [0, 0, 0, 255]);
        assert_eq!(pixel(&raster, 7, 5), [255; 4]);
    }

    #[test]
    fn glyphs_keep_native_pixels_and_tint_white_exactly() {
        let (path, font) = load_art(pixel_font::FONT_IMAGE_PATH);
        let art = Art::from_iter([(path, &font)]);
        let mut canvas = Canvas::new(UVec2::new(8, 8), &art);
        let amber = Color::srgb_u8(245, 156, 31);
        assert_eq!(canvas.text(IVec2::new(1, 1), "I", amber), pixel_font::glyph_rect('I').width() as i32);
        let raster = canvas.finish();
        let lit = (0..8).flat_map(|y| (0..8).map(move |x| (x, y))).filter(|&(x, y)| pixel(&raster, x, y) == [245, 156, 31, 255]);
        assert!(lit.count() >= 6, "the capital I is at least one column of cap height");
    }
}
