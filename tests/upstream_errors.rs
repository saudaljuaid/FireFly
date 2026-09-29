//! Diagnostic inventory against unmodified pinned upstream expectations.
//! A historical tree error is unrepresentable when there is no one-to-one
//! mapping to a diagnostic that this parser can currently express.

use std::{fs, path::Path};

use phos::{
    dom::{Element, Namespace},
    html::{
        self,
        tokenizer::{Token, Tokenizer},
    },
};
use serde_json::Value;

fn double_unescape(input: &str) -> Option<String> {
    let mut chars = input.chars().peekable();
    let mut result = String::new();
    while let Some(ch) = chars.next() {
        if ch == '\\' && chars.peek() == Some(&'u') {
            chars.next();
            let hex: String = chars.by_ref().take(4).collect();
            result.push(char::from_u32(u32::from_str_radix(&hex, 16).ok()?)?);
        } else {
            result.push(ch);
        }
    }
    Some(result)
}

#[test]
fn html5lib_tokenizer_error_inventory() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/upstream/tokenizer");
    let mut files: Vec<_> = fs::read_dir(root)
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect();
    files.sort();
    let mut totals = (0, 0, 0);
    for path in files {
        let data: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        let mut counts = (0, 0, 0);
        for case in data["tests"].as_array().unwrap() {
            let states = case["initialStates"]
                .as_array()
                .map(|v| v.iter().map(|s| s.as_str().unwrap()).collect::<Vec<_>>())
                .unwrap_or_else(|| vec!["Data state"]);
            let source = case["input"].as_str().unwrap();
            let input = if case["doubleEscaped"] == true {
                double_unescape(source)
            } else {
                Some(source.to_owned())
            };
            for state in states {
                let Some(input) = &input else {
                    counts.2 += 1;
                    continue;
                };
                let mut tokenizer = Tokenizer::new_legacy_html5lib(input);
                let last = case["lastStartTag"].as_str().unwrap_or("");
                match state {
                    "Data state" => {}
                    "PLAINTEXT state" => tokenizer.enter_plaintext(),
                    "RCDATA state" => tokenizer.enter_rcdata(last),
                    "RAWTEXT state" => tokenizer.enter_rawtext(last),
                    "Script data state" => tokenizer.enter_script_data(last),
                    "CDATA section state" => tokenizer.enter_cdata(),
                    other => panic!("unknown state: {other}"),
                }
                tokenizer.set_last_start_tag(last);
                while let Some(token) = tokenizer.next_token() {
                    if token == Token::Eof {
                        break;
                    }
                }
                let actual: Vec<_> = tokenizer
                    .errors()
                    .iter()
                    .map(|error| {
                        (
                            error.code.to_owned(),
                            error.position.line,
                            error.position.column,
                        )
                    })
                    .collect();
                let expected: Vec<_> = case["errors"]
                    .as_array()
                    .map(|v| {
                        v.iter()
                            .map(|e| {
                                (
                                    e["code"].as_str().unwrap().to_owned(),
                                    e["line"].as_u64().unwrap() as usize,
                                    e["col"].as_u64().unwrap() as usize,
                                )
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                if actual == expected {
                    counts.0 += 1
                } else {
                    counts.1 += 1;
                    if std::env::var_os("CORPUS_VERBOSE").is_some() {
                        println!(
                            "TOKEN ERROR {:?} {state}: expected={expected:?} actual={actual:?}",
                            case["description"]
                        );
                    }
                }
            }
        }
        println!(
            "{} errors: passed={} failed={} unrepresentable={}",
            path.file_name().unwrap().to_string_lossy(),
            counts.0,
            counts.1,
            counts.2
        );
        totals.0 += counts.0;
        totals.1 += counts.1;
        totals.2 += counts.2;
    }
    println!(
        "TOKEN ERRORS TOTAL: passed={} failed={} unrepresentable={}",
        totals.0, totals.1, totals.2
    );
    assert_eq!(totals, (7028, 0, 4), "tokenizer error regression");
}

fn tree_cases(data: &str) -> Vec<(String, String, Option<String>, bool)> {
    data.replace("\r\n", "\n")
        .split("#data\n")
        .skip(1)
        .filter_map(|case| {
            let (input, rest) = case
                .split_once("\n#errors\n")
                .or_else(|| case.strip_prefix("#errors\n").map(|rest| ("", rest)))?;
            let (errors, _) = rest.split_once("#document\n")?;
            let context = rest
                .split_once("#document-fragment\n")
                .and_then(|(_, tail)| tail.lines().next())
                .map(str::to_owned);
            Some((
                input.to_owned(),
                errors.to_owned(),
                context,
                rest.contains("#script-on\n"),
            ))
        })
        .collect()
}

fn context(spec: &str) -> Element {
    let (namespace, tag) = if let Some(tag) = spec.strip_prefix("svg ") {
        (Namespace::Svg, tag)
    } else if let Some(tag) = spec.strip_prefix("math ") {
        (Namespace::MathMl, tag)
    } else {
        (Namespace::Html, spec)
    };
    Element {
        namespace,
        tag: tag.into(),
        attributes: Vec::new(),
    }
}

fn historical_code(code: &str) -> Option<&'static str> {
    // Map only equivalent events. In particular, the old generic
    // `unexpected-end-tag` covers several distinct recovery branches.
    // html5lib's adoption-agency 1.2/1.3 names identify the same specific
    // branches as `formatting-element-not-open` and
    // `misnested-formatting-end-tag`, respectively.
    match code {
        "expected-doctype-but-got-start-tag"
        | "expected-doctype-but-got-chars"
        | "expected-doctype-but-got-eof" => Some("missing-doctype"),
        "unexpected-end-tag-before-html" => Some("unmatched-end-tag"),
        "named-entity-without-semicolon" => Some("missing-semicolon-after-character-reference"),
        "unexpected-eof-in-text-mode" => Some("eof-in-text"),
        "expected-closing-tag-but-got-eof" | "expected-named-closing-tag-but-got-eof" => {
            Some("unclosed-elements-at-eof")
        }
        "end-tag-too-early" => Some("end-tag-too-early"),
        "eof-in-table" => Some("eof-in-table"),
        "eof-in-frameset" => Some("eof-in-frameset"),
        "image-start-tag" => Some("image-start-tag"),
        "unexpected-doctype" => Some("unexpected-doctype"),
        "foster-parenting-start-tag" => Some("foster-parenting-start-tag"),
        "foster-parenting-end-tag" => Some("foster-parenting-end-tag"),
        "foster-parenting-character-in-table" => Some("foster-parenting-character-in-table"),
        "formatting-element-not-in-scope" => Some("formatting-element-not-in-scope"),
        "unexpected-null-character" => Some("unexpected-null-character"),
        "adoption-agency-1.2" => Some("formatting-element-not-open"),
        "adoption-agency-1.3" => Some("misnested-formatting-end-tag"),
        "adoption-agency-4.4" => Some("formatting-element-not-in-scope"),
        "unexpected-cell-in-table-body" => Some("cell-start-tag-in-table-body"),
        "unexpected-end-tag-in-table-body" => Some("unexpected-end-tag-in-table-body"),
        "unexpected-end-tag-in-table-row" => Some("unexpected-end-tag-in-row"),
        "unexpected-html-element-in-foreign-content" => Some("html-start-tag-in-foreign-content"),
        "unexpected-char-in-frameset" => Some("unexpected-character-in-frameset"),
        "unexpected-char-after-frameset" => Some("unexpected-character-after-frameset"),
        "unexpected-form-in-table" => Some("form-start-tag-in-table"),
        "unexpected-hidden-input-in-table" => Some("hidden-input-in-table"),
        "unexpected-start-tag-implies-end-tag" => Some("start-tag-implies-end-tag"),
        "eof-in-template" => Some("eof-in-template"),
        _ => None,
    }
}

fn parse_expected(input: &str) -> Option<Vec<(&'static str, usize, usize)>> {
    // WPT's legacy (line,column) coordinates are one-based. Its files are
    // CRLF-normalized in tree_cases, matching tokenizer SourcePosition.
    input
        .lines()
        .take_while(|line| !line.starts_with('#'))
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            let (pos, code) = line.split_once("): ")?;
            let (line_no, col) = pos.strip_prefix('(')?.split_once(',')?;
            Some((
                historical_code(code.trim())?,
                line_no.trim().parse().ok()?,
                col.trim().parse().ok()?,
            ))
        })
        .collect()
}

#[test]
fn wpt_tree_error_inventory() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/upstream/tree");
    let mut files: Vec<_> = fs::read_dir(root)
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect();
    files.sort();
    let mut docs = (0, 0, 0);
    let mut fragments = (0, 0, 0);
    let mut scripted = (0, 0, 0);
    for path in files {
        let mut document_counts = (0, 0, 0);
        let mut fragment_counts = (0, 0, 0);
        let mut scripted_counts = (0, 0, 0);
        for (input, expected, fragment, script_on) in
            tree_cases(&fs::read_to_string(&path).unwrap())
        {
            let counts = if fragment.is_some() {
                &mut fragment_counts
            } else if script_on {
                &mut scripted_counts
            } else {
                &mut document_counts
            };
            let totals = if fragment.is_some() {
                &mut fragments
            } else if script_on {
                &mut scripted
            } else {
                &mut docs
            };
            // The legacy #errors list remains independently comparable when
            // a fixture also carries a current-standard #new-errors block.
            // Prose-only locations and non-equivalent historical names are U.
            let Some(expected) = parse_expected(&expected) else {
                counts.2 += 1;
                totals.2 += 1;
                continue;
            };
            let report = if let Some(fragment) = fragment {
                html::parse_fragment_with_errors_and_scripting(
                    &input,
                    &context(&fragment),
                    script_on,
                )
            } else {
                html::parse_with_errors_and_scripting(&input, script_on)
            };
            let actual = report
                .unwrap()
                .errors
                .into_iter()
                .map(|e| (e.code, e.position.line, e.position.column))
                .collect::<Vec<_>>();
            if actual == expected {
                counts.0 += 1;
                totals.0 += 1;
            } else {
                counts.1 += 1;
                totals.1 += 1;
                if std::env::var_os("CORPUS_VERBOSE").is_some() {
                    println!(
                        "TREE ERROR {}: expected={expected:?} actual={actual:?}",
                        path.display()
                    );
                }
            }
        }
        println!(
            "{} tree errors: document={}/{}/{} fragment={}/{}/{} script-on={}/{}/{}",
            path.file_name().unwrap().to_string_lossy(),
            document_counts.0,
            document_counts.1,
            document_counts.2,
            fragment_counts.0,
            fragment_counts.1,
            fragment_counts.2,
            scripted_counts.0,
            scripted_counts.1,
            scripted_counts.2
        );
    }
    println!(
        "DOCUMENT ERRORS TOTAL: passed={} failed={} unrepresentable={}",
        docs.0, docs.1, docs.2
    );
    println!(
        "FRAGMENT ERRORS TOTAL: passed={} failed={} unrepresentable={}",
        fragments.0, fragments.1, fragments.2
    );
    println!(
        "SCRIPT-ON ERRORS TOTAL: passed={} failed={} unrepresentable={}",
        scripted.0, scripted.1, scripted.2
    );
    assert_eq!(docs.0 + docs.1 + docs.2, 1739);
    assert_eq!(fragments.0 + fragments.1 + fragments.2, 206);
    assert_eq!(scripted.0 + scripted.1 + scripted.2, 8);
}
