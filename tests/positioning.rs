use std::collections::HashSet;

use phos::dom::{Document, NodeId, NodeKind};
use phos::layout::{BoxGeometry, BoxKind, Primitive, Scene};
use phos::style::Color;
use phos::{css, html, layout, paint, style};

fn scene(source: &str, width: f32) -> (Document, Scene) {
    let document = html::parse(source).unwrap();
    let styles = style::compute(&document, &css::parse(&document.stylesheets()));
    let scene = layout::layout(&document, &styles, width);
    (document, scene)
}

fn node(document: &Document, class: &str) -> NodeId {
    document
        .nodes
        .iter()
        .position(
            |node| matches!(&node.kind, NodeKind::Element(element) if element.has_class(class)),
        )
        .unwrap()
}

fn geometry(scene: &Scene, node: NodeId) -> &BoxGeometry {
    scene
        .boxes
        .iter()
        .find(|item| item.node == Some(node) && item.kind == BoxKind::Element)
        .unwrap()
}

fn assert_box(scene: &Scene, node: NodeId, expected: (f32, f32, f32, f32)) {
    let item = geometry(scene, node);
    for (actual, expected) in [item.x, item.y, item.width, item.height]
        .into_iter()
        .zip([expected.0, expected.1, expected.2, expected.3])
    {
        assert!(
            (actual - expected).abs() < 0.001,
            "node {node}: {actual} != {expected}"
        );
    }
}

fn color_index(scene: &Scene, color: Color) -> usize {
    scene.primitives.iter().position(|primitive| matches!(primitive, Primitive::Box { background: Some(background), .. } if *background == color)).unwrap()
}

fn finite(scene: &Scene) {
    assert!(scene.width.is_finite() && scene.height.is_finite());
    assert!(scene.height <= 1_000_000.0);
    for item in &scene.boxes {
        for value in [item.x, item.y, item.width, item.height] {
            assert!(value.is_finite() && value.abs() <= 1_000_000.0);
        }
    }
    for run in &scene.runs {
        assert!(run.x.is_finite() && run.baseline.is_finite());
        assert!(run.x.abs() <= 1_000_000.0 && run.baseline.abs() <= 1_000_000.0);
    }
    for primitive in &scene.primitives {
        let values = match primitive {
            Primitive::Box {
                x,
                y,
                width,
                height,
                ..
            }
            | Primitive::DecoratedBox {
                x,
                y,
                width,
                height,
                ..
            }
            | Primitive::Image {
                x,
                y,
                width,
                height,
                ..
            }
            | Primitive::ClipStart {
                x,
                y,
                width,
                height,
                ..
            } => [*x, *y, *width, *height],
            Primitive::Text {
                x,
                baseline,
                width,
                size,
                ..
            } => [*x, *baseline, *width, *size],
            Primitive::ClipEnd => [0.0; 4],
        };
        for value in values {
            assert!(value.is_finite() && value.abs() <= 1_000_000.0);
        }
    }
}

#[test]
fn relative_translation_moves_descendants_and_preserves_reserved_flow() {
    for width in [320.0, 640.0, 900.0] {
        let (document, shifted) = scene(include_str!("render/relative.html"), width);
        assert_box(
            &shifted,
            node(&document, "shift"),
            (46.0, 34.0, 120.0, 40.0),
        );
        assert_box(&shifted, node(&document, "both"), (27.0, 68.0, 120.0, 30.0));
        assert_box(
            &shifted,
            node(&document, "following"),
            (22.0, 92.0, 220.0, 24.0),
        );
        let (_, unshifted) = scene(
            &include_str!("render/relative.html").replace("position:relative", "position:static"),
            width,
        );
        assert_eq!(
            geometry(&shifted, node(&document, "following")),
            geometry(&unshifted, node(&document, "following"))
        );
        assert_eq!(
            geometry(&shifted, node(&document, "parent")),
            geometry(&unshifted, node(&document, "parent"))
        );
        let shifted_text = shifted
            .runs
            .iter()
            .find(|run| run.text.content.starts_with("Shifted"))
            .unwrap();
        let original_text = unshifted
            .runs
            .iter()
            .find(|run| run.text.content.starts_with("Shifted"))
            .unwrap();
        assert!((shifted_text.x - original_text.x - 24.0).abs() < 0.001);
        assert!((shifted_text.baseline - original_text.baseline - 12.0).abs() < 0.001);
        finite(&shifted);
    }
}

#[test]
fn absolute_offsets_use_padded_containing_block_and_do_not_reserve_flow() {
    for width in [320.0, 640.0, 900.0] {
        let (document, positioned) = scene(include_str!("render/absolute.html"), width);
        assert_box(
            &positioned,
            node(&document, "parent"),
            (10.0, 10.0, 288.0, 198.0),
        );
        assert_box(
            &positioned,
            node(&document, "label"),
            (42.0, 33.0, 80.0, 30.0),
        );
        assert_box(
            &positioned,
            node(&document, "stretch"),
            (34.0, 166.0, 230.0, 26.0),
        );
        assert_box(
            &positioned,
            node(&document, "corner"),
            (246.0, 22.0, 40.0, 28.0),
        );
        assert_box(
            &positioned,
            node(&document, "flow"),
            (34.0, 34.0, 240.0, 32.0),
        );
        let paragraph = document
            .nodes
            .iter()
            .position(|node| matches!(&node.kind, NodeKind::Element(element) if element.tag == "p"))
            .unwrap();
        assert!((geometry(&positioned, paragraph).y - 234.0).abs() < 0.001);
        assert!(!positioned.truncated);
        finite(&positioned);
    }
}

#[test]
fn positioned_paint_groups_order_negative_auto_zero_positive_and_trap_descendants() {
    let (document, scene) = scene(include_str!("render/stacking.html"), 320.0);
    let colors = [
        Color(238, 242, 251, 255),
        Color(232, 118, 118, 255),
        Color(140, 187, 234, 255),
        Color(90, 195, 120, 179),
        Color(212, 166, 236, 255),
        Color(250, 205, 90, 204),
        Color(93, 66, 109, 255),
    ];
    let indices: Vec<_> = colors
        .into_iter()
        .map(|color| color_index(&scene, color))
        .collect();
    assert!(
        indices.windows(2).all(|pair| pair[0] < pair[1]),
        "{indices:?}"
    );
    assert_box(&scene, node(&document, "nested"), (82.0, 80.0, 85.0, 24.0));
    let svg = paint::to_svg(&scene);
    assert!(svg.contains("fill-opacity=\"0.702\""));
    assert!(svg.contains("fill-opacity=\"0.800\""));
    assert!(roxmltree::Document::parse(&svg).is_ok());
    finite(&scene);
}

#[test]
fn clipping_survives_absolute_and_relative_descendant_reordering() {
    let (document, scene) = scene(include_str!("render/position_clip.html"), 320.0);
    let clip = node(&document, "clip");
    assert_box(&scene, clip, (10.0, 10.0, 186.0, 106.0));
    let begin = scene
        .primitives
        .iter()
        .position(|primitive| matches!(primitive, Primitive::ClipStart { .. }))
        .unwrap();
    let end = scene
        .primitives
        .iter()
        .position(|primitive| matches!(primitive, Primitive::ClipEnd))
        .unwrap();
    let overlay_indices: Vec<_> = scene
        .primitives
        .iter()
        .enumerate()
        .filter_map(|(index, primitive)| {
            matches!(
                primitive,
                Primitive::Box {
                    background: Some(Color(226, 89, 104, 179)),
                    ..
                }
            )
            .then_some(index)
        })
        .collect();
    assert_eq!(overlay_indices.len(), 2);
    assert!(begin < overlay_indices[0] && overlay_indices[0] < end);
    assert!(end < overlay_indices[1]);
    match &scene.primitives[begin] {
        Primitive::ClipStart {
            x,
            y,
            width,
            height,
            radius,
        } => {
            assert_eq!((*x, *y, *width, *height), (13.0, 13.0, 180.0, 100.0));
            assert_eq!(*radius, [13.0; 4]);
        }
        _ => unreachable!(),
    }
    let svg = paint::to_svg(&scene);
    let xml = roxmltree::Document::parse(&svg).unwrap();
    let mut ids = HashSet::new();
    for id in xml.descendants().filter_map(|node| node.attribute("id")) {
        assert!(ids.insert(id));
    }
    assert_eq!(
        xml.descendants()
            .filter(|node| node.has_tag_name("clipPath"))
            .count(),
        1
    );
    finite(&scene);
}

#[test]
fn absolute_auto_height_percentages_nested_ancestors_and_static_fallback_are_explicit() {
    let source = "<style>html,body{margin:0}.outer{position:relative;width:200px;padding:10px;border:2px solid;height:auto}.flow{height:100px}.abs{position:absolute;left:50%;top:50%;width:50%;height:50%}.nested{position:absolute;right:0;bottom:0;width:10px;height:10px}.fallback{position:absolute;width:20px;height:10px}</style><div class=outer><div class=flow></div><div class=abs><div class=nested></div></div><div class=fallback></div></div>";
    let (document, scene) = scene(source, 500.0);
    assert_box(&scene, node(&document, "outer"), (0.0, 0.0, 224.0, 124.0));
    assert_box(&scene, node(&document, "abs"), (112.0, 62.0, 110.0, 60.0));
    assert_box(
        &scene,
        node(&document, "nested"),
        (212.0, 112.0, 10.0, 10.0),
    );
    assert_box(
        &scene,
        node(&document, "fallback"),
        (12.0, 12.0, 20.0, 10.0),
    );
    finite(&scene);
}

#[test]
fn positioned_direction_precedence_and_initial_indefinite_height_are_stable() {
    let source = "<style>html,body{margin:0}.outer{position:relative;direction:rtl;width:200px;height:100px}.relative{position:relative;left:20px;right:5px;top:8px;bottom:40px;width:40px;height:20px}.absolute{position:absolute;left:10px;right:20px;top:5px;bottom:10px;width:40px;height:20px}.initial{position:absolute;left:50%;top:50%;width:20px;height:20px}</style><div class=outer><div class=relative></div><div class=absolute></div></div><div class=initial></div>";
    let (document, scene) = scene(source, 400.0);
    assert_box(&scene, node(&document, "relative"), (-5.0, 8.0, 40.0, 20.0));
    assert_box(
        &scene,
        node(&document, "absolute"),
        (140.0, 5.0, 40.0, 20.0),
    );
    assert_box(&scene, node(&document, "initial"), (200.0, 0.0, 20.0, 20.0));
    finite(&scene);
}

#[test]
fn positioned_extreme_lengths_and_many_empty_boxes_have_bounded_output() {
    let extreme = "<style>html,body{margin:0}.parent{position:relative;width:16384px;height:16384px;padding:16384%;overflow:hidden}.child{position:absolute;left:16384%;top:16384%;width:16384%;height:16384%;margin:-16384%}</style><div class=parent><div class=child>Finite</div></div>";
    let (_, scene) = scene(extreme, 16_384.0);
    finite(&scene);
    let source = format!(
        "<style>html,body{{margin:0}}.child{{position:absolute;left:1px;top:1px;width:1px;height:1px;background:red}}</style>{}",
        "<i class=child></i>".repeat(10_000)
    );
    let (_, scene) = self::scene(&source, 320.0);
    assert_eq!(
        scene
            .primitives
            .iter()
            .filter(|primitive| matches!(
                primitive,
                Primitive::Box {
                    background: Some(Color(255, 0, 0, 255)),
                    ..
                }
            ))
            .count(),
        10_000
    );
    assert!(
        scene.boxes.len() <= 200_000
            && scene.primitives.len() <= 200_000
            && scene.runs.len() <= 200_000
    );
    assert!(!scene.truncated);
    finite(&scene);
}

#[test]
fn image_alt_fallback_runs_share_a_real_line_and_mixed_font_baseline() {
    let source = "<style>html,body,p{margin:0}</style><p><img class=only alt='Latin 中文 é'></p><p>Before <img class=mixed alt='missing label'> after</p>";
    let (document, scene) = scene(source, 320.0);
    for class in ["only", "mixed"] {
        let id = node(&document, class);
        let runs: Vec<_> = scene.runs.iter().filter(|run| run.node == id).collect();
        assert!(!runs.is_empty());
        let line = &scene.line_boxes[runs[0].line];
        for run in runs {
            assert_eq!(run.line, scene.runs[line.runs.start].line);
            assert_eq!(run.baseline, line.baseline);
            assert!(
                line.runs.contains(
                    &scene
                        .runs
                        .iter()
                        .position(|candidate| std::ptr::eq(candidate, run))
                        .unwrap()
                )
            );
            assert!(
                run.source_range.end
                    <= document
                        .element(id)
                        .unwrap()
                        .attribute("alt")
                        .unwrap()
                        .len()
            );
        }
    }
    let only_runs: Vec<_> = scene
        .runs
        .iter()
        .filter(|run| run.node == node(&document, "only"))
        .collect();
    assert!(
        only_runs
            .iter()
            .any(|run| run.text.face == phos::text::FontFace::Cjk)
    );
    assert!(
        only_runs
            .iter()
            .any(|run| run.text.face == phos::text::FontFace::DejaVuRegular)
    );
    finite(&scene);
}

#[test]
fn relative_inline_direction_does_not_override_containing_direction_precedence() {
    let source = "<style>html,body,p{margin:0}.container{direction:ltr;width:200px}.atomic{display:inline-block;position:relative;direction:rtl;left:20px;right:5px;width:40px;height:20px;background:red}.inline{position:relative;direction:rtl;left:20px;right:5px;background:blue}</style><p class=container><span class=atomic></span></p><p class=container><span class=inline>abc</span></p>";
    let (document, scene) = scene(source, 320.0);
    assert!((geometry(&scene, node(&document, "atomic")).x - 20.0).abs() < 0.001);
    let fragment = scene
        .boxes
        .iter()
        .find(|geometry| {
            geometry.node == Some(node(&document, "inline"))
                && geometry.kind == BoxKind::InlineFragment
        })
        .unwrap();
    assert!((fragment.x - 20.0).abs() < 0.001);
    finite(&scene);
}

#[test]
fn absolute_decoded_images_preserve_dimensions_ratio_and_explicit_html_sizes() {
    use base64::Engine;
    use phos::layout::ImageSource;

    let source = "<style>html,body{margin:0}.cb{position:relative;width:200px;height:140px;padding:10px;border:2px solid;overflow:hidden}.image{position:absolute;left:0;top:0}.css_height{height:40px}.css_width{width:80px}.border_box{height:52px;padding:4px;border:2px solid;box-sizing:border-box}.stretch{left:20px;right:30px;top:100px}.constrained{height:40px;max-width:70px}.both{height:40px}</style><div class=cb><img class='image css_height'><img class='image natural'><img class='image html_width' width=80><img class='image html_height' height=40><img class='image css_width'><img class='image border_box'><img class='image stretch'><img class='image wide_html' width=240><img class='image constrained'><img class='image both' width=20></div>";
    let document = html::parse(source).unwrap();
    let styles = style::compute(&document, &css::parse(&document.stylesheets()));
    let href = format!(
        "data:image/png;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(include_bytes!("render/two-pixels.png"))
    );
    let images: Vec<_> = document
        .nodes
        .iter()
        .map(|node| {
            matches!(&node.kind, NodeKind::Element(element) if element.tag == "img").then(|| {
                ImageSource {
                    href: href.clone(),
                    width: 2.0,
                    height: 1.0,
                }
            })
        })
        .collect();
    let scene = layout::layout_with_images(&document, &styles, &images, 320.0);
    for class in ["css_height", "html_width", "html_height", "css_width"] {
        assert_box(&scene, node(&document, class), (2.0, 2.0, 80.0, 40.0));
    }
    assert_box(&scene, node(&document, "natural"), (2.0, 2.0, 2.0, 1.0));
    assert_box(
        &scene,
        node(&document, "border_box"),
        (2.0, 2.0, 92.0, 52.0),
    );
    assert_box(
        &scene,
        node(&document, "stretch"),
        (22.0, 102.0, 170.0, 85.0),
    );
    assert_box(
        &scene,
        node(&document, "wide_html"),
        (2.0, 2.0, 240.0, 120.0),
    );
    // A width constraint may override the auto ratio-derived width while an
    // explicit height remains fixed, matching normal replaced-box sizing.
    assert_box(
        &scene,
        node(&document, "constrained"),
        (2.0, 2.0, 70.0, 40.0),
    );
    assert_box(&scene, node(&document, "both"), (2.0, 2.0, 20.0, 40.0));
    let start = scene
        .primitives
        .iter()
        .position(|primitive| matches!(primitive, Primitive::ClipStart { .. }))
        .unwrap();
    let end = scene
        .primitives
        .iter()
        .position(|primitive| matches!(primitive, Primitive::ClipEnd))
        .unwrap();
    let image_indices: Vec<_> = scene
        .primitives
        .iter()
        .enumerate()
        .filter_map(|(index, primitive)| {
            matches!(primitive, Primitive::Image { .. }).then_some(index)
        })
        .collect();
    assert_eq!(image_indices.len(), 10);
    assert!(
        image_indices
            .iter()
            .all(|&index| start < index && index < end)
    );
    match &scene.primitives[start] {
        Primitive::ClipStart {
            x,
            y,
            width,
            height,
            ..
        } => assert_eq!((*x, *y, *width, *height), (2.0, 2.0, 220.0, 160.0)),
        _ => unreachable!(),
    }
    assert!(scene.primitives.iter().any(|primitive| matches!(primitive, Primitive::Image { x, y, width, height, .. } if (*x,*y,*width,*height) == (8.0,8.0,80.0,40.0))));
    assert!(!scene.truncated);
    assert!(roxmltree::Document::parse(&paint::to_svg(&scene)).is_ok());
    finite(&scene);
}

#[test]
fn nested_atomic_translation_caps_run_line_and_anchor_coordinates() {
    let source = "<style>html,body{margin:0;font-size:1024px}.parent{padding-left:16384em}.atomic{display:inline-block;position:relative;padding-left:16384em}.overlay{position:absolute;left:0;top:0;width:1px;height:1px;background:red}</style><div class=parent><span class=atomic>x<span class=overlay></span></span></div>";
    let (document, scene) = scene(source, 320.0);
    if !scene.runs.iter().any(|run| run.text.content == "x") {
        assert!(scene.truncated);
    }
    for line in &scene.line_boxes {
        for value in [
            line.x,
            line.y,
            line.width,
            line.height,
            line.baseline,
            line.advance,
        ] {
            assert!(value.is_finite() && value.abs() <= 1_000_000.0, "{value}");
        }
    }
    for run in &scene.runs {
        assert!(run.line < scene.line_boxes.len());
    }
    let overlay = geometry(&scene, node(&document, "overlay"));
    assert!(overlay.x.abs() <= 1_000_000.0);
    finite(&scene);
}

#[test]
fn nested_inline_blocks_use_their_own_last_flow_baseline_and_images_use_bottom() {
    let source = "<style>html,body,p{margin:0}.outer{display:inline-block;width:120px}.inner{display:inline-block;height:60px;overflow:hidden}</style><p>before <span class=outer><span class=inner>inner</span> tail</span> after</p>";
    let (_, nested_scene) = scene(source, 320.0);
    let runs: Vec<_> = nested_scene
        .runs
        .iter()
        .filter(|run| {
            ["before", "tail", "after"]
                .iter()
                .any(|word| run.text.content.contains(word))
        })
        .collect();
    assert_eq!(runs.len(), 3);
    for run in runs {
        assert!(
            (run.baseline - 60.0).abs() < 0.001,
            "{}: {}",
            run.text.content,
            run.baseline
        );
        assert!((nested_scene.line_boxes[run.line].baseline - 60.0).abs() < 0.001);
    }
    let inner = nested_scene
        .runs
        .iter()
        .find(|run| run.text.content == "inner")
        .unwrap();
    assert!(inner.baseline < 20.0);
    assert_eq!(nested_scene.line_boxes.len(), 3);
    finite(&nested_scene);

    let (_, scene) = scene(
        "<style>html,body,p{margin:0}</style><p>before <img width=40 height=60 alt=label> after</p>",
        320.0,
    );
    for word in ["before", "after"] {
        let run = scene
            .runs
            .iter()
            .find(|run| run.text.content.contains(word))
            .unwrap();
        assert!((run.baseline - 60.0).abs() < 0.001);
    }
    let alt = scene
        .runs
        .iter()
        .find(|run| run.text.content == "label")
        .unwrap();
    assert!(alt.baseline < 20.0);
    finite(&scene);
}

#[test]
fn sole_zero_width_atomic_preserves_positive_height_but_empty_atomic_has_no_line() {
    for atomic in [
        "<span style='display:inline-block;width:0;height:40px'></span>",
        "<img width=0 height=40>",
    ] {
        let source = format!(
            "<style>html,body,p{{margin:0}}.after{{height:1px}}</style><p>{atomic}</p><div class=after></div>"
        );
        let (document, scene) = scene(&source, 320.0);
        assert_eq!(scene.line_boxes.len(), 1);
        let line = &scene.line_boxes[0];
        assert_eq!(
            (line.height, line.baseline, line.advance),
            (40.0, 40.0, 0.0)
        );
        assert_eq!(geometry(&scene, node(&document, "after")).y, 40.0);
        finite(&scene);
    }
    let source = "<style>html,body,p{margin:0}.after{height:1px}</style><p><span style='display:inline-block;width:0;height:0'></span></p><div class=after></div>";
    let (document, scene) = scene(source, 320.0);
    assert!(scene.line_boxes.is_empty());
    assert_eq!(geometry(&scene, node(&document, "after")).y, 0.0);
    finite(&scene);
}

#[test]
fn overwide_normal_and_nowrap_text_align_to_their_actual_advance() {
    for whitespace in ["normal", "nowrap"] {
        for alignment in ["left", "center", "right"] {
            let source = format!(
                "<style>html,body,p{{margin:0}}p{{width:1px;white-space:{whitespace};overflow-wrap:normal;text-align:{alignment}}}</style><p>wide</p>"
            );
            let (_, scene) = scene(&source, 320.0);
            assert_eq!(scene.line_boxes.len(), 1);
            let line = &scene.line_boxes[0];
            assert!(line.advance > line.width);
            let expected = match alignment {
                "center" => (line.width - line.advance) / 2.0,
                "right" => line.width - line.advance,
                _ => 0.0,
            };
            let run = scene
                .runs
                .iter()
                .find(|run| run.text.content == "wide")
                .unwrap();
            assert!(
                (run.x - expected).abs() < 0.001,
                "{whitespace}/{alignment}: {} != {expected}",
                run.x
            );
            finite(&scene);
        }
    }
}

#[test]
fn negative_positioned_descendants_follow_wrapped_inline_ancestor_backgrounds_inside_clips() {
    let source = "<style>html,body,p{margin:0}.cb{position:relative;width:75px;height:80px;overflow:hidden;border:2px solid;border-radius:8px;background:blue}.outer{background:red}.wrapped{background:#ffff0080;border:1px solid}.negative{position:absolute;left:0;top:0;width:50px;height:50px;background:green;z-index:-1}</style><div class=cb><span class=outer><span class=wrapped>one two three four five six<span class=negative></span></span></span></div>";
    let (document, scene) = scene(source, 320.0);
    let green = color_index(&scene, Color(0, 128, 0, 255));
    let blue = color_index(&scene, Color(0, 0, 255, 255));
    let clips: Vec<_> = scene
        .primitives
        .iter()
        .enumerate()
        .filter_map(|(index, primitive)| {
            matches!(primitive, Primitive::ClipStart { .. } | Primitive::ClipEnd).then_some(index)
        })
        .collect();
    assert_eq!(clips.len(), 2);
    assert!(blue < clips[0] && clips[0] < green && green < clips[1]);
    for (class, color) in [
        ("outer", Color(255, 0, 0, 255)),
        ("wrapped", Color(255, 255, 0, 128)),
    ] {
        let fragments: Vec<_> = scene
            .boxes
            .iter()
            .filter(|geometry| {
                geometry.node == Some(node(&document, class))
                    && geometry.kind == BoxKind::InlineFragment
            })
            .collect();
        assert!(fragments.len() >= 2);
        let painted: Vec<_> = scene.primitives.iter().enumerate().filter_map(|(index, primitive)| matches!(primitive, Primitive::Box { background: Some(background), .. } if *background == color).then_some(index)).collect();
        assert_eq!(painted.len(), fragments.len());
        assert!(
            painted
                .iter()
                .all(|&index| clips[0] < index && index < green)
        );
    }
    let first_ink = scene
        .primitives
        .iter()
        .position(|primitive| matches!(primitive, Primitive::Text { .. }))
        .unwrap();
    assert!(green < first_ink);
    let xml = paint::to_svg(&scene);
    assert!(roxmltree::Document::parse(&xml).is_ok());
    finite(&scene);
}

#[test]
fn many_empty_inline_boundaries_have_bounded_work_and_no_phantom_lines() {
    let source = format!(
        "<style>html,body,p{{margin:0}}</style><p>{}</p>",
        "<span></span>".repeat(50_000)
    );
    let (_, scene) = scene(&source, 320.0);
    assert!(!scene.truncated);
    assert!(scene.line_boxes.is_empty() && scene.runs.is_empty() && scene.primitives.is_empty());
    assert!(scene.boxes.len() <= 4);
    finite(&scene);
}

#[test]
fn nested_atomic_intrinsic_measurement_preserves_one_long_resolved_leaf() {
    let source = format!(
        "<style>html,body{{margin:0}}span{{display:inline-block;white-space:nowrap}}</style>{}{}{}",
        "<span>".repeat(64),
        "abcdefghi_".repeat(1_000),
        "</span>".repeat(64)
    );
    let (_, scene) = scene(&source, 900.0);
    assert!(!scene.truncated);
    assert_eq!(scene.line_boxes.len(), 65);
    assert_eq!(
        scene
            .runs
            .iter()
            .map(|run| run.text.content.len())
            .sum::<usize>(),
        10_000
    );
    assert_eq!(
        scene
            .runs
            .iter()
            .map(|run| run.text.glyphs.len())
            .sum::<usize>(),
        10_000
    );
    let baseline = scene.runs[0].baseline;
    assert!(
        scene
            .line_boxes
            .iter()
            .all(|line| (line.baseline - baseline).abs() < 0.001)
    );
    for run in &scene.runs {
        assert!(run.line < scene.line_boxes.len());
    }
    finite(&scene);
}

#[test]
fn saturated_clip_origins_and_absolute_anchors_share_the_same_cap() {
    let source = "<style>html,body{margin:0;font-size:1024px}.parent{padding-left:16384em;padding-top:976em}.clip{position:relative;width:20px;height:20px;border:1000px solid;overflow:hidden}.overlay{position:absolute;left:0;top:0;width:1px;height:1px;background:green}</style><div class=parent><div class=clip><span class=overlay></span></div></div>";
    let (document, scene) = scene(source, 320.0);
    let clip = scene
        .primitives
        .iter()
        .find_map(|primitive| match primitive {
            Primitive::ClipStart {
                x,
                y,
                width,
                height,
                ..
            } => Some((*x, *y, *width, *height)),
            _ => None,
        })
        .unwrap();
    assert_eq!(clip, (1_000_000.0, 1_000_000.0, 20.0, 20.0));
    assert_box(
        &scene,
        node(&document, "overlay"),
        (1_000_000.0, 1_000_000.0, 1.0, 1.0),
    );
    finite(&scene);
}

#[test]
fn huge_missing_image_alt_stops_at_a_coherent_coordinate_prefix() {
    let source = format!(
        "<style>html,body{{margin:0}}img{{display:block;width:200px;height:40px;font-size:1024px}}</style><img alt='{}'>",
        "W".repeat(4_000)
    );
    let (_, scene) = scene(&source, 320.0);
    assert!(scene.truncated);
    assert!(!scene.runs.is_empty());
    assert!(
        scene
            .runs
            .iter()
            .map(|run| run.text.content.len())
            .sum::<usize>()
            < 4_000
    );
    let mut end = 0.0;
    for run in &scene.runs {
        assert!((run.x - end).abs() < 0.1);
        end = run.x + run.text.advance;
        assert!(end <= 1_000_000.0);
        assert!(run.line < scene.line_boxes.len());
    }
    finite(&scene);
}

#[test]
fn accepted_inline_depth_limit_reports_truncation_but_hidden_depth_does_not() {
    for (depth, truncated) in [(250, false), (253, true)] {
        let source = format!(
            "<style>html,body{{margin:0}}</style>{}sentinel{}",
            "<span>".repeat(depth),
            "</span>".repeat(depth)
        );
        let (_, scene) = scene(&source, 320.0);
        assert_eq!(scene.truncated, truncated, "depth {depth}");
        assert_eq!(
            scene.runs.iter().any(|run| run.text.content == "sentinel"),
            !truncated
        );
        let svg = paint::to_svg(&scene);
        let xml = roxmltree::Document::parse(&svg).unwrap();
        assert_eq!(
            xml.root_element().attribute("data-phos-truncated"),
            truncated.then_some("true")
        );
        finite(&scene);
    }
    let source = format!(
        "<style>html,body{{margin:0}}.hidden{{display:none}}</style><p>visible</p><span class=hidden>{}hidden{}</span>",
        "<span>".repeat(252),
        "</span>".repeat(252)
    );
    let (_, scene) = scene(&source, 320.0);
    assert!(!scene.truncated);
    assert_eq!(
        scene
            .runs
            .iter()
            .map(|run| run.text.content.as_str())
            .collect::<String>(),
        "visible"
    );
    finite(&scene);
}

#[test]
fn inline_flow_item_cap_reports_a_bounded_empty_prefix() {
    let source = format!(
        "<style>html,body,p{{margin:0}}</style><p>{}</p>",
        "<span></span>".repeat(100_001)
    );
    let (_, scene) = scene(&source, 320.0);
    assert!(scene.truncated);
    assert!(scene.line_boxes.is_empty() && scene.runs.is_empty() && scene.primitives.is_empty());
    assert!(scene.boxes.len() <= 4);
    let svg = paint::to_svg(&scene);
    let xml = roxmltree::Document::parse(&svg).unwrap();
    assert_eq!(
        xml.root_element().attribute("data-phos-truncated"),
        Some("true")
    );
    finite(&scene);
}
