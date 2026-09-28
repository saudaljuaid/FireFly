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
    let normalized = data.replace("\r\n", "\n");
    let cases: Vec<_> = normalized.split("#data\n").skip(1).collect();
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
    check_cases(
        "tables01.dat",
        include_str!("fixtures/tables01.dat"),
        &[1, 2, 3, 4, 5, 6, 11, 12, 13, 14, 15, 16, 19],
    );
    check_cases(
        "adoption01.dat",
        include_str!("fixtures/adoption01.dat"),
        &[1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 14, 15, 16],
    );
    check_cases(
        "adoption02.dat",
        include_str!("fixtures/adoption02.dat"),
        &[1, 2, 3, 4],
    );
}

#[test]
fn formatting_reconstructs_across_paragraphs_and_misnested_ends() {
    assert_eq!(
        dump(&html::parse("<p><b>one<p>two</b>three").unwrap()),
        "| <html>\n|   <head>\n|   <body>\n|     <p>\n|       <b>\n|         \"one\"\n|     <p>\n|       <b>\n|         \"two\"\n|       \"three\""
    );
    assert_eq!(
        dump(&html::parse("<b>1<i>2</b>3</i>").unwrap()),
        "| <html>\n|   <head>\n|   <body>\n|     <b>\n|       \"1\"\n|       <i>\n|         \"2\"\n|     <i>\n|       \"3\""
    );
}

#[test]
fn nested_anchors_close_the_previous_active_anchor() {
    assert_eq!(
        dump(&html::parse("<a href=one>x<a href=two>y</a>z").unwrap()),
        "| <html>\n|   <head>\n|   <body>\n|     <a>\n|       href=\"one\"\n|       \"x\"\n|     <a>\n|       href=\"two\"\n|       \"y\"\n|     \"z\""
    );
}

#[test]
fn formatting_markers_keep_caption_and_cells_separate() {
    assert_eq!(
        dump(
            &html::parse("<table><caption><b>cap</caption><tr><td><i>first<td>second</table>after")
                .unwrap()
        ),
        "| <html>\n|   <head>\n|   <body>\n|     <table>\n|       <caption>\n|         <b>\n|           \"cap\"\n|       <tbody>\n|         <tr>\n|           <td>\n|             <i>\n|               \"first\"\n|           <td>\n|             \"second\"\n|     \"after\""
    );
    assert_eq!(
        dump(&html::parse("<table><caption><b>open<tr><td>cell</table>").unwrap()),
        "| <html>\n|   <head>\n|   <body>\n|     <table>\n|       <caption>\n|         <b>\n|           \"open\"\n|       <tbody>\n|         <tr>\n|           <td>\n|             \"cell\""
    );
}

#[test]
fn repeated_identical_formatting_is_limited_and_resets_at_markers() {
    let tree = dump(&html::parse("<p><b x=1 y=2><b y=2 x=1><b x=1 y=2><b y=2 x=1><p>x").unwrap());
    let last_paragraph = tree.rsplit("|     <p>").next().unwrap();
    assert_eq!(last_paragraph.matches("<b>").count(), 3);
    assert!(last_paragraph.contains("\"x\""));
    assert_eq!(
        dump(&html::parse("<table><tr><td><b>x<td>y</table>").unwrap())
            .matches("<b>")
            .count(),
        1
    );
}

#[test]
fn repeated_misnesting_keeps_the_arena_consistent() {
    let input = "<b><i><p>x</b>y</i></p>".repeat(500);
    let document = html::parse(&input).unwrap();
    let mut seen = vec![false; document.nodes.len()];
    fn walk(document: &Document, id: NodeId, seen: &mut [bool]) {
        assert!(!seen[id], "node visited twice: {id}");
        seen[id] = true;
        for &child in &document.nodes[id].children {
            assert_eq!(document.nodes[child].parent, Some(id));
            walk(document, child, seen);
        }
    }
    walk(&document, 0, &mut seen);
    assert_eq!(dump(&document).matches("\"x\"").count(), 500);
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
    assert!(html::parse(&format!("{}<table><td>", "<div>".repeat(251))).is_err());
    assert!(html::parse(&format!("{}<table><caption><p>", "<div>".repeat(252))).is_err());
}

#[test]
fn table_sections_rows_and_cells_are_explicit_or_implied() {
    assert_eq!(
        dump(&html::parse("<table><td>A<th>B<tr><td>C</table>").unwrap()),
        "| <html>\n|   <head>\n|   <body>\n|     <table>\n|       <tbody>\n|         <tr>\n|           <td>\n|             \"A\"\n|           <th>\n|             \"B\"\n|         <tr>\n|           <td>\n|             \"C\""
    );
    assert_eq!(
        dump(
            &html::parse(
                "<table><thead><tr><th>H</thead><tbody><td>D</tbody><tfoot><tr><td>F</table>"
            )
            .unwrap()
        ),
        "| <html>\n|   <head>\n|   <body>\n|     <table>\n|       <thead>\n|         <tr>\n|           <th>\n|             \"H\"\n|       <tbody>\n|         <tr>\n|           <td>\n|             \"D\"\n|       <tfoot>\n|         <tr>\n|           <td>\n|             \"F\""
    );
}

#[test]
fn table_end_tags_and_eof_recover_without_leaking_cell_state() {
    assert_eq!(
        dump(&html::parse("<table><tbody><tr><td>A</table><p>B").unwrap()),
        "| <html>\n|   <head>\n|   <body>\n|     <table>\n|       <tbody>\n|         <tr>\n|           <td>\n|             \"A\"\n|     <p>\n|       \"B\""
    );
    assert_eq!(
        dump(&html::parse("<table><tr><td>A").unwrap()),
        "| <html>\n|   <head>\n|   <body>\n|     <table>\n|       <tbody>\n|         <tr>\n|           <td>\n|             \"A\""
    );
    assert_eq!(
        dump(
            &html::parse("<table><tr><td><table><td>inner</table>outer</td></tr></table>").unwrap()
        ),
        "| <html>\n|   <head>\n|   <body>\n|     <table>\n|       <tbody>\n|         <tr>\n|           <td>\n|             <table>\n|               <tbody>\n|                 <tr>\n|                   <td>\n|                     \"inner\"\n|             \"outer\""
    );
}

#[test]
fn buffered_table_text_stays_inside_only_when_all_whitespace() {
    assert_eq!(
        dump(&html::parse("<table> \n<tr><td>X</table>").unwrap()),
        "| <html>\n|   <head>\n|   <body>\n|     <table>\n|       \" \n\"\n|       <tbody>\n|         <tr>\n|           <td>\n|             \"X\""
    );
    assert_eq!(
        dump(&html::parse("<table> A&amp;B<tr><td>X</table>").unwrap()),
        "| <html>\n|   <head>\n|   <body>\n|     \" A&B\"\n|     <table>\n|       <tbody>\n|         <tr>\n|           <td>\n|             \"X\""
    );
}

#[test]
fn fostered_text_and_elements_precede_the_table() {
    assert_eq!(
        dump(
            &html::parse(
                "<div>before<table><div class=x>outside</div>tail<tr><td>inside</table>after"
            )
            .unwrap()
        ),
        "| <html>\n|   <head>\n|   <body>\n|     <div>\n|       \"before\"\n|       <div>\n|         class=\"x\"\n|         \"outside\"\n|       \"tail\"\n|       <table>\n|         <tbody>\n|           <tr>\n|             <td>\n|               \"inside\"\n|       \"after\""
    );
    assert_eq!(
        dump(&html::parse("<table><p>one</p><tr><td>two").unwrap()),
        "| <html>\n|   <head>\n|   <body>\n|     <p>\n|       \"one\"\n|     <table>\n|       <tbody>\n|         <tr>\n|           <td>\n|             \"two\""
    );
}

#[test]
fn column_groups_keep_attributes_whitespace_and_transition_to_rows() {
    assert_eq!(
        dump(&html::parse("<table><colgroup span=2> \n<!--gap--><col span=2><col class=x></col><tr><td>R</table>").unwrap()),
        "| <html>\n|   <head>\n|   <body>\n|     <table>\n|       <colgroup>\n|         span=\"2\"\n|         \" \n\"\n|         <!-- gap -->\n|         <col>\n|           span=\"2\"\n|         <col>\n|           class=\"x\"\n|       <tbody>\n|         <tr>\n|           <td>\n|             \"R\""
    );
    assert_eq!(
        dump(&html::parse("<table><col id=a><col id=b><tbody><tr><td>X").unwrap()),
        "| <html>\n|   <head>\n|   <body>\n|     <table>\n|       <colgroup>\n|         <col>\n|           id=\"a\"\n|         <col>\n|           id=\"b\"\n|       <tbody>\n|         <tr>\n|           <td>\n|             \"X\""
    );
}

#[test]
fn malformed_column_group_endings_and_text_use_table_recovery() {
    assert_eq!(
        dump(&html::parse("<table><colgroup></col> \nX</caption><tr><td>Y</table>").unwrap()),
        "| <html>\n|   <head>\n|   <body>\n|     \"X\"\n|     <table>\n|       <colgroup>\n|         \" \n\"\n|       <tbody>\n|         <tr>\n|           <td>\n|             \"Y\""
    );
    assert_eq!(
        dump(&html::parse("<table><colgroup><col></colgroup><tr><td>Z").unwrap()),
        "| <html>\n|   <head>\n|   <body>\n|     <table>\n|       <colgroup>\n|         <col>\n|       <tbody>\n|         <tr>\n|           <td>\n|             \"Z\""
    );
}

#[test]
fn caption_content_closes_before_columns_sections_and_rows() {
    assert_eq!(
        dump(&html::parse("<table><caption><p>Title <em>now</em><tr><td>First</table>").unwrap()),
        "| <html>\n|   <head>\n|   <body>\n|     <table>\n|       <caption>\n|         <p>\n|           \"Title \"\n|           <em>\n|             \"now\"\n|       <tbody>\n|         <tr>\n|           <td>\n|             \"First\""
    );
    assert_eq!(
        dump(&html::parse("<table><caption>Head<col span=3><thead><tr><th>H</table>").unwrap()),
        "| <html>\n|   <head>\n|   <body>\n|     <table>\n|       <caption>\n|         \"Head\"\n|       <colgroup>\n|         <col>\n|           span=\"3\"\n|       <thead>\n|         <tr>\n|           <th>\n|             \"H\""
    );
    assert_eq!(
        dump(&html::parse("<table><colgroup><col><caption>Note</table><p>After").unwrap()),
        "| <html>\n|   <head>\n|   <body>\n|     <table>\n|       <colgroup>\n|         <col>\n|       <caption>\n|         \"Note\"\n|     <p>\n|       \"After\""
    );
}

#[test]
fn malformed_caption_endings_and_eof_preserve_tree_shape() {
    assert_eq!(
        dump(&html::parse("<table><caption><p>A</colgroup></td></caption><tr><td>B").unwrap()),
        "| <html>\n|   <head>\n|   <body>\n|     <table>\n|       <caption>\n|         <p>\n|           \"A\"\n|       <tbody>\n|         <tr>\n|           <td>\n|             \"B\""
    );
    assert_eq!(
        dump(&html::parse("<table><caption>Open").unwrap()),
        "| <html>\n|   <head>\n|   <body>\n|     <table>\n|       <caption>\n|         \"Open\""
    );
}
