use phos::dom::{Document, NodeId};
use phos::layout::{BoxGeometry, BoxKind, Scene};
use phos::{
    css, html, intrinsic::IntrinsicCache, layout, paint, sizing::IntrinsicSizes, style, text,
    values::Viewport,
};
fn node(document: &Document, id: &str) -> NodeId {
    document
        .nodes
        .iter()
        .enumerate()
        .find(|(i, _)| {
            document
                .element(*i)
                .is_some_and(|e| e.attribute("id") == Some(id))
        })
        .unwrap()
        .0
}
fn geometry<'a>(document: &Document, scene: &'a Scene, id: &str) -> &'a BoxGeometry {
    let id = node(document, id);
    scene
        .boxes
        .iter()
        .find(|b| b.node == Some(id) && b.kind == BoxKind::Element)
        .unwrap()
}
fn render(source: &str, width: f32) -> (Document, Scene) {
    let document = html::parse(&format!("<style>html,body{{margin:0}}</style>{source}")).unwrap();
    let styles = style::compute_with_viewport(
        &document,
        &css::parse(&document.stylesheets()),
        Viewport {
            width,
            height: None,
        },
    );
    let scene = layout::layout(&document, &styles, width);
    (document, scene)
}
fn close(a: f32, b: f32) {
    assert!((a - b).abs() < 0.02, "{a} != {b}");
}
#[test]
fn anonymous_text_across_comments_is_one_shaped_flex_item() {
    let (document, scene) = render(
        "<div id=p style='display:flex;gap:40px;align-items:flex-start'>Alpha<!-- a comment -->Beta<div id=b style='width:20px;height:10px'></div></div>",
        300.0,
    );
    close(
        geometry(&document, &scene, "b").x,
        text::width("AlphaBeta", 16.0, false) + 40.0,
    );
    assert_eq!(
        scene
            .runs
            .iter()
            .map(|r| r.text.content.as_str())
            .collect::<String>(),
        "AlphaBeta"
    );
    assert!(!scene.truncated);
}
#[test]
fn anonymous_text_member_limit_is_reported_without_silent_disappearance() {
    let source = format!(
        "<div style='display:flex'>{}</div>",
        "a<!-- bounded -->".repeat(4100)
    );
    let (_, scene) = render(&source, 300.0);
    assert!(scene.truncated);
    assert!(paint::to_svg(&scene).contains("truncated=\"true\""));
    assert!(scene.boxes.len() < 5000);
}
#[test]
fn auto_inline_and_absolute_widths_cannot_shrink_below_min_content() {
    let label = "UnbreakableLongLabel";
    let intrinsic = text::width(label, 16.0, false);
    let (document, scene) = render(
        &format!(
            "<div style='width:40px;position:relative'><span id=inline style='display:inline-block'>{label}</span><div id=absolute style='position:absolute;left:0;top:40px'>{label}</div></div>"
        ),
        100.0,
    );
    close(geometry(&document, &scene, "inline").width, intrinsic);
    close(geometry(&document, &scene, "absolute").width, intrinsic);
    assert!(!scene.truncated);
    assert_eq!(IntrinsicSizes::new(60.0, 100.0).shrink_to_fit(20.0), 60.0);
    assert_eq!(IntrinsicSizes::new(60.0, 100.0).shrink_to_fit(80.0), 80.0);
}
#[test]
fn positioned_intrinsic_keywords_keep_content_box_semantics() {
    let (document, scene) = render(
        "<div style='position:relative;width:300px'><div id=p style='position:absolute;width:min-content;box-sizing:border-box;padding:0 4px;border:2px solid'>Alpha Beta</div></div>",
        300.0,
    );
    close(
        geometry(&document, &scene, "p").width,
        text::width("Alpha", 16.0, false) + 12.0,
    );
}
#[test]
fn intrinsic_flex_basis_and_keyword_constraints_are_shared() {
    let source = "<span id=p style='display:inline-flex;gap:10px;padding:0 3px;border:1px solid;box-sizing:border-box;width:max-content'><span style='flex:0 0 80px'>a</span><span style='flex:0 0 50px'>b</span></span>";
    let (document, scene) = render(source, 300.0);
    close(geometry(&document, &scene, "p").width, 148.0);
    let styles = style::compute(&document, &css::Stylesheet::default());
    let images = vec![None; document.nodes.len()];
    let cache = IntrinsicCache::new(&document, &styles, &images);
    close(cache.contribution(node(&document, "p")).max_content, 148.0);
}
#[test]
fn cascade_work_exhaustion_preserves_prefix_and_reports_truncation() {
    let css = ".missing {width:20px}".repeat(5000);
    let source = format!(
        "<style>{css}</style><div id=prefix style='color:red'>Prefix</div>{}",
        "<div>Later</div>".repeat(1000)
    );
    let document = html::parse(&source).unwrap();
    let sheet = css::parse(&document.stylesheets());
    let computed = style::compute_with_status(&document, &sheet, Viewport::default());
    assert!(computed.truncated);
    assert_eq!(
        computed.styles[node(&document, "prefix")].color,
        style::Color(255, 0, 0, 255)
    );
    assert!(
        phos::render(&source, 300.0)
            .unwrap()
            .contains("truncated=\"true\"")
    );
}
#[test]
fn inert_template_author_styles_do_not_consume_cascade_work() {
    let source = format!(
        "<template>{}</template><div id=visible style='color:red'>Visible</div>",
        format!("<div style='{}'>inert</div>", "padding:1px;".repeat(4000)).repeat(5)
    );
    let document = html::parse(&source).unwrap();
    let computed =
        style::compute_with_status(&document, &css::Stylesheet::default(), Viewport::default());
    assert!(!computed.truncated);
    assert_eq!(
        computed.styles[node(&document, "visible")].color,
        style::Color(255, 0, 0, 255)
    );
}
#[test]
fn nested_intrinsic_measurement_does_not_duplicate_final_paint_or_glyph_runs() {
    let mut source = String::new();
    for i in 0..45 {
        source.push_str(if i % 2 == 0 {
            "<div style='display:flex;min-width:0'>"
        } else {
            "<div style='display:grid;grid-template-columns:minmax(0,1fr)'>"
        });
    }
    source.push_str("<span id=leaf style='padding:4px;background:red'>Terminal</span>");
    source.push_str(&"</div>".repeat(45));
    let (document, scene) = render(&source, 240.0);
    assert!(!scene.truncated);
    assert_eq!(scene.runs.len(), 1);
    assert_eq!(
        scene
            .boxes
            .iter()
            .filter(|b| b.node == Some(node(&document, "leaf")))
            .count(),
        1
    );
    let red = scene
        .primitives
        .iter()
        .filter(|p| {
            matches!(
                p,
                layout::Primitive::Box {
                    background: Some(style::Color(255, 0, 0, 255)),
                    ..
                }
            )
        })
        .count();
    assert_eq!(red, 1);
}
#[test]
fn a_used_flex_minimum_height_does_not_make_percentages_definite() {
    let (d, s) = render(
        "<div id=auto style='display:flex;flex-direction:column;min-height:100px'><div id=a style='flex:0 1 50%;min-height:0'><div style='height:20px'></div></div><div id=b style='flex:0 1 50%;min-height:0'><div style='height:20px'></div></div></div><div id=fixed style='display:flex;flex-direction:column;height:100px'><div id=c style='flex:0 1 50%;min-height:0'><div style='height:20px'></div></div><div id=e style='flex:0 1 50%;min-height:0'><div style='height:20px'></div></div></div>",
        300.0,
    );
    close(geometry(&d, &s, "auto").height, 100.0);
    close(geometry(&d, &s, "a").height, 20.0);
    close(geometry(&d, &s, "b").y, 20.0);
    close(geometry(&d, &s, "c").height, 50.0);
    close(geometry(&d, &s, "e").y, 150.0);
    assert!(!s.truncated);
    let (d, s) = render(
        "<div style='display:flex;align-items:flex-start;min-height:100px'><div id=a style='height:50%'><div style='height:20px'></div></div></div><div style='display:flex;align-items:flex-start;height:100px'><div id=b style='height:50%'><div style='height:20px'></div></div></div>",
        300.0,
    );
    close(geometry(&d, &s, "a").height, 20.0);
    close(geometry(&d, &s, "b").height, 50.0);
    assert!(!s.truncated);
}
#[test]
fn balanced_calculations_work_in_position_and_border_shorthands() {
    let (d, s) = render(
        "<div style='position:relative;width:200px;height:100px'><div id=box style='position:absolute;inset:calc(10px + 2px) auto auto calc(20px - 3px);width:50px;height:20px;border:calc(1px + 1px) solid rgba(0, 0, 0, 0.5);border:3px wrong;'></div></div>",
        300.0,
    );
    let b = geometry(&d, &s, "box");
    close(b.x, 17.0);
    close(b.y, 12.0);
    close(b.width, 54.0);
    close(b.height, 24.0);
    assert!(!s.truncated);
}

#[test]
fn flex_item_percentage_heights_follow_post_flex_definiteness() {
    // Chrome-pinned matrix: explicit bases and cross stretching are definite;
    // natural auto bases and unstretched auto cross sizes retain auto percentages.
    for (direction, alignment, basis, container, expected_parent, expected_item, expected_child) in [
        ("row", "stretch", "auto", "", 20.0, 20.0, 10.0),
        (
            "row",
            "stretch",
            "auto",
            "min-height:100px",
            100.0,
            100.0,
            50.0,
        ),
        ("row", "flex-start", "auto", "", 20.0, 20.0, 20.0),
        (
            "row",
            "flex-start",
            "auto",
            "min-height:100px",
            100.0,
            20.0,
            20.0,
        ),
        (
            "row",
            "flex-start",
            "auto",
            "height:100px",
            100.0,
            20.0,
            20.0,
        ),
        ("column", "stretch", "20px", "", 40.0, 20.0, 10.0),
        (
            "column",
            "stretch",
            "20px",
            "min-height:100px",
            100.0,
            50.0,
            25.0,
        ),
        ("column", "stretch", "auto", "", 40.0, 20.0, 20.0),
        (
            "column",
            "stretch",
            "auto",
            "min-height:100px",
            100.0,
            50.0,
            20.0,
        ),
        (
            "column",
            "stretch",
            "auto",
            "height:100px",
            100.0,
            50.0,
            25.0,
        ),
    ] {
        let item = format!(
            "<div id=item style='flex:1 1 {basis};min-height:0;min-width:0'><div id=child style='height:50%'><div style='height:20px'></div></div></div>"
        );
        let source = format!(
            "<div id=parent style='display:flex;flex-direction:{direction};align-items:{alignment};{container}'>{item}<div style='flex:1 1 {basis};min-height:0;min-width:0'><div style='height:50%'><div style='height:20px'></div></div></div></div>"
        );
        let (d, s) = render(&source, 200.0);
        close(geometry(&d, &s, "parent").height, expected_parent);
        close(geometry(&d, &s, "item").height, expected_item);
        close(geometry(&d, &s, "child").height, expected_child);
        assert!(!s.truncated, "{direction} {alignment} {basis} {container}");
    }
}

#[test]
fn flex_column_collection_cap_respects_minimum_over_maximum() {
    for wrap in ["nowrap", "wrap"] {
        let (d, s) = render(
            &format!(
                "<div id=p style='display:flex;flex-direction:column;flex-wrap:{wrap};width:200px;min-height:200px;max-height:100px'><div id=a style='flex:0 0 40px;min-height:0'></div><div id=b style='flex:0 0 40px;min-height:0'></div><div id=c style='flex:0 0 40px;min-height:0'></div><div id=e style='flex:0 0 40px;min-height:0'></div></div>"
            ),
            200.0,
        );
        close(geometry(&d, &s, "p").height, 200.0);
        for (id, y) in [("a", 0.0), ("b", 40.0), ("c", 80.0), ("e", 120.0)] {
            close(geometry(&d, &s, id).x, 0.0);
            close(geometry(&d, &s, id).y, y);
        }
        assert!(!s.truncated);
    }
}

#[test]
fn child_measurement_cache_has_an_exact_bound_and_reports_exhaustion() {
    let group = format!(
        "<div style='display:flex;align-items:flex-start'>{}</div>",
        "<div style='flex:0 0 20px;min-width:0'>x</div>".repeat(4096)
    );
    let (_, exact) = render(&group.repeat(4), 300.0);
    assert!(!exact.truncated);
    let (_, over) = render(
        &format!(
            "{}<div style='display:flex'><div style='min-width:0'>Later</div></div>",
            group.repeat(4)
        ),
        300.0,
    );
    assert!(over.truncated);
    assert!(over.boxes.iter().all(|b| b.x.is_finite()
        && b.y.is_finite()
        && b.width.is_finite()
        && b.height.is_finite()));
}
