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
