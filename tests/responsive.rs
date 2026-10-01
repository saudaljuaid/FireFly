use std::fs;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;

use phos::dom::{Document, NodeId};
use phos::layout::{BoxGeometry, BoxKind, Scene};
use phos::style::{Color, ComputedStyle, Length};
use phos::{Viewport, css, html, layout, style};

fn computed(source: &str, viewport: Viewport) -> (Document, Vec<ComputedStyle>, Scene) {
    let document = html::parse(source).unwrap();
    let sheet = css::parse(&document.stylesheets());
    let styles = style::compute_with_viewport(&document, &sheet, viewport);
    let scene = layout::layout_with_images_and_viewport(&document, &styles, &[], viewport);
    (document, styles, scene)
}

fn id(document: &Document, name: &str) -> NodeId {
    document
        .nodes
        .iter()
        .enumerate()
        .find_map(|(id, _)| {
            document
                .element(id)
                .is_some_and(|element| element.attribute("id") == Some(name))
                .then_some(id)
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

fn viewport(width: f32) -> Viewport {
    Viewport {
        width,
        height: None,
    }
}

fn temporary(name: &str) -> PathBuf {
    static SERIAL: AtomicUsize = AtomicUsize::new(0);
    std::env::temp_dir().join(format!(
        "phos-responsive-{}-{}-{name}",
        std::process::id(),
        SERIAL.fetch_add(1, Ordering::Relaxed)
    ))
}

#[test]
fn breakpoint_rules_change_exact_geometry_and_use_viewport_width() {
    let source = "<style>html,body{margin:0}#outer{width:200px}#item{width:100px;height:20px}@media(min-width:600px){#item{width:180px;margin-left:10px}}@media(600px < width){#item{width:190px}}</style><div id=outer><div id=item></div></div>";
    for (width, expected_width, expected_x) in [
        (599.0, 100.0, 0.0),
        (600.0, 180.0, 10.0),
        (601.0, 190.0, 10.0),
    ] {
        let (document, _, scene) = computed(source, viewport(width));
        let item = geometry(&scene, id(&document, "item"));
        assert_eq!(
            (item.x, item.width, item.height),
            (expected_x, expected_width, 20.0)
        );
        assert_eq!(geometry(&scene, id(&document, "outer")).width, 200.0);
    }
}

#[test]
fn conditional_cascade_preserves_importance_specificity_attributes_and_source_order() {
    let source = "<style>html,body{margin:0}.item{width:10px;color:red}#item{width:20px}@media(min-width:600px){.item{width:30px;color:blue!important}#item{width:40px}}#item{width:50px}@media(min-width:600px){#item{width:60px!important;color:green!important}}</style><div id=item class=item style='width:70px;color:navy'></div>";
    let (document, narrow, _) = computed(source, viewport(599.0));
    let item = id(&document, "item");
    assert_eq!(narrow[item].width, Some(Length::Px(70.0)));
    assert_eq!(narrow[item].color, Color(0, 0, 128, 255));
    let (_, wide, _) = computed(source, viewport(600.0));
    assert_eq!(wide[item].width, Some(Length::Px(60.0)));
    assert_eq!(wide[item].color, Color(0, 128, 0, 255));
}

#[test]
fn duplicate_selector_lists_use_the_highest_matching_specificity_once() {
    let source = "<style>#item,.item,.item{width:80px}.item{width:90px}</style><div id=item class=item></div>";
    let (document, styles, _) = computed(source, viewport(800.0));
    assert_eq!(styles[id(&document, "item")].width, Some(Length::Px(80.0)));
}

#[test]
fn media_font_units_are_initial_and_nested_conditions_are_conjoined() {
    let source = "<style>html{font-size:40px}body{margin:0}#item{width:100px}@media(min-width:40em){#item{width:200px}@media(max-width:50rem){#item{width:300px}}}</style><div id=item></div>";
    for (width, expected) in [
        (639.0, 100.0),
        (640.0, 300.0),
        (800.0, 300.0),
        (801.0, 200.0),
    ] {
        let (document, styles, _) = computed(source, viewport(width));
        assert_eq!(
            styles[id(&document, "item")].width,
            Some(Length::Px(expected))
        );
    }
}

#[test]
fn calc_and_viewport_lengths_do_not_confuse_containing_width_with_screen_width() {
    let source = "<style>html,body{margin:0}#outer{width:200px}#viewport{width:calc(50vw - 10px);height:10px}#percent{box-sizing:border-box;width:calc(50% - 10px);height:10px}#edges{font-size:20px;width:100px;height:10px;padding:calc(1em + 2px) calc(2px * 3);margin:0 calc(2px + 3px)}</style><div id=outer><div id=viewport></div><div id=percent></div><div id=edges></div></div>";
    let (document, _, scene) = computed(source, viewport(800.0));
    assert_eq!(geometry(&scene, id(&document, "viewport")).width, 390.0);
    assert_eq!(geometry(&scene, id(&document, "percent")).width, 90.0);
    let edges = geometry(&scene, id(&document, "edges"));
    assert_eq!((edges.x, edges.width, edges.height), (5.0, 112.0, 54.0));
}

#[test]
fn calculation_percentages_wait_for_definite_height_even_when_the_coefficient_is_zero() {
    let source = "<style>html,body{margin:0}#definite{height:200px}#a,#b{height:calc(50% - 10px)}#zero{height:calc(100px + 0%);width:10px}</style><div id=definite><div id=a></div></div><div><div id=b>Text</div><div id=zero>More</div></div>";
    let (document, styles, scene) = computed(source, viewport(800.0));
    assert_eq!(geometry(&scene, id(&document, "a")).height, 90.0);
    assert!((geometry(&scene, id(&document, "b")).height - 19.2).abs() < 0.001);
    assert_eq!(
        styles[id(&document, "zero")]
            .height
            .unwrap()
            .resolve_indefinite(None),
        None
    );
    assert!(geometry(&scene, id(&document, "zero")).height < 100.0);
}

#[test]
fn explicit_height_units_and_negative_calc_clamping_have_exact_sizes() {
    let source = "<style>html,body{margin:0}#item{width:10vmin;height:20vh;padding:calc(2px - 8px);margin-left:calc(2px - 8px)}#maximum{width:10vmax;height:calc(10px - 30px)}</style><div id=item></div><div id=maximum></div>";
    let (document, _, scene) = computed(
        source,
        Viewport {
            width: 800.0,
            height: Some(600.0),
        },
    );
    let item = geometry(&scene, id(&document, "item"));
    assert_eq!((item.x, item.width, item.height), (-6.0, 60.0, 120.0));
    assert_eq!(
        (
            geometry(&scene, id(&document, "maximum")).width,
            geometry(&scene, id(&document, "maximum")).height
        ),
        (80.0, 0.0)
    );
}

#[test]
fn invalid_conditions_and_calculations_leave_later_valid_rules_and_declarations_usable() {
    let source = "<style>html,body{margin:0}#item{width:50px;width:calc(2px * 3px);height:20px}@media(width == 800px){#item{width:900px}}@media(bogus:1), (min-width:700px){#item{width:calc(50% - 10px);height:30px}}#item{padding:calc(2px + broken);padding:3px}</style><div id=item></div>";
    let (document, _, scene) = computed(source, viewport(800.0));
    assert_eq!(
        (
            geometry(&scene, id(&document, "item")).width,
            geometry(&scene, id(&document, "item")).height
        ),
        (396.0, 36.0)
    );
}

#[test]
fn width_only_public_apis_remain_equal_and_height_environment_is_validated() {
    let source =
        "<style>body{margin:0}div{width:25vw;height:50vh;background:red}</style><div></div>";
    assert_eq!(
        phos::render(source, 800.0).unwrap(),
        phos::render_with_viewport(source, viewport(800.0)).unwrap()
    );
    assert_eq!(
        phos::render_bytes(source.as_bytes(), None, 800.0).unwrap(),
        phos::render_bytes_with_viewport(source.as_bytes(), None, viewport(800.0)).unwrap()
    );
    for height in [0.0, -1.0, 16_385.0, f32::NAN, f32::INFINITY] {
        let error = phos::render_with_viewport(
            source,
            Viewport {
                width: 800.0,
                height: Some(height),
            },
        )
        .unwrap_err();
        assert!(error.to_string().contains("viewport height"));
    }
    let explicit = phos::render_with_viewport(
        source,
        Viewport {
            width: 800.0,
            height: Some(600.0),
        },
    )
    .unwrap();
    assert!(explicit.contains("height=\"300\""));
}

#[test]
fn style_and_local_link_media_attributes_select_the_same_viewport_environment() {
    let directory = temporary("media");
    fs::create_dir(&directory).unwrap();
    let page = directory.join("page.html");
    fs::write(directory.join("wide.css"), "div{background:#123456}").unwrap();
    fs::write(&page, "<style>body{margin:0}div{width:10px;height:10px;background:red}</style><style media='(max-width:599px)'>div{background:blue}</style><link rel=stylesheet media='screen and (min-width:600px)' href=wide.css><link rel=stylesheet media=print href=missing.css><template><link rel=stylesheet href=inert.css></template><div></div>").unwrap();
    let (narrow, warnings) = phos::render_file(&page, 599.0).unwrap();
    assert!(warnings.is_empty());
    assert!(narrow.contains("#0000ff"));
    let (wide, warnings) = phos::render_file(&page, 600.0).unwrap();
    assert!(warnings.is_empty());
    assert!(wide.contains("#123456"));
    fs::remove_file(page).unwrap();
    fs::remove_file(directory.join("wide.css")).unwrap();
    fs::remove_dir(directory).unwrap();
}

#[test]
fn conditional_http_stylesheets_skip_inactive_and_inert_fetches() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let server = thread::spawn(move || {
        let mut paths = Vec::new();
        for _ in 0..2 {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = Vec::new();
            while !request.windows(4).any(|bytes| bytes == b"\r\n\r\n") {
                let mut buffer = [0; 1024];
                let read = stream.read(&mut buffer).unwrap();
                assert!(read > 0 && request.len() + read <= 8192);
                request.extend_from_slice(&buffer[..read]);
            }
            let request = String::from_utf8_lossy(&request);
            let path = request.split_ascii_whitespace().nth(1).unwrap().to_string();
            let (mime, body) = if path == "/page" {
                (
                    "text/html; charset=utf-8",
                    "<style>div{width:10px;height:10px}</style><link rel=stylesheet media='(max-width:600px)' href=/narrow.css><link rel=stylesheet media='(min-width:700px)' href=/wide.css><template><link rel=stylesheet href=/inert.css></template><div></div>",
                )
            } else {
                assert_eq!(path, "/narrow.css");
                ("text/css", "div{background:#123456}")
            };
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: {mime}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            stream.write_all(response.as_bytes()).unwrap();
            paths.push(path);
        }
        paths
    });
    let page = phos::render_url(&format!("http://127.0.0.1:{port}/page"), 500.0).unwrap();
    assert_eq!(server.join().unwrap(), ["/page", "/narrow.css"]);
    assert_eq!(page.stylesheets, 1);
    assert_eq!(page.url.path_and_query, "/page");
    assert!(page.svg.contains("#123456"));
    assert!(page.warnings.is_empty());
}

#[test]
fn stylesheet_and_inline_declaration_caps_report_output_truncation() {
    let rules = "div{color:red}".repeat(css::MAX_RULES + 1);
    let source = format!("<style>{rules}</style><div>Still present</div>");
    assert!(
        phos::render(&source, 800.0)
            .unwrap()
            .starts_with("<svg data-phos-truncated=\"true\"")
    );
    let declarations = "color:red;".repeat(8193);
    let source = format!("<div style='{declarations}'>Still present</div>");
    assert!(
        phos::render(&source, 800.0)
            .unwrap()
            .starts_with("<svg data-phos-truncated=\"true\"")
    );
}

#[test]
fn scarlite_cli_height_matches_library_output_and_rejects_invalid_environment() {
    let page = temporary("page.html");
    let svg = temporary("page.svg");
    let source = "<style>body{margin:0}div{width:25vw;height:50vh;background:red}@media(min-width:700px){div{width:30vw}}</style><div></div>";
    fs::write(&page, source).unwrap();
    let result = Command::new(env!("CARGO_BIN_EXE_scarlite"))
        .arg(&page)
        .args(["--width", "800", "--height", "600", "--output"])
        .arg(&svg)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let expected = phos::render_file_with_viewport(
        &page,
        Viewport {
            width: 800.0,
            height: Some(600.0),
        },
    )
    .unwrap()
    .0;
    assert_eq!(fs::read_to_string(&svg).unwrap(), expected);
    let result = Command::new(env!("CARGO_BIN_EXE_scarlite"))
        .arg(&page)
        .args(["--height", "NaN", "--output"])
        .arg(&svg)
        .output()
        .unwrap();
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("viewport height"));
    fs::remove_file(page).unwrap();
    fs::remove_file(svg).unwrap();
}
