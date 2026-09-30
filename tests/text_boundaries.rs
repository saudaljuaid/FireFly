use phos::dom::{Document, NodeKind};
use phos::layout::{BoxKind, Scene};
use phos::{css, html, layout, style, text};

fn scene(content: &str, rules: &str, width: f32) -> (Document, Scene) {
    let source = format!(
        "<style>html,body,p{{margin:0;padding:0}}p{{font-size:24px;line-height:32px}}{rules}</style><p>{content}</p>"
    );
    let document = html::parse(&source).unwrap();
    let styles = style::compute(&document, &css::parse(&document.stylesheets()));
    let scene = layout::layout(&document, &styles, width);
    (document, scene)
}

fn close(actual: f32, expected: f32) {
    assert!((actual - expected).abs() < 0.02, "{actual} != {expected}");
}

#[test]
fn cross_element_combining_cluster_is_shaped_and_wrapped_whole() {
    for width in [1.0, 320.0] {
        let (document, scene) = scene("<span>e</span><span>\u{301}</span>", "", width);
        assert_eq!(scene.runs.len(), 1);
        let run = &scene.runs[0];
        assert_eq!(run.text.content, "e\u{301}");
        let expected = text::resolve("e\u{301}", 24.0, false, false);
        assert_eq!(run.text.glyphs, expected[0].glyphs);
        close(run.text.advance, expected[0].advance);
        assert_eq!(scene.line_boxes.len(), 1);
        let NodeKind::Text(source) = &document.nodes[run.node].kind else {
            panic!("text origin")
        };
        assert_eq!(source, "e");
        assert!(run.source_range.end <= source.len());
        assert!(source.is_char_boundary(run.source_range.start));
        assert!(source.is_char_boundary(run.source_range.end));
    }
}

#[test]
fn padded_cross_element_cluster_keeps_one_shaped_ink_run() {
    for width in [1.0, 320.0] {
        let (document, scene) = scene(
            "<span class=base>e</span><span class=mark>\u{301}</span>",
            ".base{padding:0 7px;background:yellow}.mark{padding:0 11px;background:red}",
            width,
        );
        assert_eq!(scene.runs.len(), 1);
        let run = &scene.runs[0];
        assert_eq!(run.text.content, "e\u{301}");
        let expected = text::resolve("e\u{301}", 24.0, false, false);
        assert_eq!(run.text.glyphs, expected[0].glyphs);
        close(run.text.advance, expected[0].advance);
        let NodeKind::Text(source) = &document.nodes[run.node].kind else {
            panic!("text origin")
        };
        assert_eq!(source, "e");
        assert_eq!(run.source_range, 0..1);
        assert!(scene.boxes.iter().all(|geometry| geometry.x.is_finite()));
    }
}

#[test]
fn same_font_color_boundaries_preserve_arabic_joined_glyphs() {
    let (_, split) = scene(
        "<span class=ink>ك</span><span>تب</span>",
        "p{direction:rtl}.ink{color:red}",
        320.0,
    );
    let (_, whole) = scene("كتب", "p{direction:rtl}", 320.0);
    let glyph_ids = |scene: &Scene| {
        scene
            .runs
            .iter()
            .flat_map(|run| run.text.glyphs.iter().map(|glyph| glyph.id))
            .collect::<Vec<_>>()
    };
    assert_eq!(glyph_ids(&split), glyph_ids(&whole));
    close(split.line_boxes[0].advance, whole.line_boxes[0].advance);
    assert!(
        split
            .runs
            .iter()
            .all(|run| run.text.direction == text::Direction::Rtl)
    );
    assert_eq!(split.runs.len(), 2);
    assert!(split.runs[0].x + split.runs[0].text.advance <= split.runs[1].x + 0.02);
}

#[test]
fn rtl_inline_padding_stays_on_both_sides_of_its_ink() {
    let (document, scene) = scene(
        "left <span class=badge>שלום</span> right",
        ".badge{padding-left:7px;padding-right:11px;background:yellow}",
        640.0,
    );
    let badge = document
        .nodes
        .iter()
        .position(
            |node| matches!(&node.kind, NodeKind::Element(element) if element.has_class("badge")),
        )
        .unwrap();
    let fragment = scene
        .boxes
        .iter()
        .find(|geometry| geometry.node == Some(badge) && geometry.kind == BoxKind::InlineFragment)
        .unwrap();
    let run = scene
        .runs
        .iter()
        .find(|run| run.active_inline.contains(&badge))
        .unwrap();
    assert_eq!(run.text.direction, text::Direction::Rtl);
    close(run.x - fragment.x, 7.0);
    close(fragment.x + fragment.width - run.x - run.text.advance, 11.0);
}

#[test]
fn text_budget_discards_a_partial_trailing_grapheme() {
    let content = format!("safe q{}", "\u{301}".repeat(210_000));
    let (_, scene) = scene(&content, "", 320.0);
    assert!(scene.truncated);
    assert!(
        scene
            .runs
            .iter()
            .all(|run| !run.text.content.contains('q') && !run.text.content.contains('\u{301}'))
    );
    assert!(scene.runs.iter().any(|run| run.text.content == "safe"));
    assert!(scene.height.is_finite() && scene.primitives.len() <= 200_000);
}

#[test]
fn seamless_latin_and_cjk_wrap_at_the_shared_unicode_opportunity() {
    let width = text::width("AB中", 24.0, false) + 0.01;
    let (_, scene) = scene("AB中文EF", "", width);
    assert_eq!(scene.line_boxes.len(), 2);
    let lines: Vec<String> = scene
        .line_boxes
        .iter()
        .map(|line| {
            scene.runs[line.runs.clone()]
                .iter()
                .map(|run| run.text.content.as_str())
                .collect()
        })
        .collect();
    assert_eq!(lines, ["AB中", "文EF"]);
    assert!(scene.line_boxes.iter().all(|line| line.advance <= width));
    assert!(
        scene
            .runs
            .iter()
            .any(|run| run.text.face == text::FontFace::Cjk)
    );
    assert!(scene.runs.iter().all(|run| run.text.missing_glyphs == 0));
    let mut ascent: f32 = 0.0;
    let mut descent: f32 = 0.0;
    for face in [text::FontFace::DejaVuRegular, text::FontFace::Cjk] {
        let metrics = text::face_metrics(face, 24.0);
        let half_leading = (32.0 - metrics.ascent - metrics.descent) / 2.0;
        ascent = ascent.max(metrics.ascent + half_leading);
        descent = descent.max(metrics.descent + half_leading);
    }
    close(scene.line_boxes[0].height, ascent + descent);
    close(scene.line_boxes[1].height, ascent + descent);
    close(
        scene.line_boxes[1].baseline - scene.line_boxes[0].baseline,
        ascent + descent,
    );
}

#[test]
fn pre_line_collapses_tabs_and_spaces_around_preserved_newlines() {
    let (_, scene) = scene(
        " \t alpha \t  beta \n \t gamma \t  ",
        "p{white-space:pre-line}",
        320.0,
    );
    assert_eq!(scene.line_boxes.len(), 2);
    let lines: Vec<String> = scene
        .line_boxes
        .iter()
        .map(|line| {
            scene.runs[line.runs.clone()]
                .iter()
                .map(|run| run.text.content.as_str())
                .collect()
        })
        .collect();
    assert_eq!(lines, ["alpha beta", "gamma"]);
    assert!(
        scene
            .runs
            .iter()
            .all(|run| !run.text.content.contains('\t'))
    );
    close(scene.line_boxes[0].height, 32.0);
    close(scene.line_boxes[1].height, 32.0);
    close(
        scene.line_boxes[1].baseline - scene.line_boxes[0].baseline,
        32.0,
    );
}

#[test]
fn inline_embed_and_isolate_keep_internal_visual_order_and_logical_dom() {
    for mode in ["embed", "isolate"] {
        let (document, scene) = scene(
            "start <span class=direction>שלום 12</span> end",
            &format!(".direction{{direction:rtl;unicode-bidi:{mode}}}"),
            640.0,
        );
        let span = document.nodes.iter().position(|node| {
            matches!(&node.kind, NodeKind::Element(element) if element.has_class("direction"))
        }).unwrap();
        let runs: Vec<_> = scene
            .runs
            .iter()
            .filter(|run| run.active_inline.contains(&span))
            .collect();
        assert_eq!(
            runs.iter()
                .map(|run| run.text.content.as_str())
                .collect::<String>(),
            "12 שלום",
            "{mode}"
        );
        let digits = runs.iter().find(|run| run.text.content == "12").unwrap();
        let hebrew = runs.iter().find(|run| run.text.content == "שלום").unwrap();
        assert_eq!(digits.text.direction, text::Direction::Ltr);
        assert_eq!(hebrew.text.direction, text::Direction::Rtl);
        assert!(digits.x + digits.text.advance <= hebrew.x);
        let start = scene
            .runs
            .iter()
            .find(|run| run.text.content == "start")
            .unwrap();
        let end = scene
            .runs
            .iter()
            .find(|run| run.text.content == "end")
            .unwrap();
        assert!(start.x + start.text.advance <= digits.x);
        assert!(hebrew.x + hebrew.text.advance <= end.x);
        assert_eq!(scene.line_boxes.len(), 1);
        assert!(runs.iter().all(|run| run.baseline == start.baseline));
        let source: String = document.nodes[span]
            .children
            .iter()
            .filter_map(|&child| match &document.nodes[child].kind {
                NodeKind::Text(source) => Some(source.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(source, "שלום 12");
        assert!(scene.runs.iter().all(|run| {
            !run.text
                .content
                .chars()
                .any(|c| matches!(c, '\u{202b}' | '\u{202c}' | '\u{2067}' | '\u{2069}'))
        }));
    }
}

#[test]
fn ready_badge_fragment_includes_exact_padding_and_border_advances() {
    let document = html::parse(include_str!("render/inline_geometry.html")).unwrap();
    let styles = style::compute(&document, &css::parse(&document.stylesheets()));
    let scene = layout::layout(&document, &styles, 320.0);
    let badge = document
        .nodes
        .iter()
        .position(
            |node| matches!(&node.kind, NodeKind::Element(element) if element.has_class("badge")),
        )
        .unwrap();
    let fragments: Vec<_> = scene
        .boxes
        .iter()
        .filter(|geometry| geometry.node == Some(badge) && geometry.kind == BoxKind::InlineFragment)
        .collect();
    assert_eq!(fragments.len(), 1);
    let run = scene
        .runs
        .iter()
        .find(|run| run.active_inline.contains(&badge))
        .unwrap();
    assert_eq!(run.text.content, "Ready");
    close(fragments[0].width, text::width("Ready", 16.0, false) + 14.0);
    close(run.x - fragments[0].x, 7.0);
    close(
        fragments[0].x + fragments[0].width - run.x - run.text.advance,
        7.0,
    );
    close(fragments[0].height, 30.0);
}
