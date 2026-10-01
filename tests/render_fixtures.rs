use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::Path;
use std::thread;

use phos::dom::{Document, NodeId, NodeKind};
use phos::layout::{BoxKind, ImageSource, Primitive, Scene};
use phos::{css, html, layout, render_file, render_url, resource, style};

fn scene(source: &str, width: f32) -> (Document, Scene) {
    let document = html::parse(source).unwrap();
    let styles = style::compute(&document, &css::parse(&document.stylesheets()));
    let scene = layout::layout(&document, &styles, width);
    (document, scene)
}

fn element(document: &Document, class: &str) -> NodeId {
    document
        .nodes
        .iter()
        .position(|node| {
            matches!(&node.kind,
        NodeKind::Element(element) if element.has_class(class))
        })
        .unwrap()
}

fn boxes(scene: &Scene, id: NodeId) -> Vec<(f32, f32, f32, f32)> {
    scene
        .boxes
        .iter()
        .filter(|item| item.node == Some(id))
        .map(|item| (item.x, item.y, item.width, item.height))
        .collect()
}

#[test]
fn inline_block_cards_wrap_at_three_viewports() {
    let source = include_str!("render/cards.html");
    for (viewport, expected_y) in [(320.0, 75.0), (640.0, 5.0), (900.0, 5.0)] {
        let (document, scene) = scene(source, viewport);
        let cards: Vec<_> = document
            .nodes
            .iter()
            .enumerate()
            .filter_map(|(id, node)| {
                matches!(&node.kind, NodeKind::Element(element) if element.has_class("card"))
                    .then_some(id)
            })
            .collect();
        assert_eq!(cards.len(), 3);
        assert_eq!(boxes(&scene, cards[0]), vec![(5.0, 5.0, 100.0, 60.0)]);
        assert_eq!(boxes(&scene, cards[1]), vec![(115.0, 5.0, 100.0, 60.0)]);
        assert_eq!(
            boxes(&scene, cards[2]),
            vec![(
                if viewport == 320.0 { 5.0 } else { 225.0 },
                expected_y,
                100.0,
                60.0
            )]
        );
        assert_eq!(
            scene
                .primitives
                .iter()
                .filter(|item| matches!(item, Primitive::Box { .. }))
                .count(),
            3
        );
        assert!(scene.height >= expected_y + 65.0);
    }
}

#[test]
fn nested_padding_border_box_and_auto_margins() {
    let (document, scene) = scene(include_str!("render/nested.html"), 400.0);
    let outer = element(&document, "outer");
    let inner = element(&document, "inner");
    assert_eq!(boxes(&scene, outer)[0].0, 88.0);
    assert_eq!(boxes(&scene, outer)[0].2, 224.0);
    assert_eq!(boxes(&scene, inner)[0].0, 100.0);
    assert_eq!(boxes(&scene, inner)[0].2, 112.0);
}

#[test]
fn computed_units_alpha_inheritance_and_definite_height() {
    let source = "<style>html,body{margin:0;padding:0} .parent{width:200px;height:100px;font-size:20px;line-height:1.5;color:rgba(10,20,30,0.5)} .child{width:50%;height:50%;box-sizing:border-box;padding:10%;margin:0 auto;border:1px solid}</style><div class=parent><div class=child>Text</div></div>";
    let document = html::parse(source).unwrap();
    let styles = style::compute(&document, &css::parse(&document.stylesheets()));
    let parent = element(&document, "parent");
    let child = element(&document, "child");
    assert_eq!(styles[parent].color, style::Color(10, 20, 30, 128));
    assert_eq!(styles[child].color, styles[parent].color);
    assert_eq!(styles[child].border_color, styles[parent].color);
    assert_eq!(styles[child].line_height, 30.0);
    let scene = layout::layout(&document, &styles, 400.0);
    assert_eq!(boxes(&scene, child)[0], (50.0, 0.0, 100.0, 50.0));
}

#[test]
fn min_max_constraints_and_invalid_values_are_predictable() {
    let source = "<style>html,body{margin:0} .box{width:300px;width:calc(10px + red);max-width:150px;min-width:100px;height:10px;min-height:20px;max-height:30px;margin:0 auto;background:blue}</style><div class=box></div>";
    let (document, scene) = scene(source, 400.0);
    let box_id = element(&document, "box");
    assert_eq!(boxes(&scene, box_id)[0], (125.0, 0.0, 150.0, 20.0));
}

#[test]
fn mixed_flow_inline_fragments_and_order() {
    let (document, scene) = scene(include_str!("render/mixed.html"), 115.0);
    let ink = element(&document, "ink");
    assert!(boxes(&scene, ink).len() >= 2);
    assert!(
        scene
            .boxes
            .iter()
            .filter(|geometry| geometry.kind == BoxKind::AnonymousBlock)
            .count()
            >= 2
    );
    let text: String = scene
        .primitives
        .iter()
        .filter_map(|primitive| match primitive {
            Primitive::Text { content, .. } => Some(content.as_str()),
            _ => None,
        })
        .collect();
    assert!(text.find("Before").unwrap() < text.find("Middle").unwrap());
    assert!(text.find("Middle").unwrap() < text.find("After").unwrap());
}

#[test]
fn long_text_metrics_and_whitespace_are_finite() {
    let (_, scene) = scene(include_str!("render/text.html"), 180.0);
    assert!(scene.height > 100.0);
    assert!(scene.height.is_finite());
    assert!(scene.primitives.iter().any(
        |primitive| matches!(primitive, Primitive::Text { content, .. } if content.contains("Café"))
    ));
    assert!(scene.primitives.iter().any(
        |primitive| matches!(primitive, Primitive::Text { content, .. } if content.contains("  "))
    ));
    for primitive in &scene.primitives {
        if let Primitive::Text {
            x, baseline, width, ..
        } = primitive
        {
            assert!(x.is_finite() && baseline.is_finite() && width.is_finite());
        }
    }
}

#[test]
fn centered_text_uses_measured_advance_and_br_forces_next_line() {
    let source = "<style>html,body{margin:0}div{width:200px;text-align:center;line-height:30px}</style><div>Hi<br>Bye</div>";
    let (_, scene) = scene(source, 400.0);
    let text: Vec<_> = scene
        .primitives
        .iter()
        .filter_map(|primitive| match primitive {
            Primitive::Text {
                x,
                baseline,
                content,
                width,
                ..
            } => Some((*x, *baseline, content.as_str(), *width)),
            _ => None,
        })
        .collect();
    assert_eq!(text.len(), 2);
    assert_eq!(text[0].2, "Hi");
    assert_eq!(text[1].2, "Bye");
    assert!((text[0].0 - (200.0 - text[0].3) / 2.0).abs() < 0.01);
    assert!((text[1].0 - (200.0 - text[1].3) / 2.0).abs() < 0.01);
    assert_eq!(text[1].1 - text[0].1, 30.0);
}

#[test]
fn images_keep_intrinsic_ratio_and_alt_fallback() {
    let source = include_str!("render/images.html");
    let document = html::parse(source).unwrap();
    let styles = style::compute(&document, &css::parse(&document.stylesheets()));
    let decoded =
        resource::decode_image(include_bytes!("render/two-pixels.png"), Some("image/png")).unwrap();
    assert_eq!((decoded.width, decoded.height), (2.0, 1.0));
    let ids: Vec<_> = document
        .nodes
        .iter()
        .enumerate()
        .filter_map(|(id, node)| {
            matches!(&node.kind, NodeKind::Element(element) if element.tag == "img").then_some(id)
        })
        .collect();
    let mut images: Vec<Option<ImageSource>> = vec![None; document.nodes.len()];
    images[ids[0]] = Some(decoded.clone());
    images[ids[1]] = Some(decoded.clone());
    images[ids[2]] = Some(decoded);
    let scene = layout::layout_with_images(&document, &styles, &images, 400.0);
    assert_eq!(boxes(&scene, ids[0])[0].2, 80.0);
    assert_eq!(boxes(&scene, ids[0])[0].3, 40.0);
    assert_eq!(boxes(&scene, ids[1])[0].2, 60.0);
    assert_eq!(boxes(&scene, ids[1])[0].3, 30.0);
    assert_eq!(boxes(&scene, ids[2])[0].2, 2.0);
    assert_eq!(boxes(&scene, ids[2])[0].3, 1.0);
    assert!(scene.primitives.iter().any(
        |primitive| matches!(primitive, Primitive::Text { content, .. } if content == "missing")
    ));
    let (svg, warnings) = render_file(Path::new("tests/render/images.html"), 400.0).unwrap();
    assert_eq!(warnings.len(), 1);
    assert_eq!(svg.matches("<image ").count(), 3);
    assert!(svg.contains("data:image/png;base64,"));
    let xml = roxmltree::Document::parse(&svg).unwrap();
    let painted: Vec<_> = xml
        .descendants()
        .filter(|node| node.has_tag_name("image"))
        .map(|node| {
            (
                node.attribute("width").unwrap().to_string(),
                node.attribute("height").unwrap().to_string(),
            )
        })
        .collect();
    assert_eq!(
        painted,
        [
            ("80.00".into(), "40.00".into()),
            ("60.00".into(), "30.00".into()),
            ("2.00".into(), "1.00".into())
        ]
    );
}

#[test]
fn local_linked_stylesheet_loads_and_template_resources_stay_inert() {
    let (svg, warnings) = render_file(Path::new("tests/render/local-link.html"), 320.0).unwrap();
    assert!(warnings.is_empty());
    assert!(svg.contains("#123456"));
    assert!(!svg.contains("never.png"));
}

#[test]
fn malformed_css_keeps_later_rule_and_svg_is_well_formed() {
    let source = include_str!("render/malformed.html");
    let document = html::parse(source).unwrap();
    let sheet = css::parse(&document.stylesheets());
    let styles = style::compute(&document, &sheet);
    let good = element(&document, "good");
    assert_eq!(styles[good].color, style::Color(0x12, 0x34, 0x56, 255));
    let svg = phos::render(source, 320.0).unwrap();
    assert!(svg.contains("#123456"));
    let xml = roxmltree::Document::parse(&svg).unwrap();
    assert_eq!(xml.root_element().tag_name().name(), "svg");
    assert!(
        xml.descendants()
            .any(|node| node.has_tag_name("title") && node.text() == Some("Later"))
    );
    assert!(xml.descendants().any(|node| node.has_tag_name("use")));
}

#[test]
fn rounded_overflow_clips_descendants_inside_padding_edge() {
    let svg = phos::render(include_str!("render/clip.html"), 200.0).unwrap();
    let xml = roxmltree::Document::parse(&svg).unwrap();
    let clip = xml
        .descendants()
        .find(|node| node.has_tag_name("clipPath"))
        .unwrap();
    assert!(clip.descendants().any(|node| node.has_tag_name("path")));
    let group = xml
        .descendants()
        .find(|node| node.has_tag_name("g") && node.attribute("clip-path").is_some())
        .unwrap();
    assert!(
        group
            .descendants()
            .any(|node| node.has_tag_name("title") && node.text() == Some("Visible"))
    );
    assert!(group.descendants().any(|node| {
        node.has_tag_name("g")
            && node.attribute("data-phos-text") == Some("true")
            && node
                .attribute("data-advance")
                .unwrap()
                .parse::<f32>()
                .unwrap()
                > 0.0
    }));
    assert!(svg.contains("fill-opacity=\"0.502\""));
}

#[test]
fn background_paint_order_precedes_descendants_and_text() {
    let (_, scene) = scene(
        "<style>html,body{margin:0}.outer{background:red;padding:8px}.inner{background:blue}</style><div class=outer><div class=inner>Ink</div></div>",
        200.0,
    );
    let red = scene
        .primitives
        .iter()
        .position(|item| {
            matches!(
                item,
                Primitive::Box {
                    background: Some(style::Color(255, 0, 0, 255)),
                    ..
                }
            )
        })
        .unwrap();
    let blue = scene
        .primitives
        .iter()
        .position(|item| {
            matches!(
                item,
                Primitive::Box {
                    background: Some(style::Color(0, 0, 255, 255)),
                    ..
                }
            )
        })
        .unwrap();
    let ink = scene
        .primitives
        .iter()
        .position(|item| matches!(item, Primitive::Text { content, .. } if content == "Ink"))
        .unwrap();
    assert!(red < blue && blue < ink);
}

#[test]
fn start_page_canary_has_rows_and_rounded_tiles() {
    let source = include_str!("render/start_page.html");
    for viewport in [320.0, 640.0, 900.0] {
        let (document, scene) = scene(source, viewport);
        let ids: Vec<_> = document
            .nodes
            .iter()
            .enumerate()
            .filter_map(|(id, node)| {
                matches!(&node.kind, NodeKind::Element(element) if element.has_class("tile"))
                    .then_some(id)
            })
            .collect();
        assert_eq!(ids.len(), 5);
        let positions: Vec<_> = ids.iter().map(|&id| boxes(&scene, id)[0]).collect();
        assert!(positions[0].0 < positions[1].0);
        assert!(positions[1].1 == positions[0].1);
        assert!(positions[4].1 > positions[0].1);
        assert!(scene.primitives.iter().filter(|primitive| matches!(primitive, Primitive::Box { radius, .. } if radius[0] == 8.0)).count() >= 5);
        assert!(scene.height.is_finite());
    }
}

#[test]
fn repeated_adversarial_pages_stay_bounded() {
    for _ in 0..20 {
        let source = format!(
            "<style>div{{width:16384px;padding:100%;border:3px solid red}}</style><div>{}</div>",
            "unbreakable".repeat(400)
        );
        let (_, scene) = scene(&source, 60.0);
        assert!(scene.height.is_finite() && scene.height <= 1_000_000.0);
        assert!(scene.primitives.len() <= 200_000);
    }
}

#[test]
fn empty_and_deep_documents_keep_finite_geometry() {
    let (_, empty) = scene("<div></div>", 1.0);
    assert!(empty.height.is_finite() && empty.height >= 1.0);
    let source = format!(
        "{}x{}",
        "<div style='padding:1px'>".repeat(240),
        "</div>".repeat(240)
    );
    let (_, deep) = scene(&source, 16_384.0);
    assert!(deep.height.is_finite() && deep.height <= 1_000_000.0);
    assert!(deep.boxes.iter().all(|geometry| geometry.x.is_finite()
        && geometry.y.is_finite()
        && geometry.width.is_finite()
        && geometry.height.is_finite()));
}

#[test]
fn redirect_metadata_controls_css_and_image_loading() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let server = thread::spawn(move || {
        for _ in 0..5 {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = Vec::new();
            let mut byte = [0];
            while !request.ends_with(b"\r\n\r\n") {
                stream.read_exact(&mut byte).unwrap();
                request.push(byte[0]);
            }
            let path = request.split(|byte| *byte == b' ').nth(1).unwrap();
            let (headers, body): (&[u8], &[u8]) = match path {
                b"/" => (b"HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nConnection: close\r\n\r\n",
                    b"<link rel=stylesheet href=/style-start><div class=card>Card</div><img src=/image-start alt=bad><template><img src=/inert></template>"),
                b"/style-start" => (b"HTTP/1.1 302 Found\r\nLocation: /style-final\r\nContent-Type: text/html\r\nContent-Length: 0\r\n\r\n", b""),
                b"/style-final" => (b"HTTP/1.1 200 OK\r\nContent-Type: text/css\r\nConnection: close\r\n\r\n", b".card{border:2px solid #123456}"),
                b"/image-start" => (b"HTTP/1.1 302 Found\r\nLocation: /image-final\r\nContent-Type: text/plain\r\nContent-Length: 0\r\n\r\n", b""),
                b"/image-final" => (b"HTTP/1.1 200 OK\r\nContent-Type: image/png\r\nConnection: close\r\n\r\n", include_bytes!("render/two-pixels.png")),
                _ => panic!("unexpected request: {}", String::from_utf8_lossy(path)),
            };
            stream.write_all(headers).unwrap();
            stream.write_all(body).unwrap();
        }
    });
    let page = render_url(&format!("http://127.0.0.1:{port}/"), 400.0).unwrap();
    server.join().unwrap();
    assert_eq!(page.stylesheets, 1);
    assert!(page.warnings.is_empty());
    assert!(page.svg.contains("#123456"));
    let xml = roxmltree::Document::parse(&page.svg).unwrap();
    assert_eq!(
        xml.descendants()
            .filter(|node| node.has_tag_name("image"))
            .count(),
        1
    );
}
