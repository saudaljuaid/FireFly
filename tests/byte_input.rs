use phos::dom::{Document, Element, Namespace, NodeKind};
use phos::html::{
    self,
    encoding::{EncodingSource, decode_html_bytes},
};

fn text(document: &Document) -> String {
    document
        .nodes
        .iter()
        .filter_map(|node| match &node.kind {
            NodeKind::Text(value) => Some(value.as_str()),
            _ => None,
        })
        .collect()
}

fn integrity(document: &Document) {
    assert!(document.nodes[0].parent.is_none());
    let mut seen = vec![0; document.nodes.len()];
    for (parent, node) in document.nodes.iter().enumerate() {
        for &child in &node.children {
            assert_eq!(document.nodes[child].parent, Some(parent));
            seen[child] += 1;
        }
    }
    assert_eq!(seen[0], 0);
    assert!(seen.into_iter().skip(1).all(|count| count == 1));
}

#[test]
fn bom_transport_meta_and_alias_precedence() {
    let input = b"\xef\xbb\xbf<meta charset=windows-1252><p>caf\xc3\xa9";
    let decoded = decode_html_bytes(input, Some("text/html; charset=iso-8859-1")).unwrap();
    assert_eq!(decoded.source, EncodingSource::Bom);
    assert_eq!(decoded.encoding, "UTF-8");
    assert!(
        text(&html::parse_bytes(input, Some("text/html; charset=iso-8859-1")).unwrap())
            .contains("caf\u{e9}")
    );

    let input = b"<meta charset=utf-8><p>\x80";
    let decoded = decode_html_bytes(input, Some("text/html; charset=\"latin1\"")).unwrap();
    assert_eq!(decoded.source, EncodingSource::Transport);
    assert_eq!(decoded.encoding, "windows-1252");
    assert!(decoded.text.ends_with('€'));
    assert!(!decoded.had_decoding_errors);

    let input = b"\xff\xfe<\0p\0>\0X\0<\0/\0p\0>\0";
    let decoded = decode_html_bytes(input, Some("text/html; charset=utf-8")).unwrap();
    assert_eq!(decoded.source, EncodingSource::Bom);
    assert_eq!(decoded.encoding, "UTF-16LE");
    assert_eq!(decoded.text, "<p>X</p>");
}

#[test]
fn prescan_boundary_comments_and_pragma() {
    for start in [980, 1000, 1020, 1024] {
        let mut input = vec![b' '; start];
        input.extend_from_slice(b"<meta charset=windows-1252><p>\x80");
        let decoded = decode_html_bytes(&input, None).unwrap();
        if start + b"<meta charset=windows-1252>".len() <= 1024 {
            assert_eq!(decoded.source, EncodingSource::Meta, "start={start}");
            assert!(decoded.text.ends_with('€'));
        } else {
            assert_eq!(decoded.source, EncodingSource::Default, "start={start}");
            assert!(decoded.text.ends_with('\u{fffd}'));
        }
    }
    let commented = b"<!--<meta charset=windows-1252>--><meta http-equiv='Content-Type' content='text/html; charset=shift_jis'><p>\x82\xa0";
    let decoded = decode_html_bytes(commented, None).unwrap();
    assert_eq!(decoded.encoding, "Shift_JIS");
    assert!(decoded.text.ends_with('あ'));
    let fake = decode_html_bytes(b"<!--<meta charset=windows-1252>--><p>\x80", None).unwrap();
    assert_eq!(fake.source, EncodingSource::Default);
    let fake_attribute = decode_html_bytes(
        b"<div data-x='<meta charset=windows-1252>'></div><p>\x80",
        None,
    )
    .unwrap();
    assert_eq!(fake_attribute.source, EncodingSource::Default);
}

#[test]
fn malformed_sequences_newlines_script_attributes_and_fragments() {
    let input = b"<meta charset=utf-8><p title='a\r\nb'>x\r\ny\r\xff<script>a < b</script>";
    let decoded = decode_html_bytes(input, None).unwrap();
    assert!(decoded.had_decoding_errors);
    let document = html::parse_bytes(input, None).unwrap();
    integrity(&document);
    assert!(text(&document).contains("x\ny\n\u{fffd}a < b"));
    let title = document.nodes.iter().find_map(|node| match &node.kind {
        NodeKind::Element(element) if element.tag == "p" => element.attribute("title"),
        _ => None,
    });
    assert_eq!(title, Some("a\nb"));

    let context = Element {
        namespace: Namespace::Svg,
        tag: "svg".into(),
        attributes: Vec::new(),
    };
    let fragment = html::parse_fragment_bytes(
        b"<![CDATA[\x80]]>",
        &context,
        Some("text/html; charset=windows-1252"),
    )
    .unwrap();
    integrity(&fragment);
    assert_eq!(text(&fragment), "€");

    let svg = html::parse_bytes(
        b"<svg><![CDATA[\x80]]></svg>",
        Some("text/html; charset=windows-1252"),
    )
    .unwrap();
    integrity(&svg);
    assert!(text(&svg).contains('€'));
}

#[test]
fn xml_prescan_aliases_and_multibyte_replacement() {
    let xml = decode_html_bytes(
        b"<?xml version='1.0' encoding='shift_jis'?><p>\x82\xa0",
        None,
    )
    .unwrap();
    assert_eq!(xml.source, EncodingSource::Meta);
    assert_eq!(xml.encoding, "Shift_JIS");
    assert!(xml.text.ends_with('あ'));

    let utf16 = decode_html_bytes(b"<\0?\0x\0m\0l\0>\0<\0p\0>\0Z\0", None).unwrap();
    assert_eq!(utf16.encoding, "UTF-16LE");
    assert_eq!(utf16.source, EncodingSource::Meta);

    let user_defined = decode_html_bytes(b"<meta charset=x-user-defined><p>\x80", None).unwrap();
    assert_eq!(user_defined.encoding, "windows-1252");
    assert!(user_defined.text.ends_with('€'));

    let broken_shift_jis =
        decode_html_bytes(b"\x82", Some("text/html; charset=shift_jis")).unwrap();
    assert_eq!(broken_shift_jis.text, "\u{fffd}");
    assert!(broken_shift_jis.had_decoding_errors);
    let odd_utf16 = decode_html_bytes(b"\xff\xfe<\0p", None).unwrap();
    assert!(odd_utf16.had_decoding_errors);
    assert!(odd_utf16.text.ends_with('\u{fffd}'));
}

#[test]
fn repeated_limits_and_truncated_sequences() {
    for _ in 0..3 {
        let bytes = [b"<p>".as_slice(), &[0xf0, 0x9f, 0x92]].concat();
        let decoded = decode_html_bytes(&bytes, None).unwrap();
        assert_eq!(decoded.text, "<p>\u{fffd}");
        assert!(decoded.had_decoding_errors);
        integrity(&html::parse_bytes(&bytes, None).unwrap());
        assert!(html::parse_bytes(&vec![b'x'; 16 * 1024 * 1024 + 1], None).is_err());
    }
    let at_limit = vec![b'x'; 16 * 1024 * 1024];
    integrity(&html::parse_bytes(&at_limit, None).unwrap());
}

#[test]
fn repeated_prescan_and_eof_recovery_remain_bounded() {
    let endings: [&[u8]; 6] = [
        b"<!DOCTYPE html PUBLIC '",
        b"<!--",
        b"<div a='",
        b"<script><!--",
        b"<svg><![CDATA[",
        b"<table><tr><td>x",
    ];
    let context = Element {
        namespace: Namespace::Html,
        tag: "table".into(),
        attributes: Vec::new(),
    };
    for seed in 0..48 {
        let mut input = vec![b' '; 970 + seed % 70];
        input.extend_from_slice(b"<meta charset=windows-1252>");
        input.extend_from_slice(endings[seed % endings.len()]);
        input.extend_from_slice(&[0x80, 0xf0, 0x9f]);
        let report = html::parse_bytes_with_errors(&input, None).unwrap();
        integrity(&report.document);
        let fragment = html::parse_fragment_bytes_with_errors(&input, &context, None).unwrap();
        integrity(&fragment.document);
    }
}

#[test]
fn late_meta_restarts_tentative_document_at_and_beyond_prescan_boundary() {
    for start in [990, 999, 1000, 1010, 1023, 1024, 1100, 2048] {
        for _ in 0..3 {
            let mut input = vec![b' '; start];
            input.extend_from_slice(b"<meta charset=latin1><p>Price \x80</p>");
            let decoded = decode_html_bytes(&input, None).unwrap();
            if start + b"<meta charset=latin1>".len() > 1024 {
                assert_eq!(decoded.source, EncodingSource::Default);
                assert!(decoded.text.ends_with("Price \u{fffd}</p>"));
            }
            let report = html::parse_bytes_with_errors(&input, None).unwrap();
            integrity(&report.document);
            assert!(text(&report.document).ends_with("Price €"), "start={start}");
        }
    }
}

#[test]
fn late_meta_respects_certain_encoding_and_first_valid_declaration() {
    let mut input = vec![b' '; 1040];
    input.extend_from_slice(b"<meta charset=latin1><meta charset=utf-8><p>\x80");
    for _ in 0..3 {
        let document = html::parse_bytes(&input, None).unwrap();
        integrity(&document);
        assert!(text(&document).ends_with('€'));
        let bom = [b"\xef\xbb\xbf".as_slice(), input.as_slice()].concat();
        let document = html::parse_bytes(&bom, None).unwrap();
        integrity(&document);
        assert!(text(&document).ends_with('\u{fffd}'));
        let document = html::parse_bytes(&input, Some("text/html; charset=utf-8")).unwrap();
        integrity(&document);
        assert!(text(&document).ends_with('\u{fffd}'));
    }
    let mut invalid = vec![b' '; 1040];
    invalid.extend_from_slice(b"<meta charset=made-up><meta http-equiv=Content-Type content='text/html; charset=windows-1252'><p>\x80");
    let document = html::parse_bytes(&invalid, None).unwrap();
    integrity(&document);
    assert!(text(&document).ends_with('€'));

    let mut fallback = vec![b' '; 1040];
    fallback.extend_from_slice(b"<meta charset=unknown http-equiv=Content-Type content='text/html; charset=latin1'><p>\x80");
    let document = html::parse_bytes(&fallback, None).unwrap();
    integrity(&document);
    assert!(text(&document).ends_with('€'));

    let utf16: Vec<u8> = "<?xml?><meta charset=windows-1252><p>Z"
        .encode_utf16()
        .flat_map(u16::to_le_bytes)
        .collect();
    assert_eq!(
        decode_html_bytes(&utf16, None).unwrap().encoding,
        "UTF-16LE"
    );
    let document = html::parse_bytes(&utf16, None).unwrap();
    integrity(&document);
    assert!(text(&document).ends_with('Z'));
}

#[test]
fn byte_fragments_and_decoded_strings_do_not_restart() {
    let mut input = vec![b' '; 1040];
    input.extend_from_slice(b"<meta charset=windows-1252><p>\x80");
    let context = Element {
        namespace: Namespace::Html,
        tag: "div".into(),
        attributes: Vec::new(),
    };
    let fragment = html::parse_fragment_bytes(&input, &context, None).unwrap();
    integrity(&fragment);
    assert!(text(&fragment).ends_with('\u{fffd}'));
    let decoded = decode_html_bytes(&input, None).unwrap();
    let string_document = html::parse(&decoded.text).unwrap();
    integrity(&string_document);
    assert!(text(&string_document).ends_with('\u{fffd}'));
}

#[test]
fn late_meta_after_script_text_restarts_with_truncated_multibyte_input() {
    for _ in 0..12 {
        let mut input = b"<script>var marker = '<meta charset=utf-8>';</script>".to_vec();
        input.resize(1040, b' ');
        input.extend_from_slice(b"<meta charset=shift_jis><p>\x82\xa0\x82");
        let report = html::parse_bytes_with_errors(&input, None).unwrap();
        integrity(&report.document);
        assert!(text(&report.document).ends_with("あ\u{fffd}"));
        assert!(
            report
                .errors
                .iter()
                .all(|error| error.position.offset <= input.len())
        );
    }
}
