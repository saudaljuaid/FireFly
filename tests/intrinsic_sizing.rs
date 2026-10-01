use phos::{
    css, html,
    intrinsic::IntrinsicCache,
    layout::ImageSource,
    sizing::{AvailableSize, IntrinsicSizes, resolve_content_size},
    style::{self, BoxSizing, Length},
    text,
};

fn sizes(source: &str, id: &str) -> (IntrinsicSizes, IntrinsicSizes) {
    let document = html::parse(source).unwrap();
    let styles = style::compute(&document, &css::parse(&document.stylesheets()));
    let images = vec![None; document.nodes.len()];
    let node = document
        .nodes
        .iter()
        .enumerate()
        .find(|(i, _)| {
            document
                .element(*i)
                .is_some_and(|e| e.attribute("id") == Some(id))
        })
        .unwrap()
        .0;
    let measure = IntrinsicCache::new(&document, &styles, &images);
    let content = measure.content(node);
    let contribution = measure.contribution(node);
    assert_eq!(measure.content(node), content);
    assert!(!measure.truncated.get());
    (content, contribution)
}

fn close(a: f32, b: f32) {
    assert!((a - b).abs() < 0.02, "{a} != {b}");
}

#[test]
fn normalized_words_use_the_existing_shaped_advances() {
    let (s, _) = sizes("<div id=t>  alpha  <span> beta </span> gamma  </div>", "t");
    close(s.max_content, text::width("alpha beta gamma", 16.0, false));
    close(s.min_content, text::width("gamma", 16.0, false));
}

#[test]
fn forced_lines_and_blocks_take_maximum_not_subtree_sum() {
    let (s, _) = sizes(
        "<div id=t><div>alpha</div><div>longer text</div></div>",
        "t",
    );
    close(s.max_content, text::width("longer text", 16.0, false));
    let (s, _) = sizes("<div id=t style='white-space:pre'>abc\ndefgh</div>", "t");
    close(s.min_content, text::width("defgh", 16.0, false));
    close(s.max_content, s.min_content);
}

#[test]
fn nbsp_and_break_word_do_not_create_intrinsic_emergency_opportunities() {
    let (s, _) = sizes("<div id=t>alpha&nbsp;beta gamma</div>", "t");
    close(s.min_content, text::width("alpha\u{a0}beta", 16.0, false));
    let (s, _) = sizes(
        "<div id=t>https://example.test/abcdefghijklmnopqrstuv</div>",
        "t",
    );
    assert!(s.min_content > text::width("abcdefghijklmnopqrstuv", 16.0, false) - 0.02);
}

#[test]
fn anywhere_uses_shaped_graphemes_including_combining_marks() {
    let (s, _) = sizes(
        "<div id=t style='overflow-wrap:anywhere'>e&#x301;WWW</div>",
        "t",
    );
    close(s.min_content, text::width("W", 16.0, false));
    close(s.max_content, text::width("e\u{301}WWW", 16.0, false));
}

#[test]
fn nowrap_joins_soft_break_groups_into_one_minimum() {
    let (s, _) = sizes(
        "<div id=t style='white-space:nowrap'>alpha beta gamma</div>",
        "t",
    );
    close(s.min_content, s.max_content);
}

#[test]
fn inline_edges_and_atomic_content_contribute() {
    let (s, _) = sizes(
        "<div id=t><span style='padding:0 4px;border:2px solid'>alpha</span> <span style='display:inline-block;width:80px;padding:0 3px'>x</span></div>",
        "t",
    );
    close(
        s.max_content,
        text::width("alpha ", 16.0, false) + 12.0 + 86.0,
    );
    close(s.min_content, 86.0);
}

#[test]
fn anywhere_keeps_decorations_attached_to_end_graphemes() {
    let (s, _) = sizes(
        "<div id=t style='overflow-wrap:anywhere'><span style='padding:0 20px'>WW</span></div>",
        "t",
    );
    close(s.min_content, 20.0 + text::width("W", 16.0, false));
    close(s.max_content, 40.0 + text::width("WW", 16.0, false));
}

#[test]
fn border_box_preferred_constraints_and_auto_margins_share_one_rule() {
    let (s, c) = sizes(
        "<div id=t style='width:80px;min-width:100px;max-width:60px;box-sizing:border-box;padding:0 10px;border:2px solid;margin:0 auto'>x</div>",
        "t",
    );
    close(s.max_content, text::width("x", 16.0, false));
    close(c.min_content, 100.0);
    close(c.max_content, 100.0);
}

#[test]
fn cyclic_percentage_width_and_edges_stay_unresolved() {
    let (s, c) = sizes(
        "<div id=t style='width:50%;padding:0 10%;margin:0 20%'>alpha</div>",
        "t",
    );
    close(s.max_content, c.max_content);
    assert_eq!(
        Length::Calc {
            px: 20.0,
            percent: 0.0,
            percentage: true,
            nonnegative: true
        }
        .resolve_indefinite(None),
        None
    );
    assert_eq!(
        Length::Calc {
            px: 20.0,
            percent: 0.0,
            percentage: true,
            nonnegative: true
        }
        .resolve(100.0),
        Some(20.0)
    );
}

#[test]
fn intrinsic_keywords_refer_to_content_even_with_border_box() {
    let intrinsic = IntrinsicSizes::new(30.0, 100.0);
    assert_eq!(
        resolve_content_size(
            intrinsic,
            Length::MinContent,
            AvailableSize::Indefinite,
            20.0,
            BoxSizing::BorderBox
        ),
        Some(30.0)
    );
    assert_eq!(
        resolve_content_size(
            intrinsic,
            Length::Px(100.0),
            AvailableSize::Indefinite,
            20.0,
            BoxSizing::BorderBox
        ),
        Some(80.0)
    );
}

#[test]
fn images_preserve_intrinsic_ratio_after_border_box_height() {
    let document =
        html::parse("<img id=t style='height:100px;padding:20px;box-sizing:border-box'>").unwrap();
    let styles = style::compute(&document, &css::parse(&document.stylesheets()));
    let node = document
        .nodes
        .iter()
        .enumerate()
        .find(|(i, _)| document.element(*i).is_some_and(|e| e.tag == "img"))
        .unwrap()
        .0;
    let mut images = vec![None; document.nodes.len()];
    images[node] = Some(ImageSource {
        href: "data:image/png;base64,AA==".into(),
        width: 200.0,
        height: 100.0,
    });
    let measure = IntrinsicCache::new(&document, &styles, &images);
    close(measure.content(node).max_content, 120.0);
    close(measure.contribution(node).max_content, 160.0);
}

#[test]
fn hidden_out_of_flow_and_template_content_never_contribute() {
    let (s, _) = sizes(
        "<div id=t>alpha<span style='display:none'>WWWW</span><span style='position:absolute'>WWWW</span><template><div>WWWW</div></template></div>",
        "t",
    );
    close(s.max_content, text::width("alpha", 16.0, false));
}

#[test]
fn mixed_direction_and_cjk_stay_finite_and_cacheable() {
    let (s, _) = sizes("<div id=t dir=rtl>العربية שלום 中文 e&#x301;</div>", "t");
    assert!(s.min_content > 0.0 && s.max_content >= s.min_content && s.max_content.is_finite());
}
