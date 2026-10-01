use phos::dom::{Document, NodeId};
use phos::layout::{BoxGeometry, BoxKind, ImageSource, Primitive, Scene};
use phos::{css, html, layout, style};

fn scene(source: &str, width: f32) -> (Document, Scene) {
    let document = html::parse(&format!(
        "<style>html,body{{margin:0;padding:0}}*{{font-size:16px;line-height:20px}}</style>{source}"
    ))
    .unwrap();
    let styles = style::compute(&document, &css::parse(&document.stylesheets()));
    let scene = layout::layout(&document, &styles, width);
    assert_finite(&scene);
    (document, scene)
}

fn node(document: &Document, id: &str) -> NodeId {
    document
        .nodes
        .iter()
        .enumerate()
        .find(|(index, _)| {
            document
                .element(*index)
                .is_some_and(|element| element.attribute("id") == Some(id))
        })
        .unwrap()
        .0
}

fn geometry<'a>(document: &Document, scene: &'a Scene, id: &str) -> &'a BoxGeometry {
    let node = node(document, id);
    scene
        .boxes
        .iter()
        .find(|item| item.node == Some(node) && item.kind == BoxKind::Element)
        .unwrap()
}

fn close(actual: f32, expected: f32) {
    assert!((actual - expected).abs() < 0.04, "{actual} != {expected}");
}

fn assert_finite(scene: &Scene) {
    assert!(scene.width.is_finite() && scene.height.is_finite());
    for item in &scene.boxes {
        assert!(
            [item.x, item.y, item.width, item.height]
                .into_iter()
                .all(f32::is_finite)
        );
        assert!(item.width >= 0.0 && item.height >= 0.0);
    }
    for run in &scene.runs {
        assert!(run.x.is_finite() && run.baseline.is_finite());
        assert!(run.line < scene.line_boxes.len());
    }
}

#[test]
fn unequal_grow_and_zero_basis_resolve_actual_child_boxes() {
    let (document, scene) = scene(
        "<div style='display:flex;width:400px'><div id='a' style='flex:1 1 100px;height:20px'></div><div id='b' style='flex:3 1 100px;height:20px'></div></div>
         <div style='display:flex;width:400px'><div id='c' style='flex:1;min-width:0;height:20px'></div><div id='d' style='flex:3;min-width:0;height:20px'></div></div>",
        400.0,
    );
    close(geometry(&document, &scene, "a").width, 150.0);
    close(geometry(&document, &scene, "b").width, 250.0);
    close(geometry(&document, &scene, "b").x, 150.0);
    close(geometry(&document, &scene, "c").width, 100.0);
    close(geometry(&document, &scene, "d").width, 300.0);
}

#[test]
fn scaled_shrink_uses_unequal_bases_and_factors() {
    let (document, scene) = scene(
        "<div style='display:flex;width:300px'><div id='a' style='flex:0 1 200px;min-width:0'></div><div id='b' style='flex:0 2 100px;min-width:0'></div><div id='c' style='flex:0 1 100px;min-width:0'></div></div>",
        300.0,
    );
    close(geometry(&document, &scene, "a").width, 160.0);
    close(geometry(&document, &scene, "b").width, 60.0);
    close(geometry(&document, &scene, "c").width, 80.0);
    close(geometry(&document, &scene, "c").x, 220.0);
}

#[test]
fn definite_maximum_caps_automatic_minimum_but_explicit_minimum_wins() {
    let (document, scene) = scene(
        "<div style='display:flex;width:200px'><div id='auto' style='max-width:80px;overflow-wrap:normal'>UninterruptedNavigationLabel</div><div id='rest' style='flex:1;min-width:0'></div></div>
         <div style='display:flex;width:200px'><div id='explicit' style='max-width:80px;min-width:100px;overflow-wrap:normal'>UninterruptedNavigationLabel</div></div>",
        200.0,
    );
    close(geometry(&document, &scene, "auto").width, 80.0);
    close(geometry(&document, &scene, "rest").width, 120.0);
    close(geometry(&document, &scene, "explicit").width, 100.0);
}

#[test]
fn overflowing_fixed_items_report_coordinate_saturation() {
    let (document, scene) = scene(
        "<div style='display:flex;width:1000px'><div id='a' style='font-size:1024px;flex:0 0 700em;min-width:0;height:1px'></div><div id='b' style='font-size:1024px;flex:0 0 700em;min-width:0;height:1px'></div><div id='c' style='font-size:1024px;flex:0 0 700em;min-width:0;height:1px'></div></div>",
        1000.0,
    );
    close(geometry(&document, &scene, "a").x, 0.0);
    close(geometry(&document, &scene, "b").x, 716_800.0);
    close(geometry(&document, &scene, "c").x, 1_000_000.0);
    assert!(scene.truncated);
    let svg = phos::paint::to_svg(&scene);
    let xml = roxmltree::Document::parse(&svg).unwrap();
    assert_eq!(
        xml.root_element().attribute("data-phos-truncated"),
        Some("true")
    );
}

#[test]
fn wrap_reverse_stretched_baseline_groups_remain_at_cross_start() {
    let (document, scene) = scene(
        "<div id='normal' style='display:flex;width:200px;height:100px;align-items:baseline;flex-wrap:wrap;align-content:stretch'><div id='a' style='width:80px;font-size:16px;line-height:20px'>A</div><div id='b' style='width:80px;font-size:32px;line-height:40px'>A</div></div>
         <div id='reverse' style='display:flex;width:200px;height:100px;align-items:baseline;flex-wrap:wrap-reverse;align-content:stretch'><div id='c' style='width:80px;font-size:16px;line-height:20px'>A</div><div id='d' style='width:80px;font-size:32px;line-height:40px'>A</div></div>",
        200.0,
    );
    let normal = geometry(&document, &scene, "normal").y;
    let reverse = geometry(&document, &scene, "reverse").y;
    close(geometry(&document, &scene, "d").y - reverse, 60.0);
    close(geometry(&document, &scene, "b").y - normal, 0.0);
    close(
        geometry(&document, &scene, "c").y - reverse,
        geometry(&document, &scene, "a").y - normal + 60.0,
    );
    let c = node(&document, "c");
    let d = node(&document, "d");
    let baseline = |id: NodeId| {
        let text_node = document.nodes[id].children[0];
        scene
            .runs
            .iter()
            .find(|run| run.node == text_node)
            .unwrap()
            .baseline
    };
    close(baseline(c), baseline(d));
}

#[test]
fn min_max_freezing_redistributes_remaining_space() {
    let (document, scene) = scene(
        "<div style='display:flex;width:300px'><div id='a' style='flex:1;max-width:50px;min-width:0'></div><div id='b' style='flex:1;min-width:0'></div></div>
         <div style='display:flex;width:300px'><div id='c' style='flex:0 1 200px;min-width:180px'></div><div id='d' style='flex:0 1 200px;min-width:0'></div></div>",
        300.0,
    );
    close(geometry(&document, &scene, "a").width, 50.0);
    close(geometry(&document, &scene, "b").width, 250.0);
    close(geometry(&document, &scene, "c").width, 180.0);
    close(geometry(&document, &scene, "d").width, 120.0);
}

#[test]
fn partial_factors_leave_space_for_justify_content() {
    let (document, scene) = scene(
        "<div style='display:flex;width:300px;justify-content:center'><div id='a' style='flex:0.25 1 100px'></div><div id='b' style='flex:0.25 1 100px'></div></div>",
        300.0,
    );
    close(geometry(&document, &scene, "a").width, 125.0);
    close(geometry(&document, &scene, "a").x, 25.0);
    close(geometry(&document, &scene, "b").x, 150.0);
}

#[test]
fn wrapping_gaps_and_unequal_cross_sizes_are_exact() {
    let (document, scene) = scene(
        "<div id='p' style='display:flex;flex-wrap:wrap;width:210px;column-gap:10px;row-gap:5px;align-items:flex-start'><div id='a' style='flex:0 0 100px;height:20px'></div><div id='b' style='flex:0 0 100px;height:30px'></div><div id='c' style='flex:0 0 100px;height:40px'></div></div>",
        210.0,
    );
    close(geometry(&document, &scene, "b").x, 110.0);
    close(geometry(&document, &scene, "b").y, 0.0);
    close(geometry(&document, &scene, "c").x, 0.0);
    close(geometry(&document, &scene, "c").y, 35.0);
    close(geometry(&document, &scene, "p").height, 75.0);
}

#[test]
fn wrap_reverse_and_align_content_place_lines_without_dom_reversal() {
    let (document, scene) = scene(
        "<div style='display:flex;flex-wrap:wrap-reverse;width:100px;height:100px;align-items:flex-start;align-content:space-between'><div id='a' style='flex:0 0 60px;height:20px'></div><div id='b' style='flex:0 0 60px;height:30px'></div></div>",
        100.0,
    );
    close(geometry(&document, &scene, "a").y, 80.0);
    close(geometry(&document, &scene, "b").y, 0.0);
}

#[test]
fn column_wrapping_reverse_and_gaps_use_height_as_main_axis() {
    let (document, scene) = scene(
        "<div id='p' style='display:flex;flex-direction:column-reverse;flex-wrap:wrap;width:45px;height:100px;row-gap:10px;column-gap:5px;align-items:flex-start;align-content:flex-start'><div id='a' style='flex:0 0 40px;width:20px'></div><div id='b' style='flex:0 0 40px;width:20px'></div><div id='c' style='flex:0 0 40px;width:20px'></div></div>",
        45.0,
    );
    close(geometry(&document, &scene, "a").y, 60.0);
    close(geometry(&document, &scene, "b").y, 10.0);
    close(geometry(&document, &scene, "c").x, 25.0);
    close(geometry(&document, &scene, "c").y, 60.0);
}

#[test]
fn row_reverse_order_and_rtl_keep_logical_nodes_intact() {
    let (document, scene) = scene(
        "<div style='display:flex;flex-direction:row-reverse;width:300px'><div id='a' style='flex:0 0 50px;order:2'></div><div id='b' style='flex:0 0 50px;order:-1'></div><div id='c' style='flex:0 0 50px'></div></div>
         <div style='display:flex;direction:rtl;width:300px'><div id='d' style='flex:0 0 50px'></div><div id='e' style='flex:0 0 50px'></div></div>",
        300.0,
    );
    close(geometry(&document, &scene, "b").x, 250.0);
    close(geometry(&document, &scene, "c").x, 200.0);
    close(geometry(&document, &scene, "a").x, 150.0);
    close(geometry(&document, &scene, "d").x, 250.0);
    close(geometry(&document, &scene, "e").x, 200.0);
    let parent = document.nodes[node(&document, "a")].parent.unwrap();
    let children = &document.nodes[parent].children;
    assert!(
        children.iter().position(|&id| id == node(&document, "a"))
            < children.iter().position(|&id| id == node(&document, "b"))
    );
}

#[test]
fn percentage_basis_uses_definite_main_axis_and_auto_in_indefinite_column() {
    let (document, scene) = scene(
        "<div style='display:flex;width:400px'><div id='a' style='flex:0 0 25%;height:20px'></div><div id='b' style='flex:0 0 50%;height:20px'></div></div>
         <div id='p' style='display:flex;flex-direction:column;width:100px'><div id='c' style='flex-basis:50%'>First</div><div id='d' style='flex-basis:50%'>Second</div></div>",
        400.0,
    );
    close(geometry(&document, &scene, "a").width, 100.0);
    close(geometry(&document, &scene, "b").width, 200.0);
    close(geometry(&document, &scene, "c").height, 20.0);
    close(geometry(&document, &scene, "d").height, 20.0);
    close(geometry(&document, &scene, "p").height, 40.0);
}

#[test]
fn main_and_cross_auto_margins_override_alignment() {
    let (document, scene) = scene(
        "<div style='display:flex;width:300px;height:100px;justify-content:center;align-items:flex-end'><div id='a' style='flex:0 0 50px;height:20px'></div><div id='b' style='flex:0 0 50px;height:20px;margin-left:auto;margin-top:auto;margin-bottom:auto'></div></div>",
        300.0,
    );
    close(geometry(&document, &scene, "a").x, 0.0);
    close(geometry(&document, &scene, "a").y, 80.0);
    close(geometry(&document, &scene, "b").x, 250.0);
    close(geometry(&document, &scene, "b").y, 40.0);
}

#[test]
fn cross_stretch_resolves_percentage_descendants_from_final_size() {
    let (document, scene) = scene(
        "<div style='display:flex;width:200px;height:100px'><div id='a' style='flex:1;min-width:0;padding:10px'><div id='b' style='height:50%;width:20px'></div></div><div style='flex:1;min-width:0'></div></div>",
        200.0,
    );
    close(geometry(&document, &scene, "a").height, 100.0);
    close(geometry(&document, &scene, "b").height, 40.0);
    close(geometry(&document, &scene, "b").y, 10.0);
}

#[test]
fn mixed_font_baselines_use_shared_line_metrics() {
    let (document, scene) = scene(
        "<div style='display:flex;align-items:baseline;width:300px'><div id='a' style='font-size:16px;line-height:20px'>Small</div><div id='b' style='font-size:32px;line-height:40px'>Large</div></div>",
        300.0,
    );
    let a = scene
        .runs
        .iter()
        .find(|run| run.text.content == "Small")
        .unwrap();
    let b = scene
        .runs
        .iter()
        .find(|run| run.text.content == "Large")
        .unwrap();
    close(a.baseline, b.baseline);
    assert!(geometry(&document, &scene, "a").y > geometry(&document, &scene, "b").y);
    close(
        a.text.advance,
        scene
            .primitives
            .iter()
            .find_map(|primitive| match primitive {
                Primitive::Text { content, width, .. } if content == "Small" => Some(*width),
                _ => None,
            })
            .unwrap(),
    );
}

#[test]
fn multiline_flex_baseline_uses_first_line() {
    let (_, scene) = scene(
        "<div style='display:flex;align-items:baseline;width:180px'><div style='flex:0 0 60px;min-width:0'>First longer words</div><div style='flex:0 0 100px;min-width:0;font-size:24px;line-height:30px'>Large</div></div>",
        180.0,
    );
    let first = scene
        .runs
        .iter()
        .find(|run| run.text.content == "First")
        .unwrap();
    let large = scene
        .runs
        .iter()
        .find(|run| run.text.content == "Large")
        .unwrap();
    close(first.baseline, large.baseline);
    assert!(scene.line_boxes.len() >= 4);
}

#[test]
fn automatic_minimum_preserves_unbreakable_text_and_explicit_zero_shrinks() {
    let long = "UninterruptedNavigationDestination";
    let (document, scene) = scene(
        &format!(
            "<div style='display:flex;width:100px'><div id='a' style='flex:1'>{long}</div><div style='width:50px;flex-shrink:0'></div></div><div style='display:flex;width:100px'><div id='b' style='flex:1;min-width:0'>{long}</div><div style='width:50px;flex-shrink:0'></div></div>"
        ),
        100.0,
    );
    assert!(geometry(&document, &scene, "a").width > 100.0);
    close(geometry(&document, &scene, "b").width, 50.0);
    assert!(geometry(&document, &scene, "b").height > 20.0);
}

#[test]
fn intrinsic_basis_and_inline_decorations_use_shared_contributions() {
    let (document, scene) = scene(
        "<div style='display:flex;width:500px;align-items:flex-start'><div id='a' style='flex-basis:max-content'><span style='padding:0 5px;border:2px solid'>Label</span></div><div id='b' style='flex-basis:min-content'>Wide words</div></div>",
        500.0,
    );
    let label = scene
        .runs
        .iter()
        .find(|run| run.text.content == "Label")
        .unwrap()
        .text
        .advance;
    close(geometry(&document, &scene, "a").width, label + 14.0);
    let words: Vec<_> = scene
        .runs
        .iter()
        .filter(|run| matches!(run.text.content.as_str(), "Wide" | "words"))
        .map(|run| run.text.advance)
        .collect();
    close(
        geometry(&document, &scene, "b").width,
        words.into_iter().fold(0.0, f32::max),
    );
}

#[test]
fn nested_flex_and_grid_share_final_item_sizes() {
    let (document, scene) = scene(
        "<div style='display:flex;width:400px;gap:20px'><div id='a' style='display:flex;flex:1;min-width:0;gap:10px'><div id='b' style='flex:1;min-width:0;height:20px'></div><div id='c' style='flex:2;min-width:0;height:20px'></div></div><div id='d' style='display:grid;flex:1;min-width:0;grid-template-columns:1fr 1fr;gap:10px'><div id='e' style='height:20px'></div><div id='f' style='height:20px'></div></div></div>",
        400.0,
    );
    close(geometry(&document, &scene, "a").width, 190.0);
    close(geometry(&document, &scene, "b").width, 60.0);
    close(geometry(&document, &scene, "c").width, 120.0);
    close(geometry(&document, &scene, "d").x, 210.0);
    close(geometry(&document, &scene, "e").width, 90.0);
    close(geometry(&document, &scene, "f").x, 310.0);
}

#[test]
fn hidden_absolute_and_whitespace_nodes_do_not_add_flex_gaps() {
    let (document, scene) = scene(
        "<div style='display:flex;width:200px;gap:20px'> \n <div id='a' style='flex:0 0 50px;height:20px'></div> \n <div style='display:none;width:80px'></div><div style='position:absolute;width:30px;height:20px'></div><div id='b' style='flex:0 0 50px;height:20px'></div> \n </div>",
        200.0,
    );
    close(geometry(&document, &scene, "a").x, 0.0);
    close(geometry(&document, &scene, "b").x, 70.0);
}

#[test]
fn anonymous_text_items_are_sized_and_adjacent_text_remains_shaped() {
    let (document, scene) = scene(
        "<div style='display:flex;width:300px;gap:10px;align-items:flex-start'>First<div id='a' style='flex:0 0 50px;height:20px'></div>Last</div>",
        300.0,
    );
    let first = scene
        .runs
        .iter()
        .find(|run| run.text.content == "First")
        .unwrap();
    let last = scene
        .runs
        .iter()
        .find(|run| run.text.content == "Last")
        .unwrap();
    close(
        geometry(&document, &scene, "a").x,
        first.text.advance + 10.0,
    );
    close(last.x, first.text.advance + 70.0);
}

#[test]
fn images_preserve_intrinsic_ratio_after_flex_shrinking() {
    let document = html::parse("<style>html,body{margin:0;padding:0}</style><div style='display:flex;width:200px;align-items:flex-start'><img id='a' style='min-width:0'><div id='b' style='width:100px;flex-shrink:0;height:20px'></div></div>").unwrap();
    let styles = style::compute(&document, &css::parse(&document.stylesheets()));
    let mut images = vec![None; document.nodes.len()];
    images[node(&document, "a")] = Some(ImageSource {
        href: "data:image/png;base64,".into(),
        width: 200.0,
        height: 100.0,
    });
    let scene = layout::layout_with_images(&document, &styles, &images, 200.0);
    assert_finite(&scene);
    close(geometry(&document, &scene, "a").width, 100.0);
    close(geometry(&document, &scene, "a").height, 50.0);
    close(geometry(&document, &scene, "b").x, 100.0);
}

#[test]
fn auto_images_transfer_min_max_constraints_across_both_dimensions() {
    for display in ["block", "flex", "grid"] {
        for (constraints, expected_width, expected_height) in [
            ("max-height:20px", 40.0, 20.0),
            ("min-height:100px", 200.0, 100.0),
            ("max-width:60px;max-height:20px", 40.0, 20.0),
            ("min-width:100px;max-height:20px", 100.0, 20.0),
        ] {
            let source = format!(
                "<style>html,body{{margin:0;padding:0}}</style><div style='display:{display};width:300px;align-items:start;justify-items:start;grid-template-columns:auto'><img id='image' style='{constraints}'></div>"
            );
            let document = html::parse(&source).unwrap();
            let styles = style::compute(&document, &css::parse(&document.stylesheets()));
            let mut images = vec![None; document.nodes.len()];
            images[node(&document, "image")] = Some(ImageSource {
                href: "data:image/png;base64,".into(),
                width: 128.0,
                height: 64.0,
            });
            let scene = layout::layout_with_images(&document, &styles, &images, 300.0);
            assert_finite(&scene);
            let item = geometry(&document, &scene, "image");
            close(item.width, expected_width);
            close(item.height, expected_height);
            let painted = scene
                .primitives
                .iter()
                .find_map(|primitive| match primitive {
                    Primitive::Image { width, height, .. } => Some((*width, *height)),
                    _ => None,
                });
            assert_eq!(painted, Some((expected_width, expected_height)));
            assert!(!scene.truncated, "{display}: {constraints}");
        }
    }
}

#[test]
fn positioned_overlay_uses_padded_flex_item_and_retains_rounded_clip() {
    let (document, scene) = scene(
        "<div style='display:flex;width:200px'><div id='a' style='flex:0 0 100px;box-sizing:border-box;height:60px;padding:10px;border:2px solid;position:relative;overflow:hidden;border-radius:12px'><div id='b' style='position:absolute;right:0;bottom:0;width:20px;height:20px'></div></div></div>",
        200.0,
    );
    close(geometry(&document, &scene, "a").width, 100.0);
    close(geometry(&document, &scene, "b").x, 78.0);
    close(geometry(&document, &scene, "b").y, 38.0);
    assert!(scene.primitives.iter().any(|primitive| matches!(primitive, Primitive::ClipStart { width, height, radius, .. } if (*width - 96.0).abs() < 0.02 && (*height - 56.0).abs() < 0.02 && radius[0] > 0.0)));
    let mut depth = 0;
    for primitive in &scene.primitives {
        match primitive {
            Primitive::ClipStart { .. } => depth += 1,
            Primitive::ClipEnd => {
                assert!(depth > 0);
                depth -= 1;
            }
            _ => {}
        }
    }
    assert_eq!(depth, 0);
}

#[test]
fn reordered_overlap_paints_order_modified_source_and_static_z_index() {
    let (document, scene) = scene(
        "<div style='display:flex;flex-direction:row-reverse;width:200px'><div id='a' style='flex:0 0 100px;order:1;background:red'>A</div><div id='b' style='flex:0 0 100px;order:-1;background:blue'>B</div></div><div style='display:flex;width:200px'><div style='width:100px;z-index:2'>Positive</div><div style='width:100px;z-index:-1'>Negative</div></div>",
        200.0,
    );
    close(geometry(&document, &scene, "b").x, 100.0);
    let text: Vec<_> = scene
        .primitives
        .iter()
        .filter_map(|primitive| match primitive {
            Primitive::Text { content, .. } => Some(content.as_str()),
            _ => None,
        })
        .collect();
    assert!(
        text.iter().position(|&value| value == "B") < text.iter().position(|&value| value == "A")
    );
    assert!(
        text.iter().position(|&value| value == "Negative")
            < text.iter().position(|&value| value == "Positive")
    );
}

#[test]
fn inline_flex_is_atomic_and_uses_existing_inline_baseline() {
    let (document, scene) = scene(
        "<p style='margin:0'>Before <span id='a' style='display:inline-flex;gap:5px'><span id='b'>One</span><span id='c'>Two</span></span> After</p>",
        500.0,
    );
    let one = scene
        .runs
        .iter()
        .find(|run| run.text.content == "One")
        .unwrap();
    let two = scene
        .runs
        .iter()
        .find(|run| run.text.content == "Two")
        .unwrap();
    close(
        geometry(&document, &scene, "a").width,
        one.text.advance + two.text.advance + 5.0,
    );
    close(
        geometry(&document, &scene, "c").x - geometry(&document, &scene, "b").x,
        one.text.advance + 5.0,
    );
    let before = scene
        .runs
        .iter()
        .find(|run| run.text.content == "Before")
        .unwrap();
    close(before.baseline, one.baseline);
}

#[test]
fn unicode_and_nonbreaking_content_inside_flex_remains_bounded() {
    let (document, scene) = scene(
        "<div style='display:flex;flex-wrap:wrap;width:200px;gap:8px'><div id='a' style='flex:1 1 80px;min-width:0'>é 中文 Long\u{a0}Label</div><div id='b' style='flex:1 1 80px;min-width:0;direction:rtl'>مرحبا English שָׁלוֹם</div></div>",
        200.0,
    );
    assert!(!scene.truncated);
    close(geometry(&document, &scene, "a").width, 96.0);
    close(geometry(&document, &scene, "b").width, 96.0);
    assert!(
        scene
            .runs
            .iter()
            .any(|run| run.text.direction == phos::text::Direction::Rtl)
    );
    assert!(scene.runs.iter().any(|run| run.text.content.contains("é")));
}

#[test]
fn many_items_truncate_explicitly_without_nonfinite_geometry() {
    let mut source = "<div style='display:flex;flex-wrap:wrap;width:200px;gap:1px'>".to_string();
    source.push_str(
        &"<div style='width:10px;height:1px;flex-shrink:0'></div>"
            .repeat(phos::flex::MAX_FLEX_ITEMS + 1),
    );
    source.push_str("</div>");
    let (_, scene) = scene(&source, 200.0);
    assert!(scene.truncated);
    assert!(scene.boxes.len() < 20_000);
}

#[test]
fn invalid_flex_values_preserve_prior_valid_values_and_later_rules() {
    let (document, scene) = scene(
        "<style>.item{flex:1 1 50px;flex:1 banana;flex-grow:NaN;flex-shrink:-1;flex-basis:-20px;flex-direction:diagonal;flex-wrap:forever;order:999999999999}.last{height:30px}</style><div style='display:flex;width:200px'><div id='a' class='item' style='height:20px'></div><div id='b' class='item last'></div></div>",
        200.0,
    );
    close(geometry(&document, &scene, "a").width, 100.0);
    close(geometry(&document, &scene, "b").width, 100.0);
    close(geometry(&document, &scene, "b").height, 30.0);
    assert!(!scene.truncated);
}

#[test]
fn nonbreaking_space_minimum_and_anywhere_override_are_distinct() {
    let (document, scene) = scene(
        "<div style='display:flex;width:100px'><div id='a' style='flex:1'>Long\u{a0}Nonbreaking\u{a0}Label</div></div><div style='display:flex;width:100px'><div id='b' style='flex:1;overflow-wrap:anywhere'>Long\u{a0}Nonbreaking\u{a0}Label</div></div>",
        100.0,
    );
    assert!(geometry(&document, &scene, "a").width > 100.0);
    close(geometry(&document, &scene, "b").width, 100.0);
    assert!(geometry(&document, &scene, "b").height > 20.0);
}

#[test]
fn zero_border_box_basis_retains_negative_inner_base_until_flexing() {
    let (document, scene) = scene(
        "<div style='display:flex;width:200px'><div id='a' style='box-sizing:border-box;flex:1 1 0;min-width:0;padding:0 10px;height:20px'></div><div id='b' style='box-sizing:border-box;flex:1 1 0;min-width:0;height:20px'></div></div>",
        200.0,
    );
    close(geometry(&document, &scene, "a").width, 100.0);
    close(geometry(&document, &scene, "b").width, 100.0);
    close(geometry(&document, &scene, "b").x, 100.0);
}

#[test]
fn column_cross_width_respects_border_box_padding() {
    let (document, scene) = scene(
        "<div style='display:flex;flex-direction:column;width:200px;align-items:flex-start'><div id='a' style='box-sizing:border-box;width:100px;padding:10px;flex:0 0 20px'></div></div>",
        200.0,
    );
    close(geometry(&document, &scene, "a").width, 100.0);
}

#[test]
fn exactly_the_item_limit_does_not_claim_truncation() {
    let source = format!(
        "<div style='display:flex;flex-wrap:wrap;width:200px'>{}</div>",
        "<div style='width:10px;height:1px;flex-shrink:0'></div>"
            .repeat(phos::flex::MAX_FLEX_ITEMS)
    );
    let (_, scene) = scene(&source, 200.0);
    assert!(!scene.truncated);
}

#[test]
fn absolute_children_use_zero_order_for_flex_and_grid_painting() {
    // CSS Display §3 (https://www.w3.org/TR/css-display-4/#order-property):
    // out-of-flow children
    // participate in painting as order zero, rather than using their order
    // declarations as though they were flex/grid items. Chrome 154 also
    // confirms both displays for the two in-flow order cases below.
    for display in ["flex", "grid"] {
        for flow_order in [-10, 10] {
            let source = format!(
                "<div style='display:{display};position:relative;width:100px;height:20px;grid-template-columns:100px'><div id='first' style='position:absolute;left:0;top:0;width:40px;height:20px;z-index:0;order:100;background:blue'></div><div id='flow' style='position:relative;z-index:0;width:100px;height:20px;order:{flow_order};background:red'></div><div id='second' style='position:absolute;left:40px;top:0;width:40px;height:20px;z-index:0;order:-100;background:green'></div></div>"
            );
            let (document, scene) = scene(&source, 100.0);
            close(geometry(&document, &scene, "first").x, 0.0);
            close(geometry(&document, &scene, "second").x, 40.0);
            let painted: Vec<_> = scene
                .primitives
                .iter()
                .filter_map(|primitive| match primitive {
                    Primitive::Box {
                        background: Some(color),
                        ..
                    } => Some((color.0, color.1, color.2)),
                    _ => None,
                })
                .collect();
            let red = (255, 0, 0);
            let blue = (0, 0, 255);
            let green = (0, 128, 0);
            let expected = if flow_order < 0 {
                vec![red, blue, green]
            } else {
                vec![blue, green, red]
            };
            assert_eq!(painted, expected, "{display}, order:{flow_order}");
            assert!(!scene.truncated);
        }
    }
}

#[test]
fn auto_column_wrap_uses_maximum_but_not_minimum_height_to_collect_lines() {
    for (height_rule, expected_height, wrapped) in [
        ("min-height:100px", 160.0, false),
        ("max-height:100px", 80.0, true),
        ("height:100px", 100.0, true),
    ] {
        let source = format!(
            "<div id='parent' style='display:flex;flex-direction:column;flex-wrap:wrap;width:100px;{height_rule}'><div id='a' style='flex:0 0 40px;width:20px'></div><div id='b' style='flex:0 0 40px;width:20px'></div><div id='c' style='flex:0 0 40px;width:20px'></div><div id='d' style='flex:0 0 40px;width:20px'></div></div>"
        );
        let (document, scene) = scene(&source, 100.0);
        close(
            geometry(&document, &scene, "parent").height,
            expected_height,
        );
        for id in ["a", "b", "c", "d"] {
            close(geometry(&document, &scene, id).height, 40.0);
        }
        close(geometry(&document, &scene, "a").y, 0.0);
        close(geometry(&document, &scene, "b").y, 40.0);
        close(
            geometry(&document, &scene, "c").y,
            if wrapped { 0.0 } else { 80.0 },
        );
        close(
            geometry(&document, &scene, "d").y,
            if wrapped { 40.0 } else { 120.0 },
        );
        close(
            geometry(&document, &scene, "c").x,
            if wrapped { 50.0 } else { 0.0 },
        );
        assert!(!scene.truncated);
    }
}
