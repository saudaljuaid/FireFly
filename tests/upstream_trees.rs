use phos::dom::{Document, Element, Namespace, NodeId, NodeKind};
use phos::html;
use std::fs;
use std::path::Path;

fn dump(document: &Document) -> String {
    fn walk(document: &Document, id: NodeId, depth: usize, lines: &mut Vec<String>) {
        for &child in &document.nodes[id].children {
            let indent = "  ".repeat(depth);
            match &document.nodes[child].kind {
                NodeKind::Document => panic!("nested document"),
                NodeKind::TemplateContent => {
                    lines.push(format!("| {indent}content"));
                    walk(document, child, depth + 1, lines);
                }
                NodeKind::Doctype(doctype) => {
                    let name = doctype.name.as_deref().unwrap_or("");
                    if doctype.public_id.is_some() || doctype.system_id.is_some() {
                        lines.push(format!(
                            "| {indent}<!DOCTYPE {name} \"{}\" \"{}\">",
                            doctype.public_id.as_deref().unwrap_or(""),
                            doctype.system_id.as_deref().unwrap_or("")
                        ));
                    } else {
                        lines.push(format!("| {indent}<!DOCTYPE {name}>"));
                    }
                }
                NodeKind::Comment(text) => lines.push(format!("| {indent}<!-- {text} -->")),
                NodeKind::ProcessingInstruction { target, data } => {
                    lines.push(format!("| {indent}<?{target} {data}?>"));
                }
                NodeKind::Text(text) => lines.push(format!("| {indent}\"{text}\"")),
                NodeKind::Element(element) => {
                    let prefix = match element.namespace {
                        Namespace::Html => "",
                        Namespace::MathMl => "math ",
                        Namespace::Svg => "svg ",
                    };
                    lines.push(format!("| {indent}<{prefix}{}>", element.tag));
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

fn assert_integrity(document: &Document) {
    assert!(matches!(document.nodes[0].kind, NodeKind::Document));
    assert!(document.nodes[0].parent.is_none());
    let mut links = vec![0usize; document.nodes.len()];
    for (parent, node) in document.nodes.iter().enumerate() {
        if let NodeKind::Element(element) = &node.kind {
            let mut names = std::collections::HashSet::new();
            for attribute in &element.attributes {
                assert!(
                    names.insert(&attribute.name),
                    "duplicate attribute on {parent}"
                );
            }
        }
        if matches!(node.kind, NodeKind::TemplateContent) {
            let template = node.parent.expect("template content must have a parent");
            assert!(document.element(template).is_some_and(|element| {
                element.namespace == Namespace::Html && element.tag == "template"
            }));
            assert_eq!(document.nodes[template].children.first(), Some(&parent));
        }
        for &child in &node.children {
            assert!(child < document.nodes.len());
            assert_eq!(document.nodes[child].parent, Some(parent));
            links[child] += 1;
        }
    }
    for (id, node) in document.nodes.iter().enumerate() {
        assert_eq!(links[id], usize::from(node.parent.is_some()), "node {id}");
    }
    let mut visited = vec![false; document.nodes.len()];
    for (id, node) in document.nodes.iter().enumerate() {
        if node.parent.is_none() {
            let mut stack = vec![id];
            while let Some(current) = stack.pop() {
                assert!(!visited[current], "cycle or duplicate node {current}");
                visited[current] = true;
                stack.extend(&document.nodes[current].children);
            }
        }
    }
    assert!(visited.into_iter().all(|seen| seen));
}

fn cases(data: &str) -> Vec<(String, String, Option<String>, bool)> {
    let normalized = data.replace("\r\n", "\n");
    normalized
        .split("#data\n")
        .skip(1)
        .filter_map(|case| {
            let (input, rest) = case
                .split_once("\n#errors\n")
                .or_else(|| case.strip_prefix("#errors\n").map(|rest| ("", rest)))?;
            let (_, expected) = rest.split_once("#document\n")?;
            Some((
                input.to_owned(),
                expected.trim_end_matches('\n').to_owned(),
                rest.split_once("#document-fragment\n")
                    .and_then(|(_, suffix)| suffix.lines().next())
                    .map(str::to_owned),
                rest.contains("#script-on\n"),
            ))
        })
        .collect()
}

#[test]
fn upstream_document_trees() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/upstream/tree");
    let mut checked = 0;
    for entry in fs::read_dir(root).unwrap() {
        let path = entry.unwrap().path();
        let data = fs::read_to_string(&path).unwrap();
        for (index, (input, expected, context, script_on)) in cases(&data).into_iter().enumerate() {
            if context.is_some() || script_on {
                continue;
            }
            let document = html::parse(&input)
                .unwrap_or_else(|error| panic!("{} #{}: {error:?}", path.display(), index + 1));
            assert_integrity(&document);
            assert_eq!(
                dump(&document),
                expected,
                "{} #{}",
                path.display(),
                index + 1
            );
            assert!(
                document
                    .nodes
                    .iter()
                    .skip(1)
                    .all(|node| node.parent.is_some()),
                "document contains a detached internal node: {} #{}",
                path.display(),
                index + 1
            );
            checked += 1;
        }
    }
    assert_eq!(checked, 1739, "document fixtures are missing");
}

#[test]
fn upstream_script_on_document_trees() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/upstream/tree");
    let mut checked = 0;
    for entry in fs::read_dir(root).unwrap() {
        let path = entry.unwrap().path();
        let data = fs::read_to_string(&path).unwrap();
        for (index, (input, expected, context, script_on)) in cases(&data).into_iter().enumerate() {
            if context.is_some() || !script_on {
                continue;
            }
            let document = html::parse_with_scripting(&input, true)
                .unwrap_or_else(|error| panic!("{} #{}: {error:?}", path.display(), index + 1));
            assert_integrity(&document);
            assert_eq!(
                dump(&document),
                expected,
                "{} #{}",
                path.display(),
                index + 1
            );
            checked += 1;
        }
    }
    assert_eq!(checked, 8, "script-on fixtures are missing");
}

#[test]
#[ignore = "diagnostic: run with --ignored --nocapture to print the full upstream inventory"]
fn inventory_upstream_document_trees() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/upstream/tree");
    let mut files: Vec<_> = fs::read_dir(root)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect();
    files.sort();
    let mut totals = (0, 0, 0);
    for path in files {
        if let Ok(only) = std::env::var("CORPUS_FILE")
            && path.file_name().unwrap() != only.as_str()
        {
            continue;
        }
        let mut passed = 0;
        let mut failed = 0;
        let mut skipped = 0;
        let data = fs::read_to_string(&path).unwrap();
        for (index, (input, expected, context, script_on)) in cases(&data).into_iter().enumerate() {
            if let Ok(only) = std::env::var("CORPUS_CASE")
                && only.parse::<usize>().unwrap() != index + 1
            {
                continue;
            }
            if context.is_some() || script_on {
                skipped += 1;
                continue;
            }
            match html::parse(&input) {
                Ok(document) => {
                    assert_integrity(&document);
                    let actual = dump(&document);
                    if actual == expected {
                        passed += 1;
                    } else {
                        failed += 1;
                        if failed <= 3 || std::env::var_os("CORPUS_ALL_FAILURES").is_some() {
                            println!(
                                "  FAIL #{} {:?}",
                                index + 1,
                                input.chars().take(100).collect::<String>()
                            );
                        }
                        if std::env::var_os("CORPUS_VERBOSE").is_some() {
                            println!("INPUT:\n{input}\nEXPECTED:\n{expected}\nACTUAL:\n{actual}");
                        }
                    }
                }
                Err(error) => {
                    failed += 1;
                    if failed <= 3 || std::env::var_os("CORPUS_ALL_FAILURES").is_some() {
                        println!("  ERROR #{} {error:?}", index + 1);
                    }
                }
            }
        }
        println!(
            "{}: passed={passed} failed={failed} skipped={skipped}",
            path.file_name().unwrap().to_string_lossy()
        );
        totals.0 += passed;
        totals.1 += failed;
        totals.2 += skipped;
    }
    println!(
        "TOTAL: passed={} failed={} skipped={}",
        totals.0, totals.1, totals.2
    );
}

fn fragment_context(spec: &str) -> Element {
    let (namespace, tag) = if let Some(tag) = spec.strip_prefix("svg ") {
        (Namespace::Svg, tag)
    } else if let Some(tag) = spec.strip_prefix("math ") {
        (Namespace::MathMl, tag)
    } else {
        (Namespace::Html, spec)
    };
    Element {
        namespace,
        tag: tag.to_owned(),
        attributes: Vec::new(),
    }
}

#[test]
fn upstream_fragment_trees() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/upstream/tree");
    let mut checked = 0;
    for entry in fs::read_dir(root).unwrap() {
        let path = entry.unwrap().path();
        let data = fs::read_to_string(&path).unwrap();
        for (index, (input, expected, context, script_on)) in cases(&data).into_iter().enumerate() {
            let Some(context) = context else { continue };
            if script_on {
                continue;
            }
            let document = html::parse_fragment(&input, &fragment_context(&context))
                .unwrap_or_else(|error| panic!("{} #{}: {error:?}", path.display(), index + 1));
            assert_integrity(&document);
            assert_eq!(
                dump(&document),
                expected,
                "{} #{} ({context})",
                path.display(),
                index + 1
            );
            assert!(
                document
                    .nodes
                    .iter()
                    .skip(1)
                    .all(|node| node.parent.is_some()),
                "fragment contains a detached internal node: {} #{}",
                path.display(),
                index + 1
            );
            checked += 1;
        }
    }
    assert_eq!(checked, 206, "fragment fixtures are missing");
}

#[test]
#[ignore = "diagnostic: run with --ignored --nocapture to print fragment inventory"]
fn inventory_upstream_fragments() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/upstream/tree");
    let mut files: Vec<_> = fs::read_dir(root)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect();
    files.sort();
    let mut totals = (0, 0, 0);
    for path in files {
        if let Ok(only) = std::env::var("CORPUS_FILE")
            && path.file_name().unwrap() != only.as_str()
        {
            continue;
        }
        let mut passed = 0;
        let mut failed = 0;
        let mut skipped = 0;
        let data = fs::read_to_string(&path).unwrap();
        for (index, (input, expected, context, script_on)) in cases(&data).into_iter().enumerate() {
            if let Ok(only) = std::env::var("CORPUS_CASE")
                && only.parse::<usize>().unwrap() != index + 1
            {
                continue;
            }
            let Some(context) = context else { continue };
            if script_on {
                skipped += 1;
                continue;
            }
            match html::parse_fragment(&input, &fragment_context(&context)) {
                Ok(document) => {
                    assert_integrity(&document);
                    let actual = dump(&document);
                    if actual == expected {
                        passed += 1;
                    } else {
                        failed += 1;
                        if failed <= 3 {
                            println!(
                                "  FAIL #{} {context} {:?}",
                                index + 1,
                                input.chars().take(100).collect::<String>()
                            );
                        }
                        if std::env::var_os("CORPUS_VERBOSE").is_some() {
                            println!("INPUT:\n{input}\nEXPECTED:\n{expected}\nACTUAL:\n{actual}");
                        }
                    }
                }
                Err(error) => {
                    failed += 1;
                    if failed <= 3 {
                        println!("  ERROR #{} {context} {error:?}", index + 1);
                    }
                }
            }
        }
        if passed + failed + skipped > 0 {
            println!(
                "{}: passed={passed} failed={failed} skipped={skipped}",
                path.file_name().unwrap().to_string_lossy()
            );
        }
        totals.0 += passed;
        totals.1 += failed;
        totals.2 += skipped;
    }
    println!(
        "TOTAL: passed={} failed={} skipped={}",
        totals.0, totals.1, totals.2
    );
}

#[test]
fn repeated_adversarial_recovery_preserves_the_arena() {
    let chunks = [
        "<b><i><p>x</b>y</i></p>",
        "<table><caption><form><tr><td>z</table>",
        "<svg><foreignObject><math><mi><b>m</b></mi></math></foreignObject></svg>",
        "<template><table><tr><td>x</template>",
        "<select><optgroup><option>a<tr><td>b</table>",
        "<!---!><!DOCTYPE html PUBLIC 'bad'><script><!--<script>x</script>--></script>",
    ];
    for seed in 0..24 {
        let mut input = String::new();
        for index in 0..60 {
            input.push_str(chunks[(seed + index * 7) % chunks.len()]);
        }
        for document in [
            html::parse(&input).unwrap(),
            html::parse_fragment(&input, &fragment_context("table")).unwrap(),
            html::parse_fragment(&input, &fragment_context("svg svg")).unwrap(),
        ] {
            assert_integrity(&document);
            assert!(
                document
                    .nodes
                    .iter()
                    .skip(1)
                    .all(|node| node.parent.is_some())
            );
        }
    }
}

#[test]
fn fragment_and_document_bounds_hold_repeatedly() {
    let context = fragment_context("template");
    let too_large = "a".repeat(16 * 1024 * 1024 + 1);
    for _ in 0..3 {
        assert!(html::parse(&too_large).is_err());
        assert!(html::parse_fragment(&too_large, &context).is_err());
        assert!(html::parse(&"<b>".repeat(255)).is_err());
        assert!(html::parse_fragment(&"<b>".repeat(256), &context).is_err());
    }
    let at_limit = html::parse_fragment(&"<b>".repeat(254), &context).unwrap();
    assert_integrity(&at_limit);
}

#[test]
fn form_fragment_context_initializes_the_form_pointer() {
    let context = fragment_context("form");
    let document = html::parse_fragment(
        "<form id=ignored><p>first</form><form id=later>second",
        &context,
    )
    .unwrap();
    assert_integrity(&document);
    assert_eq!(
        dump(&document),
        "| <p>\n|   \"first\"\n| <form>\n|   id=\"later\"\n|   \"second\""
    );
}

#[test]
fn noscript_fragment_respects_the_scripting_flag() {
    let context = fragment_context("noscript");
    let inert = html::parse_fragment("<b>x</b>", &context).unwrap();
    let scripting = html::parse_fragment_with_scripting("<b>x</b>", &context, true).unwrap();
    assert_integrity(&inert);
    assert_integrity(&scripting);
    assert_eq!(dump(&inert), "| <b>\n|   \"x\"");
    assert_eq!(dump(&scripting), "| \"<b>x</b>\"");
}
