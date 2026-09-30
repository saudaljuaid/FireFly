use phos::dom::{Document, NodeId, NodeKind};
use phos::layout::{BoxKind, Primitive, Scene};
use phos::{css, html, layout, paint, resource, style};

fn scene(source: &str, width: f32) -> (Document, Scene) {
    let document = html::parse(source).unwrap();
    let styles = style::compute(&document, &css::parse(&document.stylesheets()));
    let scene = layout::layout(&document, &styles, width);
    (document, scene)
}

fn paragraph(content: &str, declarations: &str, width: f32) -> (Document, Scene) {
    scene(
        &format!(
            "<style>html,body,p{{margin:0;padding:0}}p{{font-size:16px;line-height:24px;{declarations}}}</style><p>{content}</p>"
        ),
        width,
    )
}

fn element(document: &Document, class: &str) -> NodeId {
    document
        .nodes
        .iter()
        .position(
            |node| matches!(&node.kind, NodeKind::Element(element) if element.has_class(class)),
        )
        .unwrap()
}

fn close(actual: f32, expected: f32) {
    assert!((actual - expected).abs() < 0.02, "{actual} != {expected}");
}

fn assert_finite(scene: &Scene) {
    assert!(scene.height.is_finite() && (1.0..=1_000_000.0).contains(&scene.height));
    assert!(scene.primitives.len() <= 200_000);
    assert!(scene.runs.len() <= 200_000);
    for geometry in &scene.boxes {
        assert!(
            [geometry.x, geometry.y, geometry.width, geometry.height]
                .iter()
                .all(|value| value.is_finite())
        );
        assert!(geometry.width >= 0.0 && geometry.height >= 0.0);
    }
    for line in &scene.line_boxes {
        assert!(
            [
                line.x,
                line.y,
                line.width,
                line.height,
                line.baseline,
                line.advance
            ]
            .iter()
            .all(|value| value.is_finite())
        );
        assert!(line.height > 0.0 && line.advance >= 0.0);
        assert!(line.runs.start <= line.runs.end && line.runs.end <= scene.runs.len());
    }
    for run in &scene.runs {
        assert!(run.x.is_finite() && run.baseline.is_finite());
        assert!(run.line < scene.line_boxes.len());
    }
}

#[test]
fn resolved_run_source_ranges_and_painter_geometry_are_shared() {
    let (document, scene) = paragraph("Café é and <b>bold</b> text.", "", 320.0);
    assert_finite(&scene);
    assert!(!scene.runs.is_empty());
    for run in &scene.runs {
        let NodeKind::Text(source) = &document.nodes[run.node].kind else {
            panic!("text run must map to a text node");
        };
        assert!(run.source_range.end <= source.len());
        assert!(source.is_char_boundary(run.source_range.start));
        assert!(source.is_char_boundary(run.source_range.end));
        assert!(run.layout_range.start <= run.layout_range.end);
        close(
            run.text.glyphs.iter().map(|glyph| glyph.advance).sum(),
            run.text.advance,
        );
        assert!(run.text.ascent > 0.0 && run.text.descent >= 0.0);
        assert!(scene.primitives.iter().any(|primitive| {
            matches!(primitive, Primitive::Text { x, baseline, content, run: painted, .. }
                if content == &run.text.content && *x == run.x && *baseline == run.baseline && painted == &run.text)
        }));
    }
    let svg = paint::to_svg(&scene);
    let xml = roxmltree::Document::parse(&svg).unwrap();
    let ids: Vec<_> = xml
        .descendants()
        .filter_map(|node| node.attribute("id"))
        .collect();
    assert_eq!(
        ids.len(),
        ids.iter().collect::<std::collections::HashSet<_>>().len()
    );
    assert!(!svg.contains("NaN") && !svg.contains("inf"));
}

#[test]
fn bundled_fallback_faces_and_shaped_advances_agree() {
    let (_, scene) = paragraph("Latin <b>bold 中文</b> é العربية", "", 640.0);
    assert!(
        scene
            .runs
            .iter()
            .any(|run| run.text.face == phos::text::FontFace::DejaVuRegular)
    );
    assert!(
        scene
            .runs
            .iter()
            .any(|run| run.text.face == phos::text::FontFace::DejaVuBold)
    );
    assert!(
        scene
            .runs
            .iter()
            .any(|run| run.text.face == phos::text::FontFace::Cjk)
    );
    assert!(scene.runs.iter().all(|run| run.text.missing_glyphs == 0));
    for run in &scene.runs {
        close(
            run.text.glyphs.iter().map(|glyph| glyph.advance).sum(),
            run.text.advance,
        );
        assert!(run.text.glyphs.iter().all(|glyph| glyph.id != 0));
    }
    let (_, missing) = paragraph("unassigned &#x378;", "", 320.0);
    assert!(missing.runs.iter().any(|run| run.text.missing_glyphs == 1));
    assert_finite(&missing);
}

#[test]
fn bidi_runs_have_explicit_visual_order_and_preserve_logical_dom_text() {
    let source = include_str!("render/bidi.html");
    for width in [180.0, 640.0] {
        let (document, scene) = scene(source, width);
        let logical: String = document
            .nodes
            .iter()
            .filter_map(|node| match &node.kind {
                NodeKind::Text(text) => Some(text.as_str()),
                _ => None,
            })
            .collect();
        assert!(logical.contains("مرحبا بالعالم"));
        assert!(logical.contains("שלום עולם"));
        assert!(logical.contains("English 12"));
        assert!(
            scene
                .runs
                .iter()
                .any(|run| run.text.direction == phos::text::Direction::Rtl)
        );
        assert!(
            scene
                .runs
                .iter()
                .any(|run| run.text.direction == phos::text::Direction::Ltr)
        );
        assert!(
            scene
                .line_boxes
                .iter()
                .any(|line| line.base_direction == style::Direction::Rtl)
        );
        assert!(
            scene
                .runs
                .iter()
                .any(|run| run.text.bold && run.text.direction == phos::text::Direction::Rtl)
        );
        for line in &scene.line_boxes {
            let runs = &scene.runs[line.runs.clone()];
            for pair in runs.windows(2) {
                assert!(pair[0].visual_order < pair[1].visual_order);
                assert!(pair[0].x + pair[0].text.advance <= pair[1].x + 0.02);
                close(pair[0].baseline, pair[1].baseline);
            }
        }
        let svg = paint::to_svg(&scene);
        let xml = roxmltree::Document::parse(&svg).unwrap();
        assert!(xml.descendants().any(|node| node.has_tag_name("use")));
        assert!(!xml.descendants().any(|node| node.has_tag_name("text")));
        assert!(xml.descendants().any(|node| node.has_tag_name("title")
            && node.text().is_some_and(|text| text.contains("שלום"))));
        assert_finite(&scene);
    }
}

#[test]
fn normal_whitespace_collapses_across_adjacent_inline_elements() {
    let (_, scene) = paragraph("  alpha <b> beta </b><span>  gamma</span>   ", "", 320.0);
    let text: String = scene
        .runs
        .iter()
        .map(|run| run.text.content.as_str())
        .collect();
    assert_eq!(text, "alpha beta gamma");
    assert_eq!(scene.line_boxes.len(), 1);
    let (_, empty) = paragraph(" \t  <span></span> \n ", "", 320.0);
    assert!(empty.runs.is_empty());
    assert!(empty.line_boxes.is_empty());
    assert_finite(&empty);
}

#[test]
fn unicode_line_breaks_keep_cjk_and_combining_clusters() {
    let (_, cjk) = paragraph("中文中文中文", "", 35.0);
    assert_eq!(cjk.line_boxes.len(), 3);
    assert_eq!(
        cjk.runs
            .iter()
            .map(|run| run.text.content.as_str())
            .collect::<String>(),
        "中文中文中文"
    );
    for line in &cjk.line_boxes {
        assert!(line.advance <= 35.0);
    }
    let (_, combining) = paragraph("éééé", "", 1.0);
    assert_eq!(combining.line_boxes.len(), 4);
    assert!(combining.runs.iter().all(|run| run.text.content == "é"));
    assert_finite(&combining);
}

#[test]
fn nbsp_and_narrow_no_break_space_survive_normalization() {
    let (_, scene) = paragraph("A&nbsp;B C&#8239;D", "", 320.0);
    assert_eq!(
        scene
            .runs
            .iter()
            .map(|run| run.text.content.as_str())
            .collect::<String>(),
        "A\u{a0}B C\u{202f}D"
    );
    assert_eq!(scene.line_boxes.len(), 1);
}

#[test]
fn nowrap_and_pre_do_not_emergency_wrap() {
    for mode in ["nowrap", "pre"] {
        let (_, scene) = paragraph(
            "uninterrupted_identifier_and words",
            &format!("white-space:{mode}"),
            1.0,
        );
        assert_eq!(scene.line_boxes.len(), 1, "{mode}");
        assert!(scene.line_boxes[0].advance > 1.0);
        assert_finite(&scene);
    }
}

#[test]
fn preserved_newlines_create_empty_lines_with_inherited_height() {
    for mode in ["pre", "pre-wrap", "pre-line"] {
        let (_, scene) = paragraph(
            "first\n\nthird",
            &format!("white-space:{mode};line-height:28px"),
            320.0,
        );
        assert_eq!(scene.line_boxes.len(), 3, "{mode}");
        for line in &scene.line_boxes {
            close(line.height, 28.0);
        }
        close(scene.line_boxes[1].y - scene.line_boxes[0].y, 28.0);
        close(scene.line_boxes[2].y - scene.line_boxes[1].y, 28.0);
        assert!(scene.line_boxes[1].runs.is_empty());
    }
    let (_, scene) = paragraph("<br>one<br><br>three<br>", "line-height:28px", 320.0);
    assert_eq!(scene.line_boxes.len(), 4);
    assert!(scene.line_boxes[0].runs.is_empty() && scene.line_boxes[2].runs.is_empty());
}

#[test]
fn preserved_tabs_have_finite_advances_and_spaces_remain_visible() {
    let (_, scene) = paragraph("a\tb  c", "white-space:pre-wrap", 320.0);
    assert_eq!(scene.line_boxes.len(), 1);
    let text: String = scene
        .runs
        .iter()
        .map(|run| run.text.content.as_str())
        .collect();
    assert!(text.contains("  "));
    assert!(scene.line_boxes[0].advance > 32.0);
    assert_finite(&scene);
}

#[test]
fn font_size_and_weight_runs_share_a_sensible_baseline() {
    let (_, scene) = paragraph(
        "small <b>bold</b> <span style='font-size:32px;line-height:40px'>LARGE</span> small",
        "",
        640.0,
    );
    assert_eq!(scene.line_boxes.len(), 1);
    let line = &scene.line_boxes[0];
    assert!(line.height >= 40.0);
    assert!(scene.runs.iter().all(|run| run.baseline == line.baseline));
    assert_finite(&scene);
}

#[test]
fn text_alignment_uses_line_advance_at_narrow_and_wide_widths() {
    for width in [100.0, 400.0] {
        for (alignment, fraction) in [("left", 0.0), ("center", 0.5), ("right", 1.0)] {
            let (_, scene) = paragraph("Hi", &format!("text-align:{alignment}"), width);
            let line = &scene.line_boxes[0];
            close(scene.runs[0].x, (width - line.advance) * fraction);
        }
    }
}

#[test]
fn wrapped_inline_decorations_have_separate_finite_fragments() {
    let (document, scene) = scene(include_str!("render/inline_geometry.html"), 180.0);
    let link = element(&document, "link");
    let fragments: Vec<_> = scene
        .boxes
        .iter()
        .filter(|geometry| geometry.node == Some(link) && geometry.kind == BoxKind::InlineFragment)
        .collect();
    assert!(fragments.len() >= 2);
    assert!(
        fragments
            .iter()
            .all(|fragment| fragment.width > 0.0 && fragment.width <= 162.0)
    );
    assert!(fragments.windows(2).all(|pair| pair[0].y < pair[1].y));
    let badge = element(&document, "badge");
    assert_eq!(
        scene
            .boxes
            .iter()
            .filter(
                |geometry| geometry.node == Some(badge) && geometry.kind == BoxKind::InlineFragment
            )
            .count(),
        1
    );
    assert_finite(&scene);
}

#[test]
fn inline_image_bottom_matches_text_baseline() {
    let document = html::parse("<style>html,body,p{margin:0}p{font-size:16px;line-height:24px}img{width:24px;height:12px}</style><p>Before <img src=x> after</p>").unwrap();
    let styles = style::compute(&document, &css::parse(&document.stylesheets()));
    let image = document
        .nodes
        .iter()
        .position(|node| matches!(&node.kind, NodeKind::Element(element) if element.tag == "img"))
        .unwrap();
    let mut images = vec![None; document.nodes.len()];
    images[image] =
        Some(resource::decode_image(include_bytes!("render/two-pixels.png"), None).unwrap());
    let scene = layout::layout_with_images(&document, &styles, &images, 320.0);
    let geometry = scene
        .boxes
        .iter()
        .find(|geometry| geometry.node == Some(image) && geometry.kind == BoxKind::Element)
        .unwrap();
    close(geometry.width, 24.0);
    close(geometry.height, 12.0);
    close(geometry.y + geometry.height, scene.line_boxes[0].baseline);
    assert_finite(&scene);
}

#[test]
fn original_text_fixtures_stay_finite_at_three_viewports() {
    for source in [
        include_str!("render/unicode.html"),
        include_str!("render/bidi.html"),
        include_str!("render/inline_geometry.html"),
        include_str!("render/article.html"),
        include_str!("render/nav_cards.html"),
        include_str!("render/malformed_deep.html"),
    ] {
        for width in [320.0, 640.0, 900.0] {
            let (_, scene) = scene(source, width);
            assert_finite(&scene);
            assert!(!scene.truncated);
            assert!(!scene.runs.is_empty());
            roxmltree::Document::parse(&paint::to_svg(&scene)).unwrap();
        }
    }
}

#[test]
fn many_spans_and_long_uninterrupted_text_have_bounded_output() {
    let seed = include_str!("render/many_spans.html")
        .split_once("<p class=\"spans\">")
        .unwrap()
        .1
        .split_once("</p>")
        .unwrap()
        .0;
    let source = format!(
        "<style>html,body,p{{margin:0}}p{{font-size:16px;line-height:24px}}</style><p>{}</p>",
        seed.repeat(64)
    );
    let (_, spans) = scene(&source, 320.0);
    assert_finite(&spans);
    assert!(spans.runs.len() < 20_000);
    let seed = include_str!("render/long_token.html")
        .split_once("<p class=\"token\">")
        .unwrap()
        .1
        .split_once("</p>")
        .unwrap()
        .0;
    let source = format!(
        "<style>html,body,p{{margin:0}}p{{font-size:16px;line-height:24px}}</style><p>{}</p>",
        seed.repeat(100)
    );
    let (_, token) = scene(&source, 1.0);
    assert_finite(&token);
    assert!(token.truncated || token.runs.len() <= seed.len() * 100);
}

#[test]
fn repeated_malformed_css_and_nesting_preserve_parser_bounds() {
    let source = format!(
        "<style>{}</style>{}bounded text{}",
        "div{width:NaNpx;height:infpx;padding:bad;broken:'unterminated; } .live{color:#254f87}\n"
            .repeat(400),
        "<div>".repeat(240),
        "</div>".repeat(240)
    );
    for _ in 0..3 {
        let (_, scene) = scene(&source, 320.0);
        assert_finite(&scene);
    }
    assert!(html::parse(&"<div>".repeat(255)).is_err());
}

#[test]
fn global_text_work_cap_truncates_a_valid_clipped_document() {
    let source = format!(
        "<style>html,body{{margin:0}}div{{font-size:16px;line-height:24px;height:24px;overflow:hidden}}</style><div>{}</div>",
        "a".repeat(250_001)
    );
    let (_, scene) = scene(&source, 640.0);
    assert!(scene.truncated);
    assert_finite(&scene);
    let glyphs: usize = scene.runs.iter().map(|run| run.text.glyphs.len()).sum();
    assert!(glyphs <= 200_000);
    let svg = paint::to_svg(&scene);
    assert!(svg.len() <= 128 * 1024 * 1024);
    let xml = roxmltree::Document::parse(&svg).unwrap();
    let clips = xml
        .descendants()
        .filter(|node| node.has_tag_name("clipPath"))
        .count();
    let groups = xml
        .descendants()
        .filter(|node| node.has_tag_name("g") && node.attribute("clip-path").is_some())
        .count();
    assert!(clips > 0);
    assert_eq!(clips, groups);
}
