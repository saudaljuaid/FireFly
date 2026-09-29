use phos::dom::{Element, Namespace};
use phos::html::{self, ErrorPhase};

#[test]
fn tokenizer_errors_keep_codes_and_utf16_positions() {
    let mut tokenizer = html::tokenizer::Tokenizer::new("😀<div a a>");
    while let Some(token) = tokenizer.next_token() {
        if token == html::tokenizer::Token::Eof {
            break;
        }
    }
    let errors = tokenizer.errors();
    assert_eq!(
        errors.iter().map(|e| e.code).collect::<Vec<_>>(),
        ["duplicate-attribute"]
    );
    assert_eq!(
        (errors[0].position.line, errors[0].position.column),
        (1, 11)
    );

    let mut tokenizer = html::tokenizer::Tokenizer::new("<p a='");
    while let Some(token) = tokenizer.next_token() {
        if token == html::tokenizer::Token::Eof {
            break;
        }
    }
    let eof = tokenizer
        .errors()
        .into_iter()
        .find(|e| e.code == "eof-in-tag")
        .unwrap();
    assert_eq!((eof.position.line, eof.position.column), (1, 7));
}

#[test]
fn tree_errors_survive_reprocessing_and_fragments() {
    let report = html::parse_with_errors("<a>hello</a></oops>").unwrap();
    assert_eq!(
        report.errors.iter().map(|e| e.code).collect::<Vec<_>>(),
        ["missing-doctype", "unmatched-end-tag"]
    );
    assert_eq!(report.errors[0].position.column, 3);
    assert_eq!(report.errors[1].phase, ErrorPhase::TreeConstruction);

    let context = Element {
        namespace: Namespace::Html,
        tag: "div".into(),
        attributes: Vec::new(),
    };
    let fragment = html::parse_fragment_with_errors("<b>x</b></oops>", &context).unwrap();
    assert!(
        fragment
            .errors
            .iter()
            .any(|e| e.code == "unmatched-end-tag")
    );
    assert!(!fragment.errors.iter().any(|e| e.code == "missing-doctype"));
}

#[test]
fn byte_diagnostics_use_decoded_stream_positions() {
    let report =
        html::parse_bytes_with_errors(b"\xef\xbb\xbf<!doctype html>\r\n<p a a>", None).unwrap();
    let duplicate = report
        .errors
        .iter()
        .find(|e| e.code == "duplicate-attribute")
        .unwrap();
    assert_eq!((duplicate.position.line, duplicate.position.column), (2, 7));
}

#[test]
fn recovery_modes_report_once_at_the_original_token_position() {
    for _ in 0..12 {
        let report = html::parse_with_errors(
            "<!doctype html><table>lost<tr><td>x</td></tr></table><svg><g></svg><template><div>",
        )
        .unwrap();
        let tree: Vec<_> = report
            .errors
            .iter()
            .filter(|error| error.phase == ErrorPhase::TreeConstruction)
            .collect();
        assert!(
            tree.iter().any(|error| {
                error.code == "foster-parenting-character-in-table" && error.position.column == 26
            }),
            "{tree:?}"
        );
        assert!(
            tree.iter()
                .any(|error| error.code == "foreign-end-tag-mismatch")
        );
        assert!(tree.iter().any(|error| error.code == "eof-in-template"));
        for (index, error) in tree.iter().enumerate() {
            assert!(!tree[..index].iter().any(|previous| {
                previous.code == error.code && previous.position.offset == error.position.offset
            }));
        }
    }
}

#[test]
fn fragments_and_malformed_eof_keep_tree_positions() {
    let context = Element {
        namespace: Namespace::Html,
        tag: "table".into(),
        attributes: Vec::new(),
    };
    for _ in 0..12 {
        let report =
            html::parse_fragment_with_errors("<tr><td>x</td></tr></oops>", &context).unwrap();
        assert!(report.errors.iter().any(|error| {
            error.code == "unmatched-end-tag"
                && error.phase == ErrorPhase::TreeConstruction
                && error.position.line == 1
        }));
    }
}

#[test]
fn nested_templates_report_each_eof_recovery_step() {
    let report = html::parse_with_errors("<!doctype html><template><template><b>x").unwrap();
    let errors: Vec<_> = report
        .errors
        .iter()
        .filter(|error| error.code == "eof-in-template")
        .collect();
    assert_eq!(errors.len(), 2);
    assert_eq!(errors[0].position, errors[1].position);
}
