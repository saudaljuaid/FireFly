//! Resolved, shaped text shared by line layout and SVG painting.
//!
//! Fonts are a fixed, redistributable set. No installed font is consulted:
//! bundled Latin/Arabic/Hebrew faces, then a bounded Chinese/Japanese face,
//! then the regular face's replacement character. Runs retain logical text;
//! glyphs have already been shaped into visual order, in CSS pixel units.
use std::collections::VecDeque;
use std::fmt::Write;
use std::sync::{Arc, Mutex, OnceLock};

use harfrust::{FontRef, ShapeOptions, ShaperData, UnicodeBuffer};
use unicode_segmentation::UnicodeSegmentation;

pub const REGULAR: &[u8] = include_bytes!("../assets/fonts/DejaVuSans.ttf");
pub const BOLD: &[u8] = include_bytes!("../assets/fonts/DejaVuSans-Bold.ttf");
pub const CJK: &[u8] = include_bytes!("../assets/fonts/PhosCjk-Regular.otf");
/// Each shaper invocation has a bounded source and glyph buffer.
pub const MAX_RUN_BYTES: usize = 65_536;
pub const MAX_GLYPHS: usize = 65_536;
pub const MAX_RESOLVED_GLYPHS: usize = 200_000;
/// Large sizes use smaller source chunks; glyph coordinates never saturate.
pub const MAX_RUN_ADVANCE: f32 = 250_000.0;
/// Fixed bundled faces retain at most 64 reusable shaping plans apiece.
pub const MAX_PLANS_PER_FACE: usize = 64;
const MAX_COORD: f32 = 1_000_000.0;
const MAX_OUTLINE_BYTES: usize = 65_536;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum FontFace {
    DejaVuRegular,
    DejaVuBold,
    Cjk,
}

impl FontFace {
    pub fn name(self) -> &'static str {
        match self {
            Self::DejaVuRegular => "DejaVu Sans",
            Self::DejaVuBold => "DejaVu Sans Bold",
            Self::Cjk => "Phos CJK",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Ltr,
    Rtl,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FontMetrics {
    pub ascent: f32,
    pub descent: f32,
    pub line_gap: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Glyph {
    pub id: u16,
    /// Position relative to the run's left edge and alphabetic baseline.
    pub x: f32,
    pub y: f32,
    pub advance: f32,
    /// UTF-8 byte offset in `ResolvedRun::content` (logical source order).
    pub cluster: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedRun {
    pub content: String,
    pub face: FontFace,
    pub size: f32,
    /// Requested weight; CJK has one regular face and never synthesizes bold.
    pub bold: bool,
    pub direction: Direction,
    pub advance: f32,
    pub ascent: f32,
    pub descent: f32,
    pub line_gap: f32,
    pub missing_glyphs: usize,
    pub glyphs: Vec<Glyph>,
}

struct FontData {
    shape_font: FontRef<'static>,
    shaper: ShaperData,
    outlines: ttf_parser::Face<'static>,
    ascii_coverage: [bool; 128],
    max_advance: f32,
    plans: Mutex<VecDeque<Arc<harfrust::ShapePlan>>>,
}

fn bytes(face: FontFace) -> &'static [u8] {
    match face {
        FontFace::DejaVuRegular => REGULAR,
        FontFace::DejaVuBold => BOLD,
        FontFace::Cjk => CJK,
    }
}

fn font(face: FontFace) -> Option<&'static FontData> {
    static REGULAR_FONT: OnceLock<Option<FontData>> = OnceLock::new();
    static BOLD_FONT: OnceLock<Option<FontData>> = OnceLock::new();
    static CJK_FONT: OnceLock<Option<FontData>> = OnceLock::new();
    let cell = match face {
        FontFace::DejaVuRegular => &REGULAR_FONT,
        FontFace::DejaVuBold => &BOLD_FONT,
        FontFace::Cjk => &CJK_FONT,
    };
    cell.get_or_init(|| {
        let shape_font = FontRef::new(bytes(face)).ok()?;
        let shaper = ShaperData::new(&shape_font);
        let outlines = ttf_parser::Face::parse(bytes(face), 0).ok()?;
        let ascii_coverage =
            std::array::from_fn(|index| outlines.glyph_index(char::from(index as u8)).is_some());
        let max_advance = (0..outlines.number_of_glyphs())
            .filter_map(|id| outlines.glyph_hor_advance(ttf_parser::GlyphId(id)))
            .max()
            .map_or(1.0, |advance| {
                f32::from(advance) / f32::from(outlines.units_per_em())
            });
        Some(FontData {
            shape_font,
            shaper,
            outlines,
            ascii_coverage,
            max_advance,
            plans: Mutex::new(VecDeque::new()),
        })
    })
    .as_ref()
}

fn bounded_size(size: f32) -> f32 {
    if size.is_finite() {
        size.clamp(0.0, 4096.0)
    } else {
        16.0
    }
}

pub fn face_metrics(face: FontFace, size: f32) -> FontMetrics {
    let size = bounded_size(size);
    let Some(font) = font(face) else {
        return FontMetrics {
            ascent: size * 0.8,
            descent: size * 0.2,
            line_gap: 0.0,
        };
    };
    let scale = size / f32::from(font.outlines.units_per_em());
    FontMetrics {
        ascent: (f32::from(font.outlines.ascender()) * scale).max(0.0),
        descent: (-f32::from(font.outlines.descender()) * scale).max(0.0),
        line_gap: (f32::from(font.outlines.line_gap()) * scale).max(0.0),
    }
}

pub fn metrics(size: f32, bold: bool) -> FontMetrics {
    face_metrics(
        if bold {
            FontFace::DejaVuBold
        } else {
            FontFace::DejaVuRegular
        },
        size,
    )
}

// Format controls, ZWJ/ZWNJ, variation selectors, and ASCII whitespace do not
// themselves require a visible glyph. The shaper handles these characters.
fn invisible(character: char) -> bool {
    character.is_ascii_control()
        || matches!(character as u32, 0x200b..=0x200f | 0x202a..=0x202e | 0x2060..=0x206f | 0xfe00..=0xfe0f | 0xe0100..=0xe01ef)
}

fn covered(face: FontFace, value: &str) -> bool {
    font(face).is_some_and(|font| {
        value
            .chars()
            .all(|character| invisible(character) || has_glyph(font, character))
    })
}

fn has_glyph(font: &FontData, character: char) -> bool {
    if character.is_ascii() {
        font.ascii_coverage[character as usize]
    } else {
        font.outlines.glyph_index(character).is_some()
    }
}

fn select_face(value: &str, bold: bool) -> FontFace {
    let preferred = if bold {
        FontFace::DejaVuBold
    } else {
        FontFace::DejaVuRegular
    };
    if covered(preferred, value) {
        preferred
    } else if bold && covered(FontFace::DejaVuRegular, value) {
        FontFace::DejaVuRegular
    } else if covered(FontFace::Cjk, value) {
        FontFace::Cjk
    } else {
        preferred
    }
}

fn shape_plan(
    font: &FontData,
    shaper: &harfrust::Shaper<'_>,
    buffer: &UnicodeBuffer,
) -> Arc<harfrust::ShapePlan> {
    // guess_segment_properties leaves script absent for common/inherited-only
    // buffers. Its public getter represents this as UNKNOWN. No CSS language,
    // custom features, or variable instance is supplied by this engine.
    let script = (buffer.script() != harfrust::script::UNKNOWN).then_some(buffer.script());
    let language = buffer.language();
    let key = harfrust::ShapePlanKey::new(script, buffer.direction()).language(language.as_ref());
    let mut plans = font
        .plans
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if let Some(plan) = plans.iter().find(|plan| key.matches(plan)) {
        return Arc::clone(plan);
    }
    let plan = Arc::new(harfrust::ShapePlan::new(
        shaper,
        buffer.direction(),
        script,
        language.as_ref(),
        &[],
    ));
    if plans.len() >= MAX_PLANS_PER_FACE {
        plans.pop_front();
    }
    plans.push_back(Arc::clone(&plan));
    plan
}

fn shape_face(
    value: &str,
    size: f32,
    bold: bool,
    rtl: bool,
    face: FontFace,
    context: (&str, &str),
) -> ResolvedRun {
    let size = bounded_size(size);
    let metrics = face_metrics(face, size);
    let mut run = ResolvedRun {
        content: value.to_owned(),
        face,
        size,
        bold,
        direction: if rtl { Direction::Rtl } else { Direction::Ltr },
        advance: 0.0,
        ascent: metrics.ascent,
        descent: metrics.descent,
        line_gap: metrics.line_gap,
        missing_glyphs: 0,
        glyphs: Vec::new(),
    };
    let Some(font) = font(face) else {
        // A damaged bundled face still produces finite, visible replacement
        // boxes, and never panics or consults an installed font.
        for (index, _) in value.grapheme_indices(true).take(MAX_GLYPHS) {
            let advance = size * 0.6;
            run.glyphs.push(Glyph {
                id: 0,
                x: run.advance,
                y: 0.0,
                advance,
                cluster: index,
            });
            run.advance += advance;
            run.missing_glyphs += 1;
        }
        return run;
    };
    let mut buffer = UnicodeBuffer::new();
    for (index, character) in value.char_indices() {
        let character = if !invisible(character) && !has_glyph(font, character) {
            run.missing_glyphs += 1;
            '\u{fffd}'
        } else {
            character
        };
        buffer.add(character, index as u32);
    }
    // HarfRust reads only its fixed five-codepoint context window, backwards
    // for pre-context and forwards for post-context. Passing borrowed paragraph
    // slices therefore never scans or copies the entire preceding paragraph.
    buffer.set_pre_context(context.0);
    buffer.set_post_context(context.1);
    buffer.set_direction(if rtl {
        harfrust::Direction::RightToLeft
    } else {
        harfrust::Direction::LeftToRight
    });
    buffer.guess_segment_properties();
    let shaper = font.shaper.shaper(&font.shape_font).build();
    let plan = shape_plan(font, &shaper, &buffer);
    let glyph_buffer = shaper.shape(buffer, ShapeOptions::new().plan(Some(&plan)));
    let scale = size / shaper.units_per_em() as f32;
    for (info, position) in glyph_buffer
        .glyph_infos()
        .iter()
        .zip(glyph_buffer.glyph_positions())
        .take(MAX_GLYPHS)
    {
        let advance = position.x_advance as f32 * scale;
        run.glyphs.push(Glyph {
            id: u16::try_from(info.glyph_id).unwrap_or(0),
            x: run.advance + position.x_offset as f32 * scale,
            y: -position.y_offset as f32 * scale,
            advance,
            cluster: info.cluster as usize,
        });
        run.advance += advance;
    }
    run
}

fn source_chunk_limit(face: FontFace, size: f32) -> usize {
    let max_advance = font(face).map_or(1.0, |font| font.max_advance);
    let width = bounded_size(size) * max_advance;
    if width <= 0.0 {
        MAX_RUN_BYTES
    } else {
        (MAX_RUN_ADVANCE / width)
            .floor()
            .clamp(4.0, MAX_RUN_BYTES as f32) as usize
    }
}

// Font advances give a conservative first chunk size. Shaping may adjust them
// or produce more glyphs, so verify the resolved result before placement. The
// fallback bisects only at whole graphemes, at most 16 levels for 64 KiB input.
fn shape_bounded(
    value: &str,
    size: f32,
    bold: bool,
    rtl: bool,
    face: FontFace,
    context: (&str, &str),
) -> Vec<ResolvedRun> {
    let run = shape_face(value, size, bold, rtl, face, context);
    if run.advance.is_finite()
        && (0.0..=MAX_RUN_ADVANCE).contains(&run.advance)
        && run.glyphs.iter().all(|glyph| {
            glyph.x.is_finite()
                && glyph.y.is_finite()
                && glyph.x.abs() <= MAX_COORD
                && glyph.y.abs() <= MAX_COORD
        })
    {
        return vec![run];
    }
    let boundaries: Vec<_> = value
        .grapheme_indices(true)
        .skip(1)
        .map(|(offset, _)| offset)
        .collect();
    if let Some(&at) = boundaries.get(boundaries.len() / 2) {
        let mut runs = shape_bounded(
            &value[..at],
            size,
            bold,
            rtl,
            face,
            (context.0, &value[at..]),
        );
        runs.extend(shape_bounded(
            &value[at..],
            size,
            bold,
            rtl,
            face,
            (&value[..at], context.1),
        ));
        runs
    } else {
        let mut replacement = shape_face("\u{fffd}", size, bold, rtl, face, ("", ""));
        replacement.content = value.to_owned();
        replacement.missing_glyphs = 1;
        vec![replacement]
    }
}

/// Resolve fallback at extended grapheme boundaries, retaining logical order.
/// The line builder orders these face runs visually after bidi analysis.
/// Very large runs are chunked at grapheme boundaries before shaping. A single
/// pathological cluster larger than 64 KiB or 250,000 CSS px becomes one replacement
/// cluster. Shaping coordinates are never silently clamped to the scene bound.
pub fn resolve(value: &str, size: f32, bold: bool, rtl: bool) -> Vec<ResolvedRun> {
    resolve_context(value, size, bold, rtl, "", "")
}

/// Preserve contextual joining through color-only style/node boundaries.
/// Callers omit context at line, font, direction, or decorated box boundaries.
pub fn resolve_context(
    value: &str,
    size: f32,
    bold: bool,
    rtl: bool,
    pre: &str,
    post: &str,
) -> Vec<ResolvedRun> {
    let mut runs = Vec::new();
    let mut start = 0;
    let mut end = 0;
    let mut face = None;
    let mut glyph_count = 0;
    for (index, grapheme) in value.grapheme_indices(true) {
        if glyph_count >= MAX_RESOLVED_GLYPHS {
            break;
        }
        let selected = select_face(grapheme, bold);
        if let Some(previous_face) = face
            && (previous_face != selected
                || index - start + grapheme.len() > source_chunk_limit(selected, size))
        {
            let context = (
                if start == 0 { pre } else { &value[..start] },
                &value[end..],
            );
            for run in shape_bounded(&value[start..end], size, bold, rtl, previous_face, context) {
                if glyph_count + run.glyphs.len() > MAX_RESOLVED_GLYPHS {
                    return runs;
                }
                glyph_count += run.glyphs.len();
                runs.push(run);
            }
            start = index;
        }
        face = Some(selected);
        if grapheme.len() > MAX_RUN_BYTES {
            let mut run = shape_face("\u{fffd}", size, bold, rtl, selected, ("", ""));
            run.content = grapheme.to_owned();
            run.missing_glyphs = 1;
            glyph_count += run.glyphs.len();
            runs.push(run);
            face = None;
            start = index + grapheme.len();
        }
        end = index + grapheme.len();
    }
    if let Some(face) = face
        && end > start
        && glyph_count < MAX_RESOLVED_GLYPHS
    {
        let context = (
            if start == 0 { pre } else { &value[..start] },
            if end == value.len() {
                post
            } else {
                &value[end..]
            },
        );
        for run in shape_bounded(&value[start..end], size, bold, rtl, face, context) {
            if glyph_count + run.glyphs.len() > MAX_RESOLVED_GLYPHS {
                break;
            }
            glyph_count += run.glyphs.len();
            runs.push(run);
        }
    }
    runs
}

pub fn width(value: &str, size: f32, bold: bool) -> f32 {
    resolve(value, size, bold, false)
        .iter()
        .map(|run| run.advance)
        .sum::<f32>()
        .clamp(0.0, MAX_COORD)
}

pub fn units_per_em(face: FontFace) -> f32 {
    font(face)
        .map(|font| f32::from(font.outlines.units_per_em()))
        .unwrap_or(1000.0)
}

#[derive(Default)]
struct Outline {
    path: String,
    overflowed: bool,
}

impl Outline {
    fn command(&mut self, command: std::fmt::Arguments<'_>) {
        if self.overflowed {
            return;
        }
        let before = self.path.len();
        let _ = self.path.write_fmt(command);
        if self.path.len() > MAX_OUTLINE_BYTES {
            self.path.truncate(before);
            self.overflowed = true;
        }
    }
}

impl ttf_parser::OutlineBuilder for Outline {
    fn move_to(&mut self, x: f32, y: f32) {
        self.command(format_args!("M{x:.1} {y:.1}"));
    }
    fn line_to(&mut self, x: f32, y: f32) {
        self.command(format_args!("L{x:.1} {y:.1}"));
    }
    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        self.command(format_args!("Q{x1:.1} {y1:.1} {x:.1} {y:.1}"));
    }
    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        self.command(format_args!(
            "C{x1:.1} {y1:.1} {x2:.1} {y2:.1} {x:.1} {y:.1}"
        ));
    }
    fn close(&mut self) {
        self.command(format_args!("Z"));
    }
}

/// Font-unit paths are shared by every size/position of a glyph in one SVG.
/// Empty outlines (spaces and format controls) deliberately return `None`.
pub fn glyph_outline(face: FontFace, glyph: u16) -> Option<String> {
    let Some(font) = font(face) else {
        return Some("M50 0V750H550V0ZM100 50H500V700H100Z".to_owned());
    };
    let mut outline = Outline::default();
    font.outlines
        .outline_glyph(ttf_parser::GlyphId(glyph), &mut outline)?;
    if outline.overflowed || outline.path.is_empty() {
        None
    } else {
        Some(outline.path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_buffer(value: &str, rtl: bool) -> UnicodeBuffer {
        let mut buffer = UnicodeBuffer::new();
        for (index, character) in value.char_indices() {
            buffer.add(character, index as u32);
        }
        buffer.set_direction(if rtl {
            harfrust::Direction::RightToLeft
        } else {
            harfrust::Direction::LeftToRight
        });
        buffer.guess_segment_properties();
        buffer
    }

    fn glyph_signature(buffer: &harfrust::GlyphBuffer) -> Vec<(u32, u32, i32, i32, i32, i32)> {
        buffer
            .glyph_infos()
            .iter()
            .zip(buffer.glyph_positions())
            .map(|(info, position)| {
                (
                    info.glyph_id,
                    info.cluster,
                    position.x_advance,
                    position.y_advance,
                    position.x_offset,
                    position.y_offset,
                )
            })
            .collect()
    }

    #[test]
    fn ascii_coverage_cache_matches_each_bundled_cmap() {
        for face in [FontFace::DejaVuRegular, FontFace::DejaVuBold, FontFace::Cjk] {
            let font = font(face).unwrap();
            for value in 0u8..=127 {
                let character = char::from(value);
                assert_eq!(
                    has_glyph(font, character),
                    font.outlines.glyph_index(character).is_some(),
                    "{face:?} U+{value:04X}"
                );
            }
        }
    }

    #[test]
    fn reusable_plans_match_uncached_shaping_and_warm_runs() {
        for (value, face, rtl) in [
            ("AVffi e\u{301}", FontFace::DejaVuRegular, false),
            ("AVffi e\u{301}", FontFace::DejaVuBold, false),
            ("سلام", FontFace::DejaVuRegular, true),
            ("שלום", FontFace::DejaVuBold, true),
            ("中文", FontFace::Cjk, false),
            (" () ", FontFace::DejaVuRegular, false),
        ] {
            let font = font(face).unwrap();
            let shaper = font.shaper.shaper(&font.shape_font).build();
            let uncached = shaper.shape(test_buffer(value, rtl), ShapeOptions::new());
            let buffer = test_buffer(value, rtl);
            let plan = shape_plan(font, &shaper, &buffer);
            let cached = shaper.shape(buffer, ShapeOptions::new().plan(Some(&plan)));
            assert_eq!(
                glyph_signature(&cached),
                glyph_signature(&uncached),
                "{value}"
            );
            let bold = face == FontFace::DejaVuBold;
            let cold = resolve(value, 17.0, bold, rtl);
            assert_eq!(cold, resolve(value, 17.0, bold, rtl), "{value}");
            assert_eq!(cold[0].face, face);
        }
    }

    #[test]
    fn plan_cache_is_bounded_and_eviction_preserves_active_plan() {
        // An isolated face makes eviction independent of parallel unit tests.
        let shape_font = FontRef::new(REGULAR).unwrap();
        let font = FontData {
            shaper: ShaperData::new(&shape_font),
            shape_font,
            outlines: ttf_parser::Face::parse(REGULAR, 0).unwrap(),
            ascii_coverage: std::array::from_fn(|index| {
                ttf_parser::Face::parse(REGULAR, 0)
                    .unwrap()
                    .glyph_index(char::from(index as u8))
                    .is_some()
            }),
            max_advance: 1.0,
            plans: Mutex::new(VecDeque::new()),
        };
        let shaper = font.shaper.shaper(&font.shape_font).build();
        let mut first_buffer = test_buffer("office", false);
        first_buffer.set_language("en-phos-cache-0".parse().unwrap());
        let first_plan = shape_plan(&font, &shaper, &first_buffer);
        for index in 1..70 {
            let mut buffer = test_buffer("office", false);
            buffer.set_language(format!("en-phos-cache-{index}").parse().unwrap());
            let plan = shape_plan(&font, &shaper, &buffer);
            assert!(Arc::ptr_eq(&shape_plan(&font, &shaper, &buffer), &plan));
        }
        let plans = font.plans.lock().unwrap();
        assert_eq!(plans.len(), MAX_PLANS_PER_FACE);
        assert!(!plans.iter().any(|plan| Arc::ptr_eq(plan, &first_plan)));
        drop(plans);
        let mut uncached_buffer = test_buffer("office", false);
        uncached_buffer.set_language("en-phos-cache-0".parse().unwrap());
        let uncached = shaper.shape(uncached_buffer, ShapeOptions::new());
        let evicted = shaper.shape(first_buffer, ShapeOptions::new().plan(Some(&first_plan)));
        assert_eq!(glyph_signature(&evicted), glyph_signature(&uncached));
    }

    #[test]
    fn metrics_cover_weights_and_graphemes() {
        assert!(width("WWW", 16.0, false) > width("iii", 16.0, false));
        assert_ne!(width("Hello", 16.0, false), width("Hello", 16.0, true));
        assert!((width("é", 16.0, false) - width("e\u{301}", 16.0, false)).abs() < 0.01);
        assert!(width("مرحبا 世界", 16.0, false).is_finite());
    }

    #[test]
    fn bundled_faces_parse_and_resolve_general_cjk_fallback() {
        for face in [FontFace::DejaVuRegular, FontFace::DejaVuBold, FontFace::Cjk] {
            assert!(font(face).is_some(), "{}", face.name());
        }
        let runs = resolve("Latin世界日本語", 16.0, true, false);
        assert_eq!(runs.len(), 2);
        assert_eq!(runs[0].face, FontFace::DejaVuBold);
        assert_eq!(runs[1].face, FontFace::Cjk);
        assert_eq!(runs[1].missing_glyphs, 0);
        assert_eq!(runs[1].glyphs.len(), 5);
        assert!((runs[1].advance - 80.0).abs() < 0.01);
    }

    #[test]
    fn arabic_joins_hebrew_orders_and_marks_keep_shaper_offsets() {
        let joined = resolve("سلام", 24.0, false, true);
        assert_eq!(joined.len(), 1);
        assert_eq!(joined[0].direction, Direction::Rtl);
        assert_eq!(joined[0].missing_glyphs, 0);
        assert!(joined[0].glyphs.len() < "سلام".chars().count());
        assert!(joined[0].glyphs[0].cluster > joined[0].glyphs.last().unwrap().cluster);
        let marked = resolve("q\u{301}\u{323}", 24.0, false, false);
        assert!(
            marked[0]
                .glyphs
                .iter()
                .any(|glyph| glyph.advance == 0.0 || glyph.y != 0.0)
        );
        let hebrew = resolve("שלום", 24.0, true, true);
        assert_eq!(hebrew[0].missing_glyphs, 0);
        assert!(hebrew[0].glyphs[0].cluster > hebrew[0].glyphs.last().unwrap().cluster);
    }

    #[test]
    fn contextual_arabic_color_boundaries_keep_joining() {
        let whole = resolve("كتب", 24.0, false, true);
        let first = resolve_context("ك", 24.0, false, true, "", "تب");
        let rest = resolve_context("تب", 24.0, false, true, "ك", "");
        let glyphs: Vec<_> = rest
            .iter()
            .chain(first.iter())
            .flat_map(|run| run.glyphs.iter().map(|glyph| glyph.id))
            .collect();
        assert_eq!(
            glyphs,
            whole[0]
                .glyphs
                .iter()
                .map(|glyph| glyph.id)
                .collect::<Vec<_>>()
        );
        assert!((first[0].advance + rest[0].advance - whole[0].advance).abs() < 0.01);
    }

    #[test]
    fn large_font_chunks_keep_measured_advances_and_unsaturated_positions() {
        let value = "W".repeat(3000);
        let runs = resolve(&value, 1024.0, false, false);
        assert!(runs.len() > 1);
        assert_eq!(
            runs.iter()
                .map(|run| run.content.as_str())
                .collect::<String>(),
            value
        );
        assert!(runs.iter().map(|run| run.advance).sum::<f32>() > MAX_COORD);
        for run in &runs {
            assert!(run.advance <= MAX_RUN_ADVANCE);
            let measured = run.glyphs.iter().map(|glyph| glyph.advance).sum::<f32>();
            assert!((measured - run.advance).abs() < 0.01);
            for glyphs in run.glyphs.windows(2) {
                assert!(glyphs[0].x + glyphs[0].advance <= glyphs[1].x + 0.01);
            }
            assert!(
                run.glyphs
                    .iter()
                    .all(|glyph| glyph.x + glyph.advance <= run.advance + 0.01)
            );
        }
        // An otherwise valid extended emoji cluster may contain hundreds of
        // unsupported/joined characters. Its allocated width remains bounded
        // without splitting the cluster or clamping every glyph onto one x.
        let cluster = format!("{}😀", "😀\u{200d}".repeat(500));
        let runs = resolve(&cluster, 1024.0, false, false);
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].content, cluster);
        assert_eq!(runs[0].glyphs.len(), 1);
        assert_eq!(runs[0].missing_glyphs, 1);
        assert!(runs[0].advance <= MAX_RUN_ADVANCE);
    }

    #[test]
    fn replacement_and_extreme_inputs_are_finite() {
        let missing = resolve("\u{10ffff}", 16.0, false, false);
        assert_eq!(missing[0].missing_glyphs, 1);
        assert!(glyph_outline(missing[0].face, missing[0].glyphs[0].id).is_some());
        assert!(resolve("", f32::NAN, false, true).is_empty());
        let long = "a".repeat(MAX_RUN_BYTES * 2 + 1);
        let runs = resolve(&long, f32::INFINITY, false, false);
        assert!(runs.len() >= 3);
        assert!(
            runs.iter()
                .all(|run| run.glyphs.len() <= MAX_GLYPHS && run.advance.is_finite())
        );
        let cluster = format!("a{}", "\u{301}".repeat(MAX_RUN_BYTES));
        let pathological = resolve(&cluster, 16.0, false, false);
        assert_eq!(pathological.len(), 1);
        assert_eq!(pathological[0].glyphs.len(), 1);
        assert_eq!(pathological[0].missing_glyphs, 1);
        let over_budget = resolve(
            &"x".repeat(MAX_RESOLVED_GLYPHS + MAX_RUN_BYTES),
            16.0,
            false,
            false,
        );
        assert!(
            over_budget
                .iter()
                .map(|run| run.glyphs.len())
                .sum::<usize>()
                <= MAX_RESOLVED_GLYPHS
        );
    }
}
