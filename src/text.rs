//! Deterministic advances for the bundled DejaVu Sans faces. This is a
//! deliberately narrow measurement model, not a Unicode shaping engine.
use std::sync::OnceLock;

use fontdue::{Font, FontSettings};
use unicode_segmentation::UnicodeSegmentation;

pub const REGULAR: &[u8] = include_bytes!("../assets/fonts/DejaVuSans.ttf");
pub const BOLD: &[u8] = include_bytes!("../assets/fonts/DejaVuSans-Bold.ttf");

fn font(bold: bool) -> &'static Font {
    static REGULAR_FONT: OnceLock<Font> = OnceLock::new();
    static BOLD_FONT: OnceLock<Font> = OnceLock::new();
    if bold {
        BOLD_FONT.get_or_init(|| {
            Font::from_bytes(BOLD, FontSettings::default()).expect("bundled bold font")
        })
    } else {
        REGULAR_FONT.get_or_init(|| {
            Font::from_bytes(REGULAR, FontSettings::default()).expect("bundled regular font")
        })
    }
}

pub fn width(value: &str, size: f32, bold: bool) -> f32 {
    let font = font(bold);
    let mut previous = None;
    let mut width = 0.0;
    for grapheme in value.graphemes(true) {
        let Some(character) = grapheme.chars().next() else {
            continue;
        };
        if let Some(left) = previous {
            width += font.horizontal_kern(left, character, size).unwrap_or(0.0);
        }
        let advance = if font.lookup_glyph_index(character) == 0 {
            // The SVG renderer's font fallback supplies the glyph. Keep its
            // allocated advance deterministic and fit the run with textLength.
            size * if (character as u32) >= 0x2e80 {
                1.0
            } else {
                0.62
            }
        } else {
            font.metrics(character, size).advance_width
        };
        width += advance;
        previous = Some(character);
    }
    width.clamp(0.0, 1_000_000.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn metrics_cover_weights_and_graphemes() {
        assert!(width("WWW", 16.0, false) > width("iii", 16.0, false));
        assert_ne!(width("Hello", 16.0, false), width("Hello", 16.0, true));
        assert!(width("e\u{301}", 16.0, false) <= width("ee", 16.0, false));
        assert!(width("مرحبا 世界", 16.0, false).is_finite());
    }
}
