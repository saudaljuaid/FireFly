use phos::dom::{Document, NodeId, NodeKind};
use phos::html;

fn dump(document: &Document) -> String {
    fn walk(document: &Document, id: NodeId, depth: usize, lines: &mut Vec<String>) {
        for &child in &document.nodes[id].children {
            let indent = "  ".repeat(depth);
            match &document.nodes[child].kind {
                NodeKind::Document => unreachable!(),
                NodeKind::Doctype(doctype) => lines.push(format!(
                    "| {indent}<!DOCTYPE {}>",
                    doctype.name.as_deref().unwrap_or("")
                )),
                NodeKind::Comment(text) => lines.push(format!("| {indent}<!-- {text} -->")),
                NodeKind::Text(text) => lines.push(format!("| {indent}\"{text}\"")),
                NodeKind::Element(element) => {
                    lines.push(format!("| {indent}<{}>", element.tag));
                    let mut attributes = element.attributes.clone();
                    attributes.sort_by(|left, right| left.name.cmp(&right.name));
                    for attribute in attributes {
                        lines.push(format!(
                            "| {indent}  {}=\"{}\"",
                            attribute.name, attribute.value
                        ));
                    }
                    walk(document, child, depth + 1, lines);
                }
            }
        }
    }
    let mut lines = Vec::new();
    walk(document, 0, 0, &mut lines);
    lines.join("\n")
}

fn check_cases(group: &str, data: &str, numbers: &[usize]) {
    let cases: Vec<_> = data.split("#data\n").skip(1).collect();
    assert_eq!(cases.len(), numbers.len(), "{group}");
    for (case, number) in cases.into_iter().zip(numbers) {
        let (input, rest) = case.split_once("\n#errors\n").unwrap();
        let (_, expected) = rest.split_once("#document\n").unwrap();
        let expected = expected.trim_end_matches('\n');
        let document = html::parse(input).unwrap();
        assert_eq!(
            dump(&document),
            expected,
            "{group} case #{number}: {input:?}"
        );
    }
}

#[test]
fn selected_html5lib_tree_construction_cases() {
    check_cases(
        "tests1.dat",
        include_str!("fixtures/tests1.dat"),
        &[1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 34, 50, 55, 84, 85, 86, 88],
    );
    check_cases(
        "blocks.dat",
        include_str!("fixtures/blocks.dat"),
        &[1, 2, 3, 4],
    );
    check_cases(
        "inbody01.dat",
        include_str!("fixtures/inbody01.dat"),
        &[1, 2],
    );
    check_cases(
        "scriptdata01.dat",
        include_str!("fixtures/scriptdata01.dat"),
        &[1, 2],
    );
    check_cases(
        "doctype01.dat",
        include_str!("fixtures/doctype01.dat"),
        &[1, 2],
    );
    check_cases(
        "comments01.dat",
        include_str!("fixtures/comments01.dat"),
        &[1],
    );
}

#[test]
fn explicit_document_and_head_content() {
    let source = "<!doctype html><html lang='en'><head id='h'><base href='/x/'><meta charset='utf-8'><link rel='stylesheet' href='a.css'><title>A &amp; B</title><style>p{color:red}</style><script>if (a < b) c='&amp;'</script></head><body class='page'><p>Hello</p></body></html>";
    let document = html::parse(source).unwrap();
    let html_id = document.nodes[0].children[1];
    let html_element = document.element(html_id).unwrap();
    assert_eq!(html_element.tag, "html");
    assert_eq!(html_element.attribute("lang"), Some("en"));
    let head_id = document.nodes[html_id].children[0];
    let body_id = document.nodes[html_id].children[1];
    assert_eq!(
        document.element(head_id).unwrap().attribute("id"),
        Some("h")
    );
    assert_eq!(
        document.element(body_id).unwrap().attribute("class"),
        Some("page")
    );
    assert_eq!(
        document.nodes[head_id]
            .children
            .iter()
            .filter_map(|&id| document.element(id).map(|element| element.tag.as_str()))
            .collect::<Vec<_>>(),
        ["base", "meta", "link", "title", "style", "script"]
    );
    assert!(dump(&document).contains("\"A & B\""));
    assert!(dump(&document).contains("\"if (a < b) c='&amp;'\""));
    assert!(document.stylesheets().contains("p{color:red}"));
}

#[test]
fn unmatched_end_tags_and_eof_recovery() {
    assert_eq!(
        dump(&html::parse("<body></aside></p></br>").unwrap()),
        "| <html>\n|   <head>\n|   <body>\n|     <p>\n|     <br>"
    );
    assert_eq!(
        dump(&html::parse("<title>A &amp; B").unwrap()),
        "| <html>\n|   <head>\n|     <title>\n|       \"A & B\"\n|   <body>"
    );
    assert_eq!(
        dump(&html::parse("").unwrap()),
        "| <html>\n|   <head>\n|   <body>"
    );
    assert_eq!(
        dump(&html::parse("<body>X</body> \n<!--inside--></html><!--outside-->").unwrap()),
        "| <html>\n|   <head>\n|   <body>\n|     \"X \n\"\n|   <!-- inside -->\n| <!-- outside -->"
    );
}

#[test]
fn preserves_input_and_nesting_limits() {
    assert!(html::parse(&"x".repeat(16 * 1024 * 1024 + 1)).is_err());
    assert!(html::parse(&"<div>".repeat(255)).is_err());
    assert!(html::parse(&"<div>".repeat(254)).is_ok());
}
