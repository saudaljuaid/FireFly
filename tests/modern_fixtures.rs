use phos::dom::{Document, NodeId};
use phos::layout::{BoxGeometry, BoxKind, Primitive, Scene};
use phos::{Viewport, css, html, layout, paint, resource, style};

fn fixture(name: &str, width: f32) -> (Document, Scene) {
    let source = std::fs::read_to_string(format!(
        "{}/tests/render/layout-{name}.html",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap();
    let document = html::parse(&source).unwrap();
    let viewport = Viewport {
        width,
        height: None,
    };
    let sheet = css::parse(&document.stylesheets());
    let styles = style::compute_with_viewport(&document, &sheet, viewport);
    let mut images = vec![None; document.nodes.len()];
    for node in document.preorder() {
        if let Some(element) = document.element(node)
            && element.tag == "img"
        {
            let path = format!(
                "{}/tests/render/{}",
                env!("CARGO_MANIFEST_DIR"),
                element.attribute("src").unwrap()
            );
            images[node] =
                Some(resource::decode_image(&std::fs::read(path).unwrap(), None).unwrap());
        }
    }
    let scene = layout::layout_with_images_and_viewport(&document, &styles, &images, viewport);
    (document, scene)
}

fn id(document: &Document, name: &str) -> NodeId {
    document
        .preorder()
        .into_iter()
        .find(|&id| {
            document
                .element(id)
                .is_some_and(|element| element.attribute("id") == Some(name))
        })
        .unwrap()
}

fn geometry(scene: &Scene, node: NodeId) -> &BoxGeometry {
    scene
        .boxes
        .iter()
        .find(|geometry| geometry.node == Some(node) && geometry.kind == BoxKind::Element)
        .unwrap()
}

fn close(actual: f32, expected: f32) {
    assert!(
        (actual - expected).abs() < 0.01,
        "actual {actual}, expected {expected}"
    );
}

#[test]
fn responsive_navigation_changes_direction_and_retains_padded_overlay_geometry() {
    for width in [320.0, 600.0, 601.0, 900.0] {
        let (document, scene) = fixture("navigation", width);
        let header = geometry(&scene, id(&document, "navigation"));
        let links = geometry(&scene, id(&document, "links"));
        let brand = document
            .preorder()
            .into_iter()
            .find(|&node| {
                document
                    .element(node)
                    .is_some_and(|element| element.has_class("brand"))
            })
            .unwrap();
        let brand = geometry(&scene, brand);
        close(header.width, width);
        close(brand.x, 20.0);
        if width <= 600.0 {
            close(links.x, 20.0);
            assert!(links.y >= brand.y + brand.height + 9.99);
        } else {
            close(links.x, brand.x + brand.width + 16.0);
        }
        let label = document
            .preorder()
            .into_iter()
            .find(|&node| {
                document
                    .element(node)
                    .is_some_and(|element| element.has_class("label"))
            })
            .unwrap();
        let parent = document.nodes[label].parent.unwrap();
        let label = geometry(&scene, label);
        let parent = geometry(&scene, parent);
        close(label.x + label.width, parent.x + parent.width - 4.0);
        close(label.y, parent.y + 3.0);
        assert!(scene.runs.iter().any(|run| run.text.content.contains('文')));
        assert!(!scene.truncated);
    }
}

#[test]
fn documentation_grid_has_fixed_sidebar_and_actual_breakpoint_placement() {
    for width in [320.0, 640.0, 900.0] {
        let (document, scene) = fixture("documentation", width);
        let shell = geometry(&scene, id(&document, "documentation"));
        let sidebar = geometry(&scene, id(&document, "sidebar"));
        let content = geometry(&scene, id(&document, "content"));
        close(shell.x, 16.0);
        close(shell.width, width - 32.0);
        if width < 640.0 {
            close(sidebar.width, width - 32.0);
            close(content.x, 16.0);
            close(content.y, sidebar.y + sidebar.height + 12.0);
        } else {
            close(sidebar.width, 172.0);
            close(content.x, 212.0);
            close(content.width, width - 228.0);
            close(content.y, sidebar.y);
        }
        let clipped = geometry(&scene, id(&document, "clip"));
        close(clipped.height, 78.0);
        assert!(scene.primitives.iter().any(|primitive| matches!(primitive, Primitive::ClipStart { x, y, width, radius, .. } if (*x - (clipped.x + 1.0)).abs() < 0.01 && (*y - (clipped.y + 1.0)).abs() < 0.01 && *width > 0.0 && radius[0] > 0.0)));
    }
}

#[test]
fn dashboard_spans_and_implicit_rows_are_distinct_from_equal_columns() {
    for width in [320.0, 640.0, 900.0] {
        let (document, scene) = fixture("dashboard", width);
        let summary = geometry(&scene, id(&document, "summary"));
        let queue = geometry(&scene, id(&document, "queue"));
        let register = geometry(&scene, id(&document, "register"));
        let notes = geometry(&scene, id(&document, "notes"));
        let gap = if width == 320.0 { 10.0 } else { 14.0 };
        if width == 900.0 {
            close(queue.width, 140.0);
            close(queue.x, 740.0);
            close(queue.y, summary.y);
            close(summary.width, 706.0);
            close(queue.y + queue.height, register.y + register.height);
        } else {
            close(summary.x, 14.0);
            close(summary.width, width - 28.0);
            close(queue.x, summary.x);
            close(queue.width, summary.width);
            close(queue.y, summary.y + summary.height + gap);
            close(register.y, queue.y + queue.height + gap);
        }
        close(notes.y, register.y + register.height + gap);
        close(
            notes.width,
            width - if width == 900.0 { 40.0 } else { 28.0 },
        );
        assert!(scene.primitives.iter().any(
            |primitive| matches!(primitive, Primitive::ClipStart { radius, .. } if radius[0] > 0.0)
        ));
    }
}

#[test]
fn gallery_images_keep_unequal_intrinsic_aspect_ratios_at_all_three_widths() {
    for width in [320.0, 640.0, 900.0] {
        let (document, scene) = fixture("gallery", width);
        let mut count = 0;
        for node in document.preorder() {
            let Some(element) = document
                .element(node)
                .filter(|element| element.tag == "img")
            else {
                continue;
            };
            let image = geometry(&scene, node);
            let expected_ratio = match element.attribute("src").unwrap() {
                "layout-landscape.png" => 1.5,
                "layout-portrait.png" => 2.0 / 3.0,
                "layout-wide.png" => 2.0,
                source => panic!("unexpected image {source}"),
            };
            close(image.width / image.height, expected_ratio);
            assert!(image.width > 0.0 && image.height > 0.0);
            count += 1;
        }
        assert_eq!(count, 4);
        assert_eq!(
            scene
                .primitives
                .iter()
                .filter(|primitive| matches!(primitive, Primitive::Image { .. }))
                .count(),
            4
        );
        let first = geometry(&scene, id(&document, "landscape"));
        let portrait = geometry(&scene, id(&document, "portrait"));
        if width <= 400.0 {
            close(portrait.x, first.x);
            assert!(portrait.y > first.y + first.height);
        } else {
            close(portrait.y, first.y);
            close(portrait.x, first.x + first.width + 14.0);
        }
    }
}

#[test]
fn comparison_columns_stretch_unequal_text_and_align_automatic_bottom_margins() {
    for width in [320.0, 640.0, 900.0] {
        let (document, scene) = fixture("pricing", width);
        let first = geometry(&scene, id(&document, "reading"));
        let second = geometry(&scene, id(&document, "study"));
        let third = geometry(&scene, id(&document, "publication"));
        close(first.width, second.width);
        close(first.width, third.width);
        if width > 650.0 {
            close(first.height, second.height);
            close(first.height, third.height);
            close(second.x, first.x + first.width + 16.0);
            close(third.x, second.x + second.width + 16.0);
            let bottoms: Vec<_> = document
                .preorder()
                .into_iter()
                .filter(|&node| {
                    document
                        .element(node)
                        .is_some_and(|element| element.has_class("requirements"))
                })
                .map(|node| {
                    let item = geometry(&scene, node);
                    item.y + item.height
                })
                .collect();
            assert_eq!(bottoms.len(), 3);
            close(bottoms[0], bottoms[1]);
            close(bottoms[1], bottoms[2]);
        } else {
            close(first.width, width - 40.0);
            close(second.x, first.x);
            close(second.y, first.y + first.height + 12.0);
            close(third.y, second.y + second.height + 12.0);
            assert!(second.height > first.height);
        }
    }
}

#[test]
fn restrained_landing_nests_grid_and_flex_and_preserves_image_overlay() {
    for width in [320.0, 640.0, 900.0] {
        let (document, scene) = fixture("landing", width);
        let intro = geometry(&scene, id(&document, "intro"));
        let plate = geometry(&scene, id(&document, "hero-plate"));
        let features = geometry(&scene, id(&document, "features"));
        close(intro.width, width - 44.0);
        if width <= 700.0 {
            close(plate.x, intro.x);
        } else {
            assert!(plate.x > intro.x + 300.0);
        }
        assert!(features.y >= intro.y + intro.height + 25.99);
        let annotation = document
            .preorder()
            .into_iter()
            .find(|&node| {
                document
                    .element(node)
                    .is_some_and(|element| element.has_class("annotation"))
            })
            .unwrap();
        let annotation = geometry(&scene, annotation);
        close(
            annotation.x + annotation.width,
            plate.x + plate.width - 1.0 - 20.0,
        );
        close(
            annotation.y + annotation.height,
            plate.y + plate.height - 1.0 - 20.0,
        );
    }
}

#[test]
fn all_original_layout_sources_paint_finite_unique_self_contained_svg() {
    for name in [
        "navigation",
        "documentation",
        "dashboard",
        "gallery",
        "pricing",
        "landing",
    ] {
        for width in [320.0, 640.0, 900.0] {
            let (_, scene) = fixture(name, width);
            assert!(!scene.truncated, "{name} {width}");
            assert!(scene.boxes.iter().all(|box_| {
                [box_.x, box_.y, box_.width, box_.height]
                    .iter()
                    .all(|value| value.is_finite())
            }));
            let svg = paint::to_svg(&scene);
            let parsed = roxmltree::Document::parse(&svg).unwrap();
            let mut ids = std::collections::HashSet::new();
            for node in parsed.descendants().filter(|node| node.is_element()) {
                if let Some(id) = node.attribute("id") {
                    assert!(ids.insert(id), "duplicate {id} in {name} {width}");
                }
                if let Some(href) = node.attribute("href") {
                    assert!(href.starts_with('#') || href.starts_with("data:image/"));
                }
            }
        }
    }
}
