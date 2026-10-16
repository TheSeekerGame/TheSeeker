//! Metrics for the variable-width `PixelFont2.png` bitmap font.
//!
//! The atlas holds the 95 printable ASCII glyphs in three rows separated by
//! magenta guides. Each cell includes the glyph's dark shadow column, so glyph
//! widths already contain letter spacing.
use crate::prelude::*;

pub(crate) const FONT_IMAGE_PATH: &str = "font/PixelFont2.png";
/// Blank rows above the capital letters in every glyph cell.
pub(crate) const CAP_TOP: u32 = 2;
const GLYPH_HEIGHT: u32 = 11;
const LINE_ADVANCE: u32 = 12;
const FALLBACK_CHAR: char = '?';
#[cfg(test)]
const FONT_IMAGE_SIZE: UVec2 = UVec2::new(159, 37);

const ROW_0_BOUNDS: [u32; 34] = [
    0, 3, 6, 11, 18, 23, 28, 34, 37, 41, 45, 50, 55, 58, 63, 66, 71, 76,
    81, 86, 91, 96, 101, 106, 111, 116, 121, 124, 127, 131, 136, 140, 145,
    154,
];
const ROW_1_BOUNDS: [u32; 32] = [
    0, 5, 10, 15, 20, 25, 30, 35, 40, 45, 50, 55, 60, 67, 73, 78, 83, 88,
    93, 98, 103, 108, 113, 120, 125, 130, 135, 139, 144, 148, 153, 158,
];
const ROW_2_BOUNDS: [u32; 32] = [
    0, 3, 8, 13, 18, 23, 28, 33, 38, 43, 46, 50, 55, 58, 65, 70, 75, 80,
    85, 90, 95, 100, 105, 110, 117, 122, 127, 132, 137, 140, 145, 151,
];

/// The atlas cell of `character`, or of `?` for unsupported characters.
pub(crate) fn glyph_rect(character: char) -> URect {
    let supported = if (' '..='~').contains(&character) { character } else { FALLBACK_CHAR };
    let ascii = supported as u8;
    let (row, column, bounds): (u32, usize, &[u32]) = match ascii {
        32..=64 => (0, usize::from(ascii - 32), &ROW_0_BOUNDS),
        65..=95 => (1, usize::from(ascii - 65), &ROW_1_BOUNDS),
        _ => (2, usize::from(ascii - 96), &ROW_2_BOUNDS),
    };
    let min = UVec2::new(bounds[column] + 1, row * LINE_ADVANCE + 1);
    URect::from_corners(min, UVec2::new(bounds[column + 1], min.y + GLYPH_HEIGHT))
}

pub(crate) fn text_width(text: &str) -> u32 {
    text.chars().map(|character| glyph_rect(character).width()).sum()
}

/// Splits paragraphs at `\n` and wraps words to `max_width`, breaking words that do not fit a line.
#[expect(
    clippy::excessive_nesting,
    reason = "Word and character fallback wrapping share one compact state machine."
)]
pub(crate) fn wrap_text(text: &str, max_width: u32) -> Vec<String> {
    let mut lines = Vec::new();
    for paragraph in text.split('\n') {
        let mut current = String::new();
        for word in paragraph.split_whitespace() {
            let candidate = if current.is_empty() { word.to_owned() } else { format!("{current} {word}") };
            if text_width(&candidate) <= max_width {
                current = candidate;
                continue;
            }
            if !current.is_empty() {
                lines.push(std::mem::take(&mut current));
            }
            for character in word.chars() {
                if !current.is_empty() && text_width(&current) + glyph_rect(character).width() > max_width {
                    lines.push(std::mem::take(&mut current));
                }
                current.push(character);
            }
        }
        lines.push(current);
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_all_printable_ascii_inside_the_atlas_without_separator_pixels() {
        for character in ' '..='~' {
            let rect = glyph_rect(character);
            assert!(rect.min.x < rect.max.x);
            assert!(rect.max.x <= FONT_IMAGE_SIZE.x && rect.max.y <= FONT_IMAGE_SIZE.y);
            assert_eq!(rect.min.y % LINE_ADVANCE, 1);
        }
    }

    #[test]
    fn uses_variable_glyph_widths_and_question_mark_fallback() {
        assert_ne!(glyph_rect('i').width(), glyph_rect('W').width());
        assert_eq!(glyph_rect('é'), glyph_rect('?'));
    }

    #[test]
    fn wraps_on_words_and_explicit_newlines() {
        assert_eq!(wrap_text("AB CD", text_width("AB")), vec!["AB", "CD"]);
        assert_eq!(wrap_text("A\nB", 100), vec!["A", "B"]);
    }
}
