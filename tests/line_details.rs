use phos::layout::Primitive;
use phos::{css, dom::NodeKind, html, layout, style, text};

fn scene(source: &str, width: f32) -> (phos::dom::Document, layout::Scene) {
    let document = html::parse(source).unwrap();
    let styles = style::compute(&document, &css::parse(&document.stylesheets()));
    let scene = layout::layout(&document, &styles, width);
    (document, scene)
}

#[test]
fn tabs_advance_to_exact_eight_space_stops() {
    let (_, scene) = scene(
        "<style>html,body,p{margin:0}p{white-space:pre;font-size:16px}</style><p>a\tb\tc</p>",
        320.0,
    );
    let stop = text::width(" ", 16.0, false) * 8.0;
    for (word, expected) in [("b", stop), ("c", stop * 2.0)] {
        let run = scene
            .runs
            .iter()
            .find(|run| run.text.content == word)
            .unwrap();
        assert!((run.x - expected).abs() < 0.001, "{} != {expected}", run.x);
    }
    assert_eq!(scene.line_boxes.len(), 1);
}

#[test]
fn wrapped_rtl_preserved_spaces_reset_to_the_paragraph_level() {
    let (_, scene) = scene(
        "<style>html,body,p{margin:0}p{direction:rtl;white-space:pre-wrap;font-size:16px}</style><p>one    two</p>",
        60.0,
    );
    assert_eq!(scene.line_boxes.len(), 2);
    let first = &scene.line_boxes[0];
    let runs = &scene.runs[first.runs.clone()];
    assert_eq!(runs[0].text.content, "    ");
    assert_eq!(runs[1].text.content, "one");
    assert!(runs[0].x < runs[1].x);
    assert!((runs[1].x + runs[1].text.advance - 60.0).abs() < 0.001);
}

#[test]
fn wrapped_inline_borders_are_sliced_at_line_boundaries() {
    let (_, scene) = scene(
        "<style>html,body,p{margin:0}p{line-height:28px}a{border:2px solid red;padding:3px;background:#eeeeee}</style><p><a>alpha beta gamma delta epsilon</a></p>",
        75.0,
    );
    let borders: Vec<_> = scene
        .primitives
        .iter()
        .filter_map(|primitive| match primitive {
            Primitive::Box {
                border_color: style::Color(255, 0, 0, 255),
                border_width,
                ..
            } => Some(*border_width),
            _ => None,
        })
        .collect();
    assert!(borders.len() >= 3);
    assert_eq!(borders[0][3], 2.0);
    assert_eq!(borders[0][1], 0.0);
    for border in &borders[1..borders.len() - 1] {
        assert_eq!(border[1], 0.0);
        assert_eq!(border[3], 0.0);
    }
    assert_eq!(borders.last().unwrap()[1], 2.0);
    assert_eq!(borders.last().unwrap()[3], 0.0);
}

#[test]
fn list_markers_share_the_first_line_baseline_and_respect_none() {
    let (document, scene) = scene(
        "<style>html,body{margin:0}.off{list-style-type:none}</style><ol start=3><li>alpha beta gamma</li><li>second</li></ol><ul><li>bullet</li><li class=off>hidden marker</li><li></li></ul>",
        110.0,
    );
    let markers: Vec<_> = scene.runs.iter().filter(|run| matches!(&document.nodes[run.node].kind, NodeKind::Element(element) if element.tag == "li")).collect();
    assert_eq!(
        markers
            .iter()
            .map(|run| run.text.content.as_str())
            .collect::<Vec<_>>(),
        ["3.", "4.", "•", "•"]
    );
    for marker in markers {
        assert_eq!(marker.baseline, scene.line_boxes[marker.line].baseline);
        assert!(marker.x < scene.line_boxes[marker.line].x);
        assert!(marker.source_range.is_empty());
    }
}

#[test]
fn overflow_wrap_policy_and_atomic_nowrap_are_explicit() {
    let (_, normal) = scene(
        "<style>html,body,p{margin:0}p{overflow-wrap:normal}</style><p>abcdefghijk</p>",
        1.0,
    );
    assert_eq!(normal.line_boxes.len(), 1);
    let (_, glue) = scene(
        "<style>html,body,p{margin:0}</style><p>A&nbsp;B C&#8239;D</p>",
        1.0,
    );
    assert_eq!(glue.line_boxes.len(), 2);
    assert!(glue.line_boxes.iter().all(|line| line.advance > 1.0));
    let (_, atomic) = scene(
        "<style>html,body,p{margin:0}p{white-space:nowrap}span{display:inline-block;width:40px;height:10px}</style><p>before<span></span><span></span>after</p>",
        1.0,
    );
    let outer = atomic
        .line_boxes
        .iter()
        .filter(|line| line.width == 1.0)
        .count();
    assert_eq!(outer, 1);
}

#[test]
fn unicode_break_opportunities_preserve_punctuation_and_urls() {
    for (source, first, second) in [
        ("alpha-beta", "alpha-", "beta"),
        ("hello (world)", "hello", "(world)"),
        ("https://example.com/path", "https://", "example.com/path"),
    ] {
        let width = text::width(first, 16.0, false) + 0.01;
        let (_, scene) = scene(
            &format!(
                "<style>html,body,p{{margin:0}}p{{overflow-wrap:normal}}</style><p>{source}</p>"
            ),
            width,
        );
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
        assert_eq!(lines.first().unwrap(), first);
        assert_eq!(lines[1..].concat(), second);
    }
    let (_, marks) = scene(
        "<style>html,body,p{margin:0}p{overflow-wrap:normal}</style><p>!!!!!!!!!!!!!!!!!!!!</p>",
        1.0,
    );
    assert_eq!(marks.line_boxes.len(), 1);
    assert_eq!(
        marks
            .runs
            .iter()
            .map(|run| run.text.content.as_str())
            .collect::<String>(),
        "!!!!!!!!!!!!!!!!!!!!"
    );
}

#[test]
fn atomic_only_lines_and_inline_blocks_use_the_documented_baseline() {
    let (_, mixed) = scene(
        "<style>html,body,p{margin:0}span{display:inline-block;width:70px;padding:3px;line-height:22px}</style><p>before <span>two<br>lines</span> after</p>",
        320.0,
    );
    let before = mixed
        .runs
        .iter()
        .find(|run| run.text.content == "before")
        .unwrap();
    let last = mixed
        .runs
        .iter()
        .find(|run| run.text.content == "lines")
        .unwrap();
    assert_eq!(before.baseline, last.baseline);
    let (_, empty) = scene(
        "<style>html,body,p{margin:0}span{display:inline-block;width:30px;height:40px}</style><p><span></span></p>",
        320.0,
    );
    assert_eq!(empty.line_boxes.len(), 1);
    assert_eq!(empty.line_boxes[0].height, 40.0);
    assert_eq!(empty.line_boxes[0].baseline, 40.0);
    let (_, zero) = scene(
        "<style>html,body,p{margin:0}</style><p><span></span></p>",
        1.0,
    );
    assert!(zero.runs.is_empty());
    assert!(zero.line_boxes.is_empty());
}

#[test]
fn large_nowrap_text_stops_before_coordinate_saturation() {
    let source = format!(
        "<style>html,body,p{{margin:0}}p{{font-size:1024px;white-space:nowrap}}</style><p>{}</p>",
        "W".repeat(3000)
    );
    let (_, scene) = scene(&source, 320.0);
    assert!(scene.truncated);
    for line in &scene.line_boxes {
        let runs = &scene.runs[line.runs.clone()];
        assert!(line.advance <= 1_000_000.0);
        for run in runs {
            assert!(run.x + run.text.advance <= 1_000_000.0);
            assert!(
                (run.text
                    .glyphs
                    .iter()
                    .map(|glyph| glyph.advance)
                    .sum::<f32>()
                    - run.text.advance)
                    .abs()
                    < 1.0
            );
        }
        for pair in runs.windows(2) {
            assert!(pair[0].x + pair[0].text.advance <= pair[1].x + 1.0);
        }
    }
}
