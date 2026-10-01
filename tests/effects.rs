use std::collections::HashSet;

use phos::effects::{BoxShadow, ColorStop, GradientDirection, LinearGradient};
use phos::layout::{Primitive, Scene};
use phos::style::{BorderStyle, Color, Length};
use phos::{css, html, layout, paint, style};

fn render(source: &str) -> (Scene, String) {
    let document = html::parse(&format!(
        "<style>html,body{{margin:0;padding:0}}</style>{source}"
    ))
    .unwrap();
    let styles = style::compute(&document, &css::parse(&document.stylesheets()));
    let scene = layout::layout(&document, &styles, 300.0);
    let svg = paint::to_svg(&scene);
    assert_valid_resources(&svg);
    (scene, svg)
}

fn assert_valid_resources(svg: &str) {
    let xml = roxmltree::Document::parse(svg).unwrap();
    let mut ids = HashSet::new();
    for node in xml.descendants() {
        if let Some(id) = node.attribute("id") {
            assert!(ids.insert(id), "duplicate {id}");
        }
    }
    for node in xml.descendants() {
        for name in ["fill", "filter", "mask", "clip-path"] {
            if let Some(reference) = node
                .attribute(name)
                .and_then(|value| value.strip_prefix("url(#"))
                .and_then(|value| value.strip_suffix(')'))
            {
                assert!(ids.contains(reference), "unresolved {reference}");
            }
        }
    }
    assert!(!svg.contains("NaN") && !svg.contains("Infinity"));
}

fn manual(primitives: Vec<Primitive>) -> Scene {
    Scene {
        width: 300.0,
        height: 300.0,
        boxes: Vec::new(),
        primitives,
        runs: Vec::new(),
        line_boxes: Vec::new(),
        truncated: false,
    }
}

fn decorated(gradient: Option<LinearGradient>, shadows: Vec<BoxShadow>) -> Primitive {
    Primitive::DecoratedBox {
        x: 20.0,
        y: 20.0,
        width: 100.0,
        height: 100.0,
        background: Some(Color::WHITE),
        border_color: Color::BLACK,
        border_width: [0.0; 4],
        border_style: BorderStyle::None,
        radius: [10.0; 4],
        gradient,
        shadows,
    }
}

#[test]
fn pinned_browser_probe_has_exact_geometry_and_self_contained_resources() {
    let source = include_str!("render/effects.html");
    let document = html::parse(source).unwrap();
    let styles = style::compute(&document, &css::parse(&document.stylesheets()));
    let scene = layout::layout(&document, &styles, 300.0);
    assert_eq!((scene.width, scene.height), (300.0, 360.0));
    assert!(!scene.truncated);
    for (id, expected) in [
        ("a", (20.0, 20.0, 100.0, 60.0)),
        ("b", (160.0, 20.0, 100.0, 60.0)),
        ("c", (20.0, 120.0, 100.0, 60.0)),
        ("d", (160.0, 120.0, 104.0, 64.0)),
        ("e", (20.0, 240.0, 100.0, 60.0)),
        ("f", (160.0, 240.0, 100.0, 60.0)),
    ] {
        let node = document
            .nodes
            .iter()
            .enumerate()
            .find(|(index, _)| {
                document
                    .element(*index)
                    .is_some_and(|element| element.attribute("id") == Some(id))
            })
            .unwrap()
            .0;
        let geometry = scene
            .boxes
            .iter()
            .find(|item| item.node == Some(node))
            .unwrap();
        assert_eq!(
            (geometry.x, geometry.y, geometry.width, geometry.height),
            expected,
            "{id}"
        );
    }
    let svg = paint::to_svg(&scene);
    assert_valid_resources(&svg);
    let xml = roxmltree::Document::parse(&svg).unwrap();
    for (tag, expected) in [
        ("linearGradient", 4),
        ("filter", 3),
        ("mask", 4),
        ("stop", 29),
    ] {
        assert_eq!(
            xml.descendants()
                .filter(|node| node.has_tag_name(tag))
                .count(),
            expected,
            "{tag}"
        );
    }
}

#[test]
fn gradients_overlay_color_and_precede_border_and_text() {
    let (_, svg) = render(
        "<div style='width:100px;height:40px;background-color:red;background-image:linear-gradient(to right,rgba(0,0,255,.5),transparent);border:2px solid green;border-radius:8px'>Label</div>",
    );
    let xml = roxmltree::Document::parse(&svg).unwrap();
    let paths: Vec<_> = xml
        .descendants()
        .filter(|node| {
            node.has_tag_name("path")
                && node
                    .ancestors()
                    .all(|ancestor| !ancestor.has_tag_name("defs"))
        })
        .collect();
    let background = paths
        .iter()
        .position(|node| node.attribute("fill") == Some("#ff0000"))
        .unwrap();
    let gradient = paths
        .iter()
        .position(|node| node.attribute("data-phos-gradient") == Some("true"))
        .unwrap();
    let border = paths
        .iter()
        .position(|node| node.attribute("stroke") == Some("#008000"))
        .unwrap();
    assert!(background < gradient && gradient < border);
    assert_eq!(
        paths[gradient]
            .attribute("d")
            .unwrap()
            .matches(" A ")
            .count(),
        4
    );
    assert!(svg.find("data-phos-gradient").unwrap() < svg.find("data-phos-text").unwrap());
}

#[test]
fn identical_geometry_reuses_gradients_at_different_positions() {
    let (_, svg) = render(
        "<div style='display:flex;gap:10px'><div style='width:100px;height:40px;background-image:linear-gradient(45deg,red 10px,blue)'></div><div style='width:100px;height:40px;background-image:linear-gradient(45deg,red 10px,blue)'></div></div>",
    );
    let xml = roxmltree::Document::parse(&svg).unwrap();
    assert_eq!(
        xml.descendants()
            .filter(|node| node.has_tag_name("linearGradient"))
            .count(),
        1
    );
    let fills: Vec<_> = xml
        .descendants()
        .filter(|node| node.attribute("data-phos-gradient").is_some())
        .map(|node| node.attribute("fill").unwrap())
        .collect();
    assert_eq!(fills.len(), 2);
    assert_eq!(fills[0], fills[1]);
    let transforms: Vec<_> = xml
        .descendants()
        .filter(|node| node.attribute("data-phos-gradient").is_some())
        .map(|node| node.parent().unwrap().attribute("transform").unwrap())
        .collect();
    assert_ne!(transforms[0], transforms[1]);
}

#[test]
fn out_of_range_and_coincident_stops_preserve_defined_transitions() {
    let (_, svg) = render(
        "<div style='width:100px;height:20px;background:linear-gradient(to right,red -50%,white,blue 150%)'></div><div style='width:100px;height:20px;background:linear-gradient(to right,red 50%,blue 50%)'></div>",
    );
    let xml = roxmltree::Document::parse(&svg).unwrap();
    let gradients: Vec<_> = xml
        .descendants()
        .filter(|node| node.has_tag_name("linearGradient"))
        .collect();
    assert_eq!(gradients[0].attribute("x1"), Some("-50.000000"));
    assert_eq!(gradients[0].attribute("x2"), Some("150.000000"));
    let hard: Vec<_> = gradients[1]
        .children()
        .filter(|node| node.has_tag_name("stop"))
        .map(|node| node.attribute("offset").unwrap())
        .collect();
    assert_eq!(hard, ["0.50000000", "0.50000000"]);
}

#[test]
fn transparent_gradient_intervals_retain_nonblack_unassociated_colors() {
    let (_, svg) = render(
        "<div style='width:100px;height:20px;background:linear-gradient(to right,red,transparent,blue)'></div>",
    );
    let xml = roxmltree::Document::parse(&svg).unwrap();
    let stops: Vec<_> = xml
        .descendants()
        .filter(|node| node.has_tag_name("stop"))
        .collect();
    assert!(
        stops
            .iter()
            .filter(|node| node.attribute("stop-opacity") == Some("0.00000000"))
            .all(|node| node.attribute("stop-color") != Some("rgb(0.000000,0.000000,0.000000)"))
    );
    assert!(
        stops
            .iter()
            .any(|node| node.attribute("stop-color") == Some("rgb(255.000000,0.000000,0.000000)"))
    );
    assert!(
        stops
            .iter()
            .any(|node| node.attribute("stop-color") == Some("rgb(0.000000,0.000000,255.000000)"))
    );
}

#[test]
fn alpha_color_changes_receive_bounded_premultiplied_samples() {
    let (_, svg) = render(
        "<div style='width:100px;height:20px;background:linear-gradient(to right,rgba(255,0,0,.2),rgba(0,0,255,.9))'></div>",
    );
    let xml = roxmltree::Document::parse(&svg).unwrap();
    let stops: Vec<_> = xml
        .descendants()
        .filter(|node| node.has_tag_name("stop"))
        .collect();
    assert!(stops.len() > 2 && stops.len() <= 257);
    let half = stops
        .iter()
        .find(|node| node.attribute("offset") == Some("0.50000000"))
        .unwrap();
    let alpha: f32 = half.attribute("stop-opacity").unwrap().parse().unwrap();
    assert!((alpha - 0.550_980_4).abs() < 0.001);
}

#[test]
fn shadow_gaussian_and_interior_knockout_are_self_contained() {
    let (_, svg) = render(
        "<div style='width:100px;height:100px;border-radius:10px;background:transparent;box-shadow:2px 3px 10px rgba(0,0,0,.5)'></div>",
    );
    let xml = roxmltree::Document::parse(&svg).unwrap();
    let gaussian = xml
        .descendants()
        .find(|node| node.has_tag_name("feGaussianBlur"))
        .unwrap();
    assert_eq!(gaussian.attribute("stdDeviation"), Some("5.0000"));
    assert_eq!(
        gaussian.parent().unwrap().attribute("filterUnits"),
        Some("userSpaceOnUse")
    );
    let mask = xml
        .descendants()
        .find(|node| node.has_tag_name("mask"))
        .unwrap();
    assert_eq!(mask.attribute("maskUnits"), Some("userSpaceOnUse"));
    assert!(
        mask.children()
            .any(|node| node.has_tag_name("path") && node.attribute("fill") == Some("black"))
    );
    assert!(
        xml.descendants()
            .any(|node| node.attribute("data-phos-shadow") == Some("true")
                && node.parent().unwrap().attribute("mask").is_some())
    );
}

#[test]
fn first_authored_shadow_is_foremost_and_filters_reuse() {
    let (_, svg) = render(
        "<div style='width:100px;height:40px;box-shadow:0 2px 4px red,0 2px 4px blue'></div><div style='width:100px;height:40px;box-shadow:0 2px 4px red'></div>",
    );
    let xml = roxmltree::Document::parse(&svg).unwrap();
    let shadow_colors: Vec<_> = xml
        .descendants()
        .filter(|node| node.attribute("data-phos-shadow").is_some())
        .map(|node| node.attribute("fill").unwrap())
        .collect();
    assert_eq!(shadow_colors, ["#0000ff", "#ff0000", "#ff0000"]);
    assert_eq!(
        xml.descendants()
            .filter(|node| node.has_tag_name("filter"))
            .count(),
        1
    );
    assert_eq!(
        xml.descendants()
            .filter(|node| node.has_tag_name("mask"))
            .count(),
        1
    );
}

#[test]
fn own_overflow_clip_preserves_outer_shadow_and_ancestor_clip_contains_it() {
    let (_, svg) = render(
        "<div style='width:150px;height:100px;overflow:hidden;border-radius:15px'><div style='width:100px;height:40px;overflow:hidden;border-radius:8px;box-shadow:0 2px 8px black'>Text</div></div>",
    );
    let xml = roxmltree::Document::parse(&svg).unwrap();
    let shadow = xml
        .descendants()
        .find(|node| node.attribute("data-phos-shadow").is_some())
        .unwrap();
    assert_eq!(
        shadow
            .ancestors()
            .filter(|node| node.attribute("clip-path").is_some())
            .count(),
        1
    );
    let text = xml
        .descendants()
        .find(|node| node.attribute("data-phos-text").is_some())
        .unwrap();
    assert_eq!(
        text.ancestors()
            .filter(|node| node.attribute("clip-path").is_some())
            .count(),
        2
    );
}

#[test]
fn shadows_expand_review_ink_extent_without_changing_box_layout() {
    let (scene, _) =
        render("<div style='width:100px;height:40px;box-shadow:0 10px 8px 2px black'></div>");
    let box_height = scene
        .boxes
        .iter()
        .find(|geometry| geometry.width == 100.0)
        .unwrap()
        .height;
    assert_eq!(box_height, 40.0);
    assert!(scene.height >= 64.0);
}

#[test]
fn invalid_effect_declarations_preserve_prior_value_and_later_rules() {
    let (_, svg) = render(
        "<div style='width:100px;height:20px;background-image:linear-gradient(red,blue);background-image:linear-gradient(red);box-shadow:0 2px 4px black;box-shadow:0 2px -1px black;border:2px solid green'>Valid</div>",
    );
    let xml = roxmltree::Document::parse(&svg).unwrap();
    assert_eq!(
        xml.descendants()
            .filter(|node| node.has_tag_name("linearGradient"))
            .count(),
        1
    );
    assert_eq!(
        xml.descendants()
            .filter(|node| node.attribute("data-phos-shadow").is_some())
            .count(),
        1
    );
    assert!(
        xml.descendants()
            .any(|node| node.attribute("stroke") == Some("#008000"))
    );
    assert!(!svg.contains("data-phos-truncated"));
}

#[test]
fn currentcolor_shadows_use_final_cascaded_color_independent_of_order() {
    let (_, svg) = render(
        "<style>.item{color:blue!important}</style><div style='width:20px;height:20px;box-shadow:0 2px;color:red'></div><div class='item' style='width:20px;height:20px;box-shadow:0 2px currentcolor;color:red'></div><div style='color:green'><div style='width:20px;height:20px;box-shadow:0 2px'></div></div>",
    );
    let xml = roxmltree::Document::parse(&svg).unwrap();
    let colors: Vec<_> = xml
        .descendants()
        .filter(|node| node.attribute("data-phos-shadow").is_some())
        .map(|node| node.attribute("fill").unwrap())
        .collect();
    assert_eq!(colors, ["#ff0000", "#0000ff", "#008000"]);
}

#[test]
fn unsupported_inset_and_excess_layers_do_not_install_partial_shadows() {
    let (_, svg) = render(
        "<div style='width:100px;height:20px;box-shadow:0 2px red,0 0 inset blue'></div><div style='width:100px;height:20px;box-shadow:0 0,0 0,0 0,0 0,0 0'></div>",
    );
    assert!(!svg.contains("data-phos-shadow"));
}

#[test]
fn negative_spread_can_remove_shadow_without_truncating_content() {
    let (_, svg) = render(
        "<div style='width:10px;height:10px;box-shadow:0 0 4px -20px black;background:red'></div>",
    );
    assert!(!svg.contains("data-phos-shadow"));
    assert!(!svg.contains("data-phos-truncated"));
    assert!(svg.contains("fill=\"#ff0000\""));
}

#[test]
fn hostile_manual_gradient_resources_are_bounded_and_references_remain_valid() {
    let mut primitives = Vec::new();
    for index in 0..paint::MAX_EFFECT_DEFINITIONS + 1 {
        let gradient = LinearGradient {
            direction: GradientDirection::Angle(index as f32 / 100.0),
            stops: vec![
                ColorStop {
                    color: Color::BLACK,
                    position: Some(Length::Percent(0.0)),
                },
                ColorStop {
                    color: Color::WHITE,
                    position: Some(Length::Percent(100.0)),
                },
            ],
        };
        primitives.push(decorated(Some(gradient), Vec::new()));
    }
    let svg = paint::to_svg(&manual(primitives));
    assert_valid_resources(&svg);
    let xml = roxmltree::Document::parse(&svg).unwrap();
    assert_eq!(
        xml.descendants()
            .filter(|node| node.has_tag_name("linearGradient"))
            .count(),
        paint::MAX_EFFECT_DEFINITIONS
    );
    assert_eq!(
        xml.root_element().attribute("data-phos-truncated"),
        Some("true")
    );
}

#[test]
fn oversized_shadow_surface_reports_truncation_and_preserves_background() {
    let shadow = BoxShadow {
        offset_x: 0.0,
        offset_y: 2.0,
        blur: 4.0,
        spread: 0.0,
        color: Color::BLACK,
    };
    let mut primitive = decorated(None, vec![shadow]);
    if let Primitive::DecoratedBox { width, height, .. } = &mut primitive {
        *width = 5000.0;
        *height = 5000.0;
    }
    let svg = paint::to_svg(&manual(vec![primitive]));
    assert_valid_resources(&svg);
    assert!(svg.contains("data-phos-truncated=\"true\""));
    assert!(!svg.contains("data-phos-shadow"));
    assert!(svg.contains("fill=\"#ffffff\""));
}
