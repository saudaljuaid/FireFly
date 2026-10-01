use phos::dom::{Document, NodeId, NodeKind};
use phos::layout::{BoxGeometry, BoxKind, ImageSource, Primitive, Scene};
use phos::{css, html, layout, paint, style, values};

fn render(source: &str, width: f32) -> (Document, Vec<style::ComputedStyle>, Scene) {
    let document = html::parse(&format!(
        "<style>html,body,div,p{{margin:0;padding:0}}{source}"
    ))
    .unwrap();
    let styles = style::compute_with_viewport(
        &document,
        &css::parse(&document.stylesheets()),
        values::Viewport {
            width,
            height: None,
        },
    );
    let scene = layout::layout(&document, &styles, width);
    (document, styles, scene)
}

fn node(document: &Document, class: &str) -> NodeId {
    document
        .nodes
        .iter()
        .position(|node| {
            matches!(&node.kind,
        NodeKind::Element(element) if element.has_class(class))
        })
        .unwrap()
}

fn geometry<'a>(document: &Document, scene: &'a Scene, class: &str) -> &'a BoxGeometry {
    let id = node(document, class);
    scene
        .boxes
        .iter()
        .find(|geometry| geometry.node == Some(id) && geometry.kind == BoxKind::Element)
        .unwrap()
}

fn close(actual: f32, expected: f32) {
    assert!((actual - expected).abs() < 0.04, "{actual} != {expected}");
}

fn rectangle(geometry: &BoxGeometry, expected: [f32; 4]) {
    for (actual, expected) in [geometry.x, geometry.y, geometry.width, geometry.height]
        .into_iter()
        .zip(expected)
    {
        close(actual, expected);
    }
}

fn finite(scene: &Scene) {
    assert!(scene.width.is_finite() && scene.height.is_finite());
    assert!(scene.boxes.iter().all(|geometry| {
        [geometry.x, geometry.y, geometry.width, geometry.height]
            .iter()
            .all(|value| value.is_finite())
    }));
    assert!(scene.primitives.len() <= 200_000);
}

fn image_source() -> ImageSource {
    use base64::Engine;
    ImageSource {
        href: format!(
            "data:image/png;base64,{}",
            base64::engine::general_purpose::STANDARD
                .encode(include_bytes!("render/two-pixels.png"))
        ),
        width: 80.0,
        height: 40.0,
    }
}

#[test]
fn fixed_and_fractional_tracks_and_spans_have_exact_area_geometry() {
    let (document, styles, scene) = render(
        ".grid{display:grid;width:600px;grid-template-columns:100px 1fr 2fr;grid-template-rows:40px 60px;column-gap:10px;row-gap:8px}.b{grid-column:2/span 2}.c{grid-column:3;grid-row:2}</style><div class=grid><div class=a></div><div class=b></div><div class=c></div></div>",
        700.0,
    );
    assert_eq!(
        styles[node(&document, "grid")].grid_template_columns.len(),
        3
    );
    rectangle(geometry(&document, &scene, "a"), [0.0, 0.0, 100.0, 40.0]);
    rectangle(geometry(&document, &scene, "b"), [110.0, 0.0, 490.0, 40.0]);
    rectangle(geometry(&document, &scene, "c"), [280.0, 48.0, 320.0, 60.0]);
    close(geometry(&document, &scene, "grid").height, 108.0);
    assert!(!scene.truncated);
    finite(&scene);
}

#[test]
fn automatic_items_skip_explicit_areas_and_implicit_rows_cycle() {
    let (document, _, scene) = render(
        ".grid{display:grid;width:220px;grid-template-columns:100px 100px;column-gap:20px;row-gap:5px;grid-auto-rows:20px 30px}.reserved{grid-row:1;grid-column:1}</style><div class=grid><div class=a></div><div class=reserved></div><div class=b></div><div class=c></div><div class=d></div><div class=e></div></div>",
        300.0,
    );
    rectangle(
        geometry(&document, &scene, "reserved"),
        [0.0, 0.0, 100.0, 20.0],
    );
    rectangle(geometry(&document, &scene, "a"), [120.0, 0.0, 100.0, 20.0]);
    rectangle(geometry(&document, &scene, "b"), [0.0, 25.0, 100.0, 30.0]);
    rectangle(geometry(&document, &scene, "d"), [0.0, 60.0, 100.0, 20.0]);
    close(geometry(&document, &scene, "grid").height, 80.0);
}

#[test]
fn grid_content_item_alignment_and_auto_margins_are_separate() {
    let (document, _, scene) = render(
        ".grid{display:grid;width:300px;grid-template-columns:50px 50px;grid-template-rows:60px;justify-content:space-between;justify-items:center;align-items:center}.a,.b{width:40px;height:20px}.b{margin-left:auto}</style><div class=grid><div class=a></div><div class=b></div></div>",
        400.0,
    );
    rectangle(geometry(&document, &scene, "a"), [5.0, 20.0, 40.0, 20.0]);
    rectangle(geometry(&document, &scene, "b"), [260.0, 20.0, 40.0, 20.0]);
}

#[test]
fn rtl_mirrors_tracks_and_logical_item_alignment_without_dom_reversal() {
    let (document, _, scene) = render(
        ".grid{display:grid;direction:rtl;width:200px;grid-template-columns:60px 80px;column-gap:10px;grid-template-rows:30px;justify-content:start;justify-items:start}.a,.b{width:20px}</style><div class=grid><div class=a></div><div class=b></div></div>",
        300.0,
    );
    rectangle(geometry(&document, &scene, "a"), [180.0, 0.0, 20.0, 30.0]);
    rectangle(geometry(&document, &scene, "b"), [110.0, 0.0, 20.0, 30.0]);
    assert!(node(&document, "a") < node(&document, "b"));
}

#[test]
fn percentage_padding_and_box_sizing_resolve_against_each_grid_area() {
    let (document, _, scene) = render(
        ".grid{display:grid;width:300px;grid-template-columns:1fr 1fr;grid-template-rows:80px}.a{padding:10%;border:2px solid;box-sizing:border-box}.b{width:50%;height:50%;padding:5px;box-sizing:border-box}</style><div class=grid><div class=a></div><div class=b></div></div>",
        400.0,
    );
    rectangle(geometry(&document, &scene, "a"), [0.0, 0.0, 150.0, 80.0]);
    rectangle(geometry(&document, &scene, "b"), [150.0, 0.0, 75.0, 40.0]);
}

#[test]
fn grid_contains_flex_and_nested_grid_with_shared_border_box_constraints() {
    let (document, _, scene) = render(
        ".outer{display:grid;width:400px;grid-template-columns:100px minmax(0,1fr);gap:20px;grid-template-rows:80px}.flex{display:flex;padding:10px;border:2px solid;gap:10px}.innera,.nested{flex:1;min-width:0}.nested{display:grid;grid-template-columns:1fr 2fr;gap:10px}</style><div class=outer><div class=left></div><div class=flex><div class=innera></div><div class=nested><div class=innerb></div><div class=innerc></div></div></div></div>",
        500.0,
    );
    rectangle(
        geometry(&document, &scene, "flex"),
        [120.0, 0.0, 280.0, 80.0],
    );
    rectangle(
        geometry(&document, &scene, "innera"),
        [132.0, 12.0, 123.0, 56.0],
    );
    rectangle(
        geometry(&document, &scene, "nested"),
        [265.0, 12.0, 123.0, 56.0],
    );
    close(geometry(&document, &scene, "innerb").width, 113.0 / 3.0);
    close(geometry(&document, &scene, "innerc").width, 226.0 / 3.0);
    assert!(!scene.truncated);
}

#[test]
fn grid_breakpoints_change_track_count_and_exact_item_placement() {
    let source = ".grid{display:grid;grid-template-columns:repeat(3,minmax(0,1fr));gap:12px;grid-auto-rows:40px}@media(max-width:480px){.grid{grid-template-columns:1fr}}@media(min-width:481px) and (max-width:760px){.grid{grid-template-columns:1fr 1fr}}</style><div class=grid><div class=a></div><div class=b></div><div class=c></div></div>";
    for (width, columns, expected_b, expected_c) in [
        (
            320.0,
            1,
            [0.0, 52.0, 320.0, 40.0],
            [0.0, 104.0, 320.0, 40.0],
        ),
        (
            640.0,
            2,
            [326.0, 0.0, 314.0, 40.0],
            [0.0, 52.0, 314.0, 40.0],
        ),
        (
            900.0,
            3,
            [304.0, 0.0, 292.0, 40.0],
            [608.0, 0.0, 292.0, 40.0],
        ),
    ] {
        let (document, styles, scene) = render(source, width);
        assert_eq!(
            styles[node(&document, "grid")].grid_template_columns.len(),
            columns
        );
        rectangle(geometry(&document, &scene, "b"), expected_b);
        rectangle(geometry(&document, &scene, "c"), expected_c);
        assert!(!scene.truncated);
    }
}

#[test]
fn positioned_overlays_use_padded_grid_item_containing_blocks_and_clip() {
    let (document, _, scene) = render(
        ".grid{display:grid;width:240px;grid-template-columns:100px 120px;gap:20px;grid-template-rows:80px}.b{position:relative;padding:10px;border:2px solid;border-radius:8px;overflow:hidden}.overlay{position:absolute;right:5px;top:5px;width:30px;height:20px;background:red}</style><div class=grid><div class=a></div><div class=b><div class=overlay></div></div></div>",
        300.0,
    );
    rectangle(geometry(&document, &scene, "b"), [120.0, 0.0, 120.0, 80.0]);
    rectangle(
        geometry(&document, &scene, "overlay"),
        [203.0, 7.0, 30.0, 20.0],
    );
    assert!(scene.primitives.iter().any(|primitive|matches!(primitive,Primitive::ClipStart{x,y,width,height,..} if *x==122.0&&*y==2.0&&*width==116.0&&*height==76.0)));
    assert!(!scene.truncated);
}

#[test]
fn reordered_overlapping_grid_items_use_order_and_static_z_index_in_painting() {
    let (document, _, scene) = render(
        ".grid{display:grid;width:100px;grid-template-columns:100px;grid-template-rows:40px}.a,.b,.c{grid-row:1;grid-column:1}.a{background:red;order:2}.b{background:blue;order:-1}.c{background:green;z-index:3}</style><div class=grid><div class=a></div><div class=b></div><div class=c></div></div>",
        200.0,
    );
    for class in ["a", "b", "c"] {
        rectangle(geometry(&document, &scene, class), [0.0, 0.0, 100.0, 40.0]);
    }
    let colors: Vec<_> = scene
        .primitives
        .iter()
        .filter_map(|primitive| match primitive {
            Primitive::Box {
                background: Some(color),
                ..
            } => Some(*color),
            _ => None,
        })
        .collect();
    assert_eq!(
        colors,
        [
            style::Color(0, 0, 255, 255),
            style::Color(255, 0, 0, 255),
            style::Color(0, 128, 0, 255)
        ]
    );
}

#[test]
fn mixed_font_baselines_share_a_row_and_intrinsic_height_accounts_for_descent() {
    let (document, _, scene) = render(
        ".grid{display:grid;width:300px;grid-template-columns:1fr 1fr;align-items:baseline}.a{font-size:12px;line-height:18px}.b{font-size:32px;line-height:40px}</style><div class=grid><div class=a>Small label</div><div class=b>Large</div></div>",
        400.0,
    );
    let baseline = |class| {
        let id = node(&document, class);
        scene
            .runs
            .iter()
            .find(|run| document.nodes[run.node].parent == Some(id))
            .unwrap()
            .baseline
    };
    close(baseline("a"), baseline("b"));
    assert!(geometry(&document, &scene, "grid").height >= 40.0);
    assert!(geometry(&document, &scene, "a").y > geometry(&document, &scene, "b").y);
}

#[test]
fn intrinsic_images_keep_ratio_and_explicit_stretch_is_deliberate() {
    let document=html::parse("<style>html,body{margin:0}.grid{display:grid;width:300px;grid-template-columns:1fr 1fr;grid-template-rows:100px}.stretched{justify-self:stretch;align-self:stretch}</style><div class=grid><img class=natural><img class=stretched></div>").unwrap();
    let styles = style::compute(&document, &css::parse(&document.stylesheets()));
    let mut images = vec![None; document.nodes.len()];
    for class in ["natural", "stretched"] {
        images[node(&document, class)] = Some(image_source());
    }
    let scene = layout::layout_with_images(&document, &styles, &images, 400.0);
    rectangle(
        geometry(&document, &scene, "natural"),
        [0.0, 0.0, 80.0, 40.0],
    );
    rectangle(
        geometry(&document, &scene, "stretched"),
        [150.0, 0.0, 150.0, 100.0],
    );
    assert_eq!(
        scene
            .primitives
            .iter()
            .filter(|primitive| matches!(primitive, Primitive::Image { .. }))
            .count(),
        2
    );
}

#[test]
fn anonymous_text_hidden_and_absolute_items_preserve_shared_text_runs() {
    let (document, _, scene) = render(
        ".grid{display:grid;grid-template-columns:minmax(0,1fr) minmax(0,1fr);gap:8px}.hidden{display:none}.overlay{position:absolute;left:0;top:0;width:10px;height:10px}</style><div class=grid>Combining é NBSP a&nbsp;b 漢字 <div class=label>שלום Label طويل</div><div class=hidden>hidden</div><div class=overlay></div></div>",
        180.0,
    );
    assert!(scene.runs.iter().any(|run| run.text.content.contains('漢')));
    assert!(
        scene
            .runs
            .iter()
            .any(|run| run.text.direction == phos::text::Direction::Rtl)
    );
    assert!(
        !scene
            .runs
            .iter()
            .any(|run| run.text.content.contains("hidden"))
    );
    assert!(geometry(&document, &scene, "label").x > 0.0);
    finite(&scene);
    let svg = paint::to_svg(&scene);
    assert!(roxmltree::Document::parse(&svg).is_ok());
}

#[test]
fn malformed_grid_declarations_recover_and_extreme_placement_reports_truncation() {
    let (document, styles, scene) = render(
        ".grid{display:grid;grid-template-columns:80px 120px;grid-template-columns:repeat(999999,1fr);grid-template-rows:30px;gap:10px}.a{grid-column:257/span 2}.b{grid-column:2}</style><div class=grid><div class=a></div><div class=b></div></div>",
        300.0,
    );
    assert_eq!(
        styles[node(&document, "grid")].grid_template_columns.len(),
        2
    );
    assert!(scene.truncated);
    rectangle(geometry(&document, &scene, "b"), [90.0, 0.0, 120.0, 30.0]);
    finite(&scene);
}

#[test]
fn many_items_and_nested_grid_remain_finite_with_explicit_bounds() {
    let mut source=".grid{display:grid;grid-template-columns:repeat(16,minmax(0,1fr));grid-auto-rows:4px;gap:1px}</style><div class=grid>".to_string();
    for index in 0..512 {
        source.push_str(&format!("<div class=i{index}></div>"));
    }
    source.push_str("</div>");
    let (document, _, scene) = render(&source, 400.0);
    rectangle(
        geometry(&document, &scene, "i511"),
        [375.9375, 155.0, 24.0625, 4.0],
    );
    close(geometry(&document, &scene, "grid").height, 159.0);
    assert!(!scene.truncated);
    finite(&scene);
}

#[test]
fn inline_grid_intrinsic_contributions_match_keyword_border_box_layout() {
    let (document, styles, scene) = render(
        ".grid{display:inline-grid;grid-template-columns:auto auto;gap:10px;padding:0 4px;border:2px solid;box-sizing:border-box;width:max-content}.a{width:40px;height:20px}.b{width:80px;height:30px}</style><div>Before <span class=grid><div class=a></div><div class=b></div></span> after</div>",
        400.0,
    );
    let images = vec![None; document.nodes.len()];
    let cache = phos::intrinsic::IntrinsicCache::new(&document, &styles, &images);
    let id = node(&document, "grid");
    let content = cache.content(id);
    let contribution = cache.contribution(id);
    close(content.min_content, 130.0);
    close(content.max_content, 130.0);
    close(contribution.min_content, 142.0);
    close(contribution.max_content, 142.0);
    close(geometry(&document, &scene, "grid").width, 142.0);
    assert_eq!(cache.content(id), content);
    assert!(!cache.truncated.get());
    assert!(!scene.truncated);
}

#[test]
fn nested_grid_intrinsic_modes_use_shared_shaped_contributions() {
    let (document, styles, _) = render(
        ".grid{display:inline-grid;grid-template-columns:100px minmax(0,1fr);gap:10px}</style><span class=grid><div></div><div>alpha beta gamma</div></span>",
        400.0,
    );
    let images = vec![None; document.nodes.len()];
    let cache = phos::intrinsic::IntrinsicCache::new(&document, &styles, &images);
    let content = cache.content(node(&document, "grid"));
    close(content.min_content, 110.0);
    close(
        content.max_content,
        110.0 + phos::text::width("alpha beta gamma", 16.0, false),
    );
    assert!(!cache.truncated.get());
}

#[test]
fn min_width_zero_relaxes_an_automatic_flexible_track_minimum() {
    let token = "ThisLabelHasNoBreakOpportunityAndCannotFitInNinetyPixels";
    let base = format!(
        ".grid{{display:grid;width:180px;grid-template-columns:1fr 1fr}}.a{{white-space:nowrap}}.b{{height:20px}}</style><div class=grid><div class=a>{token}</div><div class=b></div></div>"
    );
    let (document, _, scene) = render(&base, 300.0);
    close(
        geometry(&document, &scene, "a").width,
        phos::text::width(token, 16.0, false),
    );
    close(geometry(&document, &scene, "b").width, 0.0);
    let relaxed = base.replace("white-space:nowrap", "white-space:nowrap;min-width:0");
    let (document, _, scene) = render(&relaxed, 300.0);
    close(geometry(&document, &scene, "a").width, 90.0);
    close(geometry(&document, &scene, "b").width, 90.0);
    assert!(
        scene
            .line_boxes
            .iter()
            .any(|line| line.advance > line.width)
    );
}

#[test]
fn column_auto_flow_and_negative_end_lines_have_exact_geometry() {
    let (document, _, scene) = render(
        ".grid{display:grid;width:240px;grid-template-rows:30px 40px;grid-auto-columns:50px 60px;gap:10px;grid-auto-flow:column}.span{grid-row:1/-1;grid-column:3}</style><div class=grid><div class=a></div><div class=b></div><div class=c></div><div class=d></div><div class=span></div></div>",
        300.0,
    );
    rectangle(geometry(&document, &scene, "a"), [0.0, 0.0, 50.0, 30.0]);
    rectangle(geometry(&document, &scene, "b"), [0.0, 40.0, 50.0, 40.0]);
    rectangle(geometry(&document, &scene, "c"), [60.0, 0.0, 60.0, 30.0]);
    rectangle(geometry(&document, &scene, "d"), [60.0, 40.0, 60.0, 40.0]);
    rectangle(
        geometry(&document, &scene, "span"),
        [130.0, 0.0, 50.0, 80.0],
    );
    assert!(!scene.truncated);
}

#[test]
fn definite_minimum_and_maximum_heights_constrain_fractional_rows_before_layout() {
    let (document, _, scene) = render(
        ".grid{display:grid;width:200px;min-height:100px;grid-template-rows:1fr 1fr;gap:10px}</style><div class=grid><div class=a></div><div class=b></div></div>",
        300.0,
    );
    close(geometry(&document, &scene, "grid").height, 100.0);
    rectangle(geometry(&document, &scene, "a"), [0.0, 0.0, 200.0, 45.0]);
    rectangle(geometry(&document, &scene, "b"), [0.0, 55.0, 200.0, 45.0]);
    let (document, _, scene) = render(
        ".grid{display:grid;width:200px;height:100px;max-height:70px;grid-template-rows:1fr 1fr;gap:10px}</style><div class=grid><div class=a></div><div class=b></div></div>",
        300.0,
    );
    rectangle(geometry(&document, &scene, "a"), [0.0, 0.0, 200.0, 30.0]);
    rectangle(geometry(&document, &scene, "b"), [0.0, 40.0, 200.0, 30.0]);
    close(geometry(&document, &scene, "grid").height, 70.0);
}

#[test]
fn deeply_nested_single_track_grid_retains_the_terminal_text_and_finite_boxes() {
    let mut source =
        ".grid{display:grid;grid-template-columns:minmax(0,1fr)}.leaf{height:24px}</style>"
            .to_string();
    for _ in 0..48 {
        source.push_str("<div class=grid>");
    }
    source.push_str("<div class=leaf>Terminal é 漢字</div>");
    for _ in 0..48 {
        source.push_str("</div>");
    }
    let (document, _, scene) = render(&source, 240.0);
    close(geometry(&document, &scene, "leaf").width, 240.0);
    close(geometry(&document, &scene, "leaf").height, 24.0);
    assert!(
        scene
            .runs
            .iter()
            .any(|run| run.text.content.contains("Terminal"))
    );
    assert!(!scene.truncated);
    finite(&scene);
}

#[test]
fn a_grid_baseline_comes_from_its_first_row_even_when_source_order_starts_later() {
    let (document, _, scene) = render(
        ".outer{display:flex;align-items:baseline;gap:10px}.peer{font-size:12px;line-height:18px}.grid{display:grid;grid-template-columns:100px;grid-template-rows:20px 40px}.bottom{grid-row:2;font-size:30px;line-height:36px}.top{grid-row:1;font-size:10px;line-height:16px}</style><div class=outer><div class=peer>Peer</div><div class=grid><div class=bottom>Bottom</div><div class=top>Top</div></div></div>",
        300.0,
    );
    let baseline = |class| {
        let id = node(&document, class);
        scene
            .runs
            .iter()
            .find(|run| document.nodes[run.node].parent == Some(id))
            .unwrap()
            .baseline
    };
    close(baseline("peer"), baseline("top"));
    assert!(baseline("bottom") > baseline("top"));
    close(geometry(&document, &scene, "grid").height, 60.0);
    assert!(!scene.truncated);
}

#[test]
fn spanning_flexible_auto_minimum_and_definite_preferred_contribution_differ() {
    let source = ".grid{display:grid;width:200px;grid-template-columns:1fr 1fr;grid-auto-rows:24px}.a{grid-column:1/span 2;white-space:nowrap}</style><div class=grid><div class=a>UnbrokenLabelThatIsWiderThanTwoHundredPixels</div><div class=b></div><div class=c></div></div>";
    let (document, _, scene) = render(source, 300.0);
    close(geometry(&document, &scene, "a").width, 200.0);
    close(geometry(&document, &scene, "b").width, 100.0);
    close(geometry(&document, &scene, "c").x, 100.0);
    let preferred = source.replace("white-space:nowrap", "white-space:nowrap;width:500px");
    let (document, _, scene) = render(&preferred, 300.0);
    close(geometry(&document, &scene, "b").width, 250.0);
    close(geometry(&document, &scene, "c").x, 250.0);
    let fixed_min = preferred.replace(
        "grid-template-columns:1fr 1fr",
        "grid-template-columns:minmax(0,1fr) minmax(0,1fr)",
    );
    let (document, _, scene) = render(&fixed_min, 300.0);
    close(geometry(&document, &scene, "a").width, 500.0);
    close(geometry(&document, &scene, "b").width, 100.0);
}

#[test]
fn hidden_overflow_and_explicit_zero_minimum_keep_decorations_in_track_minima() {
    let source = ".grid{display:grid;width:190px;grid-template-columns:1fr 1fr;grid-template-rows:40px}.a{white-space:nowrap;padding:10px;border:2px solid;overflow:hidden}</style><div class=grid><div class=a>LongUnbrokenLabelThatCannotFit</div><div class=b></div></div>";
    let (document, _, scene) = render(source, 300.0);
    rectangle(geometry(&document, &scene, "a"), [0.0, 0.0, 95.0, 40.0]);
    rectangle(geometry(&document, &scene, "b"), [95.0, 0.0, 95.0, 40.0]);
    assert!(scene.primitives.iter().any(
        |primitive| matches!(primitive,Primitive::ClipStart{width,..} if (*width-91.0).abs()<0.01)
    ));
    let explicit = source.replace("overflow:hidden", "min-width:0");
    let (document, _, scene) = render(&explicit, 300.0);
    close(geometry(&document, &scene, "a").width, 95.0);
    assert!(
        scene
            .line_boxes
            .iter()
            .any(|line| line.advance > line.width)
    );
}

#[test]
fn a_replaced_items_definite_height_transfers_through_its_intrinsic_ratio() {
    let document=html::parse("<style>html,body{margin:0}.grid{display:grid;width:300px;grid-template-columns:1fr 1fr;grid-template-rows:80px}img{height:80px}</style><div class=grid><img class=image><div class=other></div></div>").unwrap();
    let styles = style::compute(&document, &css::parse(&document.stylesheets()));
    let mut images = vec![None; document.nodes.len()];
    images[node(&document, "image")] = Some(image_source());
    let scene = layout::layout_with_images(&document, &styles, &images, 400.0);
    rectangle(
        geometry(&document, &scene, "image"),
        [0.0, 0.0, 160.0, 80.0],
    );
    rectangle(
        geometry(&document, &scene, "other"),
        [160.0, 0.0, 140.0, 80.0],
    );
    assert!(!scene.truncated);
}

#[test]
fn vertical_automatic_minima_respect_hidden_overflow_zero_minimum_and_flexible_spans() {
    let source = ".grid{display:grid;width:180px;height:100px;grid-template-columns:1fr;grid-template-rows:1fr 1fr}.item{padding:10px;border:2px solid}.tall{height:200px}</style><div class=grid><div class='item a'><div class=tall></div></div><div class='item b'><div class=tall></div></div></div>";
    let (document, _, scene) = render(source, 240.0);
    close(geometry(&document, &scene, "a").height, 224.0);
    close(geometry(&document, &scene, "b").y, 224.0);
    for escape in ["overflow:hidden", "min-height:0"] {
        let source = source.replace("padding:10px", &format!("{escape};padding:10px"));
        let (document, _, scene) = render(&source, 240.0);
        rectangle(geometry(&document, &scene, "a"), [0.0, 0.0, 180.0, 50.0]);
        rectangle(geometry(&document, &scene, "b"), [0.0, 50.0, 180.0, 50.0]);
        assert!(!scene.truncated);
    }
    let (document, _, scene) = render(
        ".grid{display:grid;width:180px;height:100px;grid-template-columns:1fr 1fr;grid-template-rows:1fr 1fr}.a{grid-row:1/span 2}.tall{height:200px}</style><div class=grid><div class=a><div class=tall></div></div><div class=b></div><div class=c></div></div>",
        240.0,
    );
    rectangle(geometry(&document, &scene, "a"), [0.0, 0.0, 90.0, 100.0]);
    rectangle(geometry(&document, &scene, "b"), [90.0, 0.0, 90.0, 50.0]);
    rectangle(geometry(&document, &scene, "c"), [90.0, 50.0, 90.0, 50.0]);
}

#[test]
fn spanning_areas_include_distributed_gaps_for_percentage_edges_and_heights() {
    let (document, _, scene) = render(
        ".grid{display:grid;width:400px;height:260px;grid-template-columns:100px 100px;grid-template-rows:60px 100px;gap:10px;justify-content:space-between;align-content:space-between}.span{grid-column:1/span 2;grid-row:1/span 2;padding:10%;margin:5%;height:50%;border:2px solid;overflow:hidden}.leaf{height:10px}</style><div class=grid><div class=span><div class=leaf></div></div></div>",
        500.0,
    );
    rectangle(
        geometry(&document, &scene, "span"),
        [20.0, 20.0, 360.0, 214.0],
    );
    rectangle(
        geometry(&document, &scene, "leaf"),
        [62.0, 62.0, 276.0, 10.0],
    );
    close(geometry(&document, &scene, "grid").height, 260.0);
    assert!(!scene.truncated);
}

#[test]
fn clipping_a_grid_item_keeps_its_existing_text_baseline() {
    let (document, _, scene) = render(
        ".grid{display:grid;width:300px;grid-template-columns:1fr 1fr;align-items:baseline}.a{font-size:12px;line-height:18px;height:50px;overflow:hidden}.b{font-size:32px;line-height:40px}</style><div class=grid><div class=a>Small label</div><div class=b>Large</div></div>",
        400.0,
    );
    let baseline = |class| {
        let id = node(&document, class);
        scene
            .runs
            .iter()
            .find(|run| document.nodes[run.node].parent == Some(id))
            .unwrap()
            .baseline
    };
    close(baseline("a"), baseline("b"));
    assert!(!scene.truncated);
}

#[test]
fn a_grid_baseline_uses_the_first_occupied_row_and_logical_grid_order() {
    let (document, _, scene) = render(
        ".outer{display:flex;align-items:baseline;gap:10px}.peer{font-size:12px;line-height:18px}.grid{display:grid;grid-template-columns:100px 100px;grid-template-rows:20px 40px 40px}.late{grid-row:3;grid-column:2;font-size:30px;line-height:36px}.right{grid-row:2;grid-column:2;font-size:30px;line-height:36px}.left{grid-row:2;grid-column:1;font-size:10px;line-height:16px}</style><div class=outer><div class=peer>Peer</div><div class=grid><div class=late>Late</div><div class=right>Right</div><div class=left>Left</div></div></div>",
        340.0,
    );
    let baseline = |class| {
        let id = node(&document, class);
        scene
            .runs
            .iter()
            .find(|run| document.nodes[run.node].parent == Some(id))
            .unwrap()
            .baseline
    };
    close(baseline("peer"), baseline("left"));
    assert!(baseline("right") > baseline("left"));
    assert!(baseline("late") > baseline("right"));
    assert!(!scene.truncated);
}

#[test]
fn used_height_constraints_do_not_create_a_percentage_gap_or_track_basis() {
    let source = ".grid{display:grid;width:180px;min-height:100px;grid-template-columns:1fr;grid-template-rows:1fr 1fr;row-gap:10%}.item{min-height:0}.leaf{height:20px}</style><div class=grid><div class='item a'><div class=leaf></div></div><div class='item b'><div class=leaf></div></div></div>";
    let (document, _, scene) = render(source, 240.0);
    rectangle(geometry(&document, &scene, "a"), [0.0, 0.0, 180.0, 50.0]);
    rectangle(geometry(&document, &scene, "b"), [0.0, 50.0, 180.0, 50.0]);
    let explicit = source.replace("min-height:100px", "height:100px");
    let (document, _, scene) = render(&explicit, 240.0);
    rectangle(geometry(&document, &scene, "a"), [0.0, 0.0, 180.0, 45.0]);
    rectangle(geometry(&document, &scene, "b"), [0.0, 55.0, 180.0, 45.0]);
    // Final Grid areas are definite for item descendants even when the
    // automatic container's used minimum did not resolve its percentage gap.
    let nested_percentage = source.replace(".leaf{height:20px}", ".leaf{height:50%}");
    let (document, _, scene) = render(&nested_percentage, 240.0);
    close(geometry(&document, &scene, "leaf").height, 25.0);
    let explicit = nested_percentage.replace("min-height:100px", "height:100px");
    let (document, _, scene) = render(&explicit, 240.0);
    close(geometry(&document, &scene, "leaf").height, 22.5);
    for tracks in [
        "25% 75%",
        "calc(25% + 0px) calc(75% + 0px)",
        "minmax(10px,25%) minmax(10px,75%)",
    ] {
        let percentages = source.replace("1fr 1fr;row-gap:10%", &format!("{tracks};row-gap:0"));
        let (document, _, scene) = render(&percentages, 240.0);
        rectangle(geometry(&document, &scene, "a"), [0.0, 0.0, 180.0, 50.0]);
        rectangle(geometry(&document, &scene, "b"), [0.0, 50.0, 180.0, 50.0]);
        let explicit = percentages.replace("min-height:100px", "height:100px");
        let (document, _, scene) = render(&explicit, 240.0);
        rectangle(geometry(&document, &scene, "a"), [0.0, 0.0, 180.0, 25.0]);
        rectangle(geometry(&document, &scene, "b"), [0.0, 25.0, 180.0, 75.0]);
        assert!(!scene.truncated);
    }
}

#[test]
fn constrained_replaced_heights_transfer_the_used_intrinsic_ratio_inside_layout_items() {
    for (formatting, height, expected) in [
        (
            "display:grid;grid-template-columns:1fr 1fr;grid-template-rows:80px",
            "height:80px;max-height:40px",
            [80.0, 40.0],
        ),
        (
            "display:grid;grid-template-columns:1fr 1fr;grid-template-rows:80px",
            "height:40px;min-height:80px",
            [160.0, 80.0],
        ),
        (
            "display:flex;align-items:flex-start",
            "height:80px;max-height:40px",
            [80.0, 40.0],
        ),
        (
            "display:flex;align-items:flex-start",
            "height:40px;min-height:80px",
            [160.0, 80.0],
        ),
    ] {
        let document=html::parse(&format!("<style>html,body{{margin:0}}.layout{{{formatting};width:300px}}img{{{height}}}</style><div class=layout><img class=image><div class=other></div></div>")).unwrap();
        let styles = style::compute(&document, &css::parse(&document.stylesheets()));
        let mut images = vec![None; document.nodes.len()];
        images[node(&document, "image")] = Some(image_source());
        let scene = layout::layout_with_images(&document, &styles, &images, 400.0);
        close(geometry(&document, &scene, "image").width, expected[0]);
        close(geometry(&document, &scene, "image").height, expected[1]);
        assert!(!scene.truncated);
    }
    for formatting in [
        "display:grid;grid-template-columns:1fr 1fr;grid-template-rows:80px",
        "display:flex;align-items:flex-start",
    ] {
        for (constraint, expected) in [
            ("min-height:80px", [160.0, 80.0]),
            ("max-height:.5px", [1.0, 0.5]),
            ("min-width:3px;max-height:.5px", [3.0, 0.5]),
        ] {
            let document=html::parse(&format!("<style>html,body{{margin:0}}.layout{{{formatting};width:300px}}img{{{constraint}}}</style><div class=layout><img class=image><div class=other></div></div>")).unwrap();
            let styles = style::compute(&document, &css::parse(&document.stylesheets()));
            let mut native = image_source();
            native.width = 2.0;
            native.height = 1.0;
            let mut images = vec![None; document.nodes.len()];
            images[node(&document, "image")] = Some(native);
            let scene = layout::layout_with_images(&document, &styles, &images, 400.0);
            close(geometry(&document, &scene, "image").width, expected[0]);
            close(geometry(&document, &scene, "image").height, expected[1]);
            assert!(!scene.truncated);
        }
    }
}

#[test]
fn grid_item_percentage_descendants_follow_stretch_and_preferred_height_definiteness() {
    for (rows, container, alignment, item_height, item, child) in [
        ("100px", "", "start", "", 20.0, 20.0),
        ("100px", "", "stretch", "", 100.0, 50.0),
        ("auto", "", "start", "", 20.0, 20.0),
        ("auto", "", "stretch", "", 20.0, 10.0),
        ("auto", "min-height:100px", "start", "", 20.0, 20.0),
        ("auto", "min-height:100px", "stretch", "", 100.0, 50.0),
        ("100px", "", "start", "height:20px", 20.0, 10.0),
        ("100px", "", "stretch", "height:20px", 20.0, 10.0),
        ("100px", "", "start", "height:50%", 50.0, 25.0),
        (
            "auto",
            "min-height:100px",
            "start",
            "height:50%",
            50.0,
            25.0,
        ),
    ] {
        let source = format!(
            ".grid{{display:grid;width:200px;grid-template-columns:1fr;grid-template-rows:{rows};align-items:{alignment};{container}}}.item{{{item_height}}}.child{{height:50%}}.leaf{{height:20px}}</style><div class=grid><div class=item><div class=child><div class=leaf></div></div></div></div>"
        );
        let (document, _, scene) = render(&source, 300.0);
        close(geometry(&document, &scene, "item").height, item);
        close(geometry(&document, &scene, "child").height, child);
        assert!(
            !scene.truncated,
            "{rows} {container} {alignment} {item_height}"
        );
    }
}
