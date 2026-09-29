use phos::html::tokenizer::{Token, Tokenizer};
use serde_json::Value;
use std::fs;
use std::path::Path;

fn normalized_tokens(input: &str, state: &str, last_tag: &str, legacy: bool) -> Vec<Value> {
    let mut tokenizer = if legacy {
        Tokenizer::new_legacy_html5lib(input)
    } else {
        Tokenizer::new(input)
    };
    match state {
        "Data state" => {}
        "PLAINTEXT state" => tokenizer.enter_plaintext(),
        "RCDATA state" => tokenizer.enter_rcdata(last_tag),
        "RAWTEXT state" => tokenizer.enter_rawtext(last_tag),
        "Script data state" => tokenizer.enter_script_data(last_tag),
        "CDATA section state" => tokenizer.enter_cdata(),
        other => panic!("unknown tokenizer state: {other}"),
    }
    tokenizer.set_last_start_tag(last_tag);
    let mut result: Vec<Value> = Vec::new();
    while let Some(token) = tokenizer.next_token() {
        let value = match token {
            Token::Doctype(doctype) => serde_json::json!([
                "DOCTYPE",
                doctype.name,
                doctype.public_id,
                doctype.system_id,
                !doctype.force_quirks
            ]),
            Token::StartTag(tag) => {
                let attributes: serde_json::Map<String, Value> = tag
                    .attributes
                    .into_iter()
                    .map(|attribute| (attribute.name, Value::String(attribute.value)))
                    .collect();
                if tag.self_closing {
                    serde_json::json!(["StartTag", tag.name, attributes, true])
                } else {
                    serde_json::json!(["StartTag", tag.name, attributes])
                }
            }
            Token::EndTag(tag) => serde_json::json!(["EndTag", tag.name]),
            Token::Comment(data) => serde_json::json!(["Comment", data]),
            Token::ProcessingInstruction { target, data } => {
                serde_json::json!(["ProcessingInstruction", target, data])
            }
            Token::Character(data) => {
                if let Some(last) = result.last_mut()
                    && last[0] == "Character"
                {
                    let mut joined = last[1].as_str().unwrap().to_owned();
                    joined.push_str(&data);
                    last[1] = Value::String(joined);
                    continue;
                }
                serde_json::json!(["Character", data])
            }
            Token::Eof => break,
        };
        result.push(value);
    }
    result
}

fn test_states(case: &Value) -> Vec<&str> {
    case.get("initialStates")
        .and_then(Value::as_array)
        .map(|states| states.iter().map(|state| state.as_str().unwrap()).collect())
        .unwrap_or_else(|| vec!["Data state"])
}

fn decode_double_escaped(input: &str) -> Option<String> {
    let mut chars = input.chars().peekable();
    let mut decoded = String::new();
    while let Some(ch) = chars.next() {
        if ch == '\\' && chars.peek() == Some(&'u') {
            chars.next();
            let hex: String = chars.by_ref().take(4).collect();
            let codepoint = u32::from_str_radix(&hex, 16).ok()?;
            decoded.push(char::from_u32(codepoint)?);
        } else {
            decoded.push(ch);
        }
    }
    Some(decoded)
}

fn decode_value(value: &Value) -> Option<Value> {
    match value {
        Value::String(text) => Some(Value::String(decode_double_escaped(text)?)),
        Value::Array(items) => Some(Value::Array(
            items.iter().map(decode_value).collect::<Option<_>>()?,
        )),
        Value::Object(items) => Some(Value::Object(
            items
                .iter()
                .map(|(key, value)| Some((key.clone(), decode_value(value)?)))
                .collect::<Option<_>>()?,
        )),
        other => Some(other.clone()),
    }
}

fn representable_case(case: &Value) -> Option<(String, Vec<Value>)> {
    let input = case["input"].as_str()?;
    let output = case["output"].as_array()?;
    if case["doubleEscaped"] == true {
        Some((
            decode_double_escaped(input)?,
            output.iter().map(decode_value).collect::<Option<_>>()?,
        ))
    } else {
        Some((input.to_owned(), output.clone()))
    }
}

fn current_pi_expectation(file: &str, case_number: usize) -> Option<Vec<Value>> {
    match (file, case_number) {
        ("test2.test", 32) => Some(vec![serde_json::json!([
            "ProcessingInstruction",
            "namespace",
            ""
        ])]),
        ("test2.test", 33) => Some(vec![serde_json::json!([
            "ProcessingInstruction",
            "foo--",
            ""
        ])]),
        ("test3.test", 1159 | 1181 | 1182 | 1183 | 1184 | 1186 | 1187 | 1188 | 1189) => {
            Some(Vec::new())
        }
        _ => None,
    }
}

#[test]
fn upstream_tokenizer_corpus_current() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/upstream/tokenizer");
    let mut files: Vec<_> = fs::read_dir(root)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect();
    files.sort();
    assert_eq!(files.len(), 13, "tokenizer fixture files are missing");
    let mut checked = 0;
    let mut historical_pi_cases = 0;
    let mut unrepresentable = 0;
    for path in files {
        let file = path.file_name().unwrap().to_str().unwrap();
        let fixture: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        for (index, case) in fixture["tests"].as_array().unwrap().iter().enumerate() {
            let Some((input, upstream_expected)) = representable_case(case) else {
                unrepresentable += test_states(case).len();
                continue;
            };
            let current_expected = current_pi_expectation(file, index + 1);
            if current_expected.is_some() {
                historical_pi_cases += test_states(case).len();
            }
            let expected = current_expected.unwrap_or(upstream_expected);
            let last_tag = case
                .get("lastStartTag")
                .and_then(Value::as_str)
                .unwrap_or("");
            for state in test_states(case) {
                assert_eq!(
                    normalized_tokens(&input, state, last_tag, false),
                    expected,
                    "{file} case #{} {:?} in {state}",
                    index + 1,
                    case["description"]
                );
                checked += 1;
            }
        }
    }
    assert_eq!(checked, 7028, "tokenizer fixture cases are missing");
    assert_eq!(historical_pi_cases, 11);
    assert_eq!(unrepresentable, 4);
}

#[test]
fn upstream_tokenizer_corpus_legacy() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/upstream/tokenizer");
    let mut files: Vec<_> = fs::read_dir(root)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect();
    files.sort();
    assert_eq!(files.len(), 13, "tokenizer fixture files are missing");
    let mut checked = 0;
    let mut unrepresentable = 0;
    for path in files {
        let fixture: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        for (index, case) in fixture["tests"].as_array().unwrap().iter().enumerate() {
            // Unpaired UTF-16 surrogates cannot be represented by Rust's UTF-8 str input API.
            let Some((input, expected)) = representable_case(case) else {
                unrepresentable += test_states(case).len();
                continue;
            };
            let last_tag = case
                .get("lastStartTag")
                .and_then(Value::as_str)
                .unwrap_or("");
            for state in test_states(case) {
                assert_eq!(
                    normalized_tokens(&input, state, last_tag, true),
                    expected,
                    "{} case #{} {:?} in {state}",
                    path.file_name().unwrap().to_string_lossy(),
                    index + 1,
                    case["description"]
                );
                checked += 1;
            }
        }
    }
    assert_eq!(checked, 7028, "tokenizer fixture cases are missing");
    assert_eq!(unrepresentable, 4, "unexpected unrepresentable cases");
}

#[test]
#[ignore = "diagnostic: run with --ignored --nocapture to print the full tokenizer inventory"]
fn inventory_upstream_tokenizer() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/upstream/tokenizer");
    let mut files: Vec<_> = fs::read_dir(root)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect();
    files.sort();
    let mut totals = (0, 0, 0);
    for path in files {
        let fixture: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        let mut passed = 0;
        let mut failed = 0;
        let mut skipped = 0;
        let legacy = std::env::var_os("CORPUS_LEGACY").is_some();
        for (index, case) in fixture["tests"].as_array().unwrap().iter().enumerate() {
            let Some((input, expected)) = representable_case(case) else {
                skipped += test_states(case).len();
                continue;
            };
            let last_tag = case
                .get("lastStartTag")
                .and_then(Value::as_str)
                .unwrap_or("");
            for state in test_states(case) {
                let actual = normalized_tokens(&input, state, last_tag, legacy);
                if actual == expected {
                    passed += 1;
                } else {
                    failed += 1;
                    if failed <= 3 || std::env::var_os("CORPUS_VERBOSE").is_some() {
                        println!(
                            "  FAIL #{} {state} {:?}: expected={expected:?} actual={actual:?}",
                            index + 1,
                            case["description"]
                        );
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
