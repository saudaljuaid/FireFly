use crate::dom::{Document, Element, NodeId};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Declaration {
    pub name: String,
    pub value: String,
    pub important: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompoundSelector {
    pub tag: Option<String>,
    pub id: Option<String>,
    pub classes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Selector {
    pub parts: Vec<CompoundSelector>,
    pub specificity: (usize, usize, usize),
}

impl Selector {
    pub fn matches(&self, document: &Document, node: NodeId) -> bool {
        let Some(last) = self.parts.last() else {
            return false;
        };
        if !document
            .element(node)
            .is_some_and(|element| last.matches(element))
        {
            return false;
        }
        let mut ancestor = document.nodes[node].parent;
        for part in self.parts[..self.parts.len() - 1].iter().rev() {
            let mut found = None;
            while let Some(id) = ancestor {
                if document
                    .element(id)
                    .is_some_and(|element| part.matches(element))
                {
                    found = Some(id);
                    break;
                }
                ancestor = document.nodes[id].parent;
            }
            let Some(id) = found else {
                return false;
            };
            ancestor = document.nodes[id].parent;
        }
        true
    }
}

impl CompoundSelector {
    fn matches(&self, element: &Element) -> bool {
        self.tag.as_ref().is_none_or(|tag| tag == &element.tag)
            && self
                .id
                .as_deref()
                .is_none_or(|id| element.attribute("id") == Some(id))
            && self.classes.iter().all(|class| element.has_class(class))
    }
}

#[derive(Debug, Clone)]
pub struct Rule {
    pub selectors: Vec<Selector>,
    pub declarations: Vec<Declaration>,
}

#[derive(Debug, Clone, Default)]
pub struct Stylesheet {
    pub rules: Vec<Rule>,
}

// A small CSS scanner, intentionally bounded rather than a general CSS tokenizer.
// Comments become whitespace so `red/*x*/blue` cannot turn into `redblue`.
fn strip_comments(source: &str) -> String {
    let mut output = String::with_capacity(source.len());
    let mut chars = source.chars().peekable();
    let mut quote = None;
    let mut escaped = false;
    while let Some(ch) = chars.next() {
        if escaped {
            output.push(ch);
            escaped = false;
        } else if ch == '\\' {
            output.push(ch);
            escaped = true;
        } else if Some(ch) == quote || (quote.is_some() && ch == '\n') {
            output.push(ch);
            quote = None;
        } else if quote.is_none() && matches!(ch, '\'' | '"') {
            output.push(ch);
            quote = Some(ch);
        } else if quote.is_none() && ch == '/' && chars.peek() == Some(&'*') {
            chars.next();
            output.push(' ');
            let mut previous = '\0';
            for next in chars.by_ref() {
                if previous == '*' && next == '/' {
                    break;
                }
                previous = next;
            }
        } else {
            output.push(ch);
        }
    }
    output
}

fn parse_declaration(part: &str) -> Option<Declaration> {
    let (name, value) = part.split_once(':')?;
    let name = name.trim().to_ascii_lowercase();
    if name.is_empty()
        || name.len() > 128
        || !name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
    {
        return None;
    }
    let mut value = value.trim();
    if value.is_empty() || value.len() > 65_536 {
        return None;
    }
    // Only a trailing top-level !important flag affects the cascade.
    let mut important = false;
    let lower = value.to_ascii_lowercase();
    if let Some(prefix) = lower.strip_suffix("important") {
        let trimmed = prefix.trim_end();
        if let Some(before) = trimmed.strip_suffix('!') {
            value = value[..before.len()].trim_end();
            important = true;
        }
    }
    if value.is_empty() {
        return None;
    }
    Some(Declaration {
        name,
        value: value.to_string(),
        important,
    })
}

pub fn parse_declarations(source: &str) -> Vec<Declaration> {
    let clean = strip_comments(source);
    let mut result = Vec::new();
    let mut start = 0;
    let mut quote = None;
    let mut escaped = false;
    let mut depth = 0usize;
    let mut invalid = false;
    for (index, ch) in clean.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        if ch == '\\' {
            escaped = true;
            continue;
        }
        if Some(ch) == quote {
            quote = None;
            continue;
        }
        if quote.is_some() && ch == '\n' {
            quote = None;
            invalid = true;
            continue;
        }
        if quote.is_some() {
            continue;
        }
        match ch {
            '\'' | '"' => quote = Some(ch),
            '(' => depth = depth.saturating_add(1).min(64),
            ')' => depth = depth.saturating_sub(1),
            ';' if depth == 0 => {
                if !invalid && let Some(decl) = parse_declaration(&clean[start..index]) {
                    result.push(decl);
                    if result.len() == 8192 {
                        return result;
                    }
                }
                start = index + 1;
                invalid = false;
            }
            _ => {}
        }
    }
    if !invalid
        && quote.is_none()
        && depth == 0
        && let Some(decl) = parse_declaration(&clean[start..])
    {
        result.push(decl);
    }
    result
}

fn parse_compound(source: &str) -> Option<(CompoundSelector, (usize, usize, usize))> {
    let mut result = CompoundSelector {
        tag: None,
        id: None,
        classes: Vec::new(),
    };
    let mut specificity = (0, 0, 0);
    let mut chars = source.chars().peekable();
    if chars.peek() == Some(&'*') {
        chars.next();
    } else if chars
        .peek()
        .is_some_and(|character| character.is_ascii_alphabetic())
    {
        let mut tag = String::new();
        while chars
            .peek()
            .is_some_and(|character| character.is_ascii_alphanumeric() || *character == '-')
        {
            tag.push(chars.next().unwrap());
        }
        result.tag = Some(tag.to_ascii_lowercase());
        specificity.2 += 1;
    }
    while let Some(marker) = chars.next() {
        if marker != '.' && marker != '#' {
            return None;
        }
        let mut name = String::new();
        while chars.peek().is_some_and(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '-' | '_')
        }) {
            name.push(chars.next().unwrap());
        }
        if name.is_empty() {
            return None;
        }
        if marker == '#' {
            result.id = Some(name);
            specificity.0 += 1;
        } else {
            result.classes.push(name);
            specificity.1 += 1;
        }
    }
    Some((result, specificity))
}

fn parse_selector(source: &str) -> Option<Selector> {
    let mut parts = Vec::new();
    let mut specificity = (0, 0, 0);
    for word in source.split_ascii_whitespace() {
        let (part, score) = parse_compound(word)?;
        specificity.0 += score.0;
        specificity.1 += score.1;
        specificity.2 += score.2;
        parts.push(part);
    }
    if parts.is_empty() {
        None
    } else {
        Some(Selector { parts, specificity })
    }
}

pub fn parse(source: &str) -> Stylesheet {
    let clean = strip_comments(source);
    let mut remaining = clean.as_str();
    let mut rules = Vec::new();
    loop {
        remaining = remaining.trim_start();
        if remaining.is_empty() {
            break;
        }
        let Some(open) = remaining.find('{') else {
            break;
        };
        if remaining.starts_with('@')
            && let Some(semicolon) = remaining.find(';')
            && semicolon < open
        {
            remaining = &remaining[semicolon + 1..];
            continue;
        }
        let Some(close) = matching_brace(remaining, open) else {
            break;
        };
        let selector_text = remaining[..open].trim();
        let body = &remaining[open + 1..close];
        if !selector_text.starts_with('@') {
            let selectors = selector_text
                .split(',')
                .filter_map(parse_selector)
                .collect::<Vec<_>>();
            let declarations = parse_declarations(body);
            if !selectors.is_empty() && !declarations.is_empty() {
                rules.push(Rule {
                    selectors,
                    declarations,
                });
            }
        }
        remaining = &remaining[close + 1..];
    }
    Stylesheet { rules }
}

fn matching_brace(source: &str, open: usize) -> Option<usize> {
    let mut depth = 1;
    let mut quote = None;
    let mut escaped = false;
    let mut candidate = None;
    for (offset, character) in source[open + 1..].char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        if quote.is_some() && character == '\\' {
            escaped = true;
            continue;
        }
        if Some(character) == quote {
            quote = None;
            candidate = None;
            continue;
        }
        if quote.is_some() {
            if character == '}' && depth == 1 && candidate.is_none() {
                candidate = Some(open + 1 + offset);
            } else if character == '\n' {
                quote = None;
                if candidate.is_some() {
                    return candidate;
                }
            }
            continue;
        }
        if quote.is_none() && matches!(character, '\'' | '"') {
            quote = Some(character);
            continue;
        }
        if quote.is_none() {
            match character {
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        return Some(open + 1 + offset);
                    }
                }
                _ => {}
            }
        }
    }
    candidate
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dom::NodeKind;
    use crate::html;

    #[test]
    fn matches_descendant_and_compound_selectors() {
        let document =
            html::parse("<section id='app'><p class='lead hot'>Hello</p></section>").unwrap();
        let sheet = parse("section#app p.lead.hot { color: red; }");
        let paragraph = document
            .nodes
            .iter()
            .position(|node| matches!(&node.kind, NodeKind::Element(element) if element.tag == "p"))
            .unwrap();
        assert!(sheet.rules[0].selectors[0].matches(&document, paragraph));
        assert_eq!(sheet.rules[0].selectors[0].specificity, (1, 2, 2));
    }

    #[test]
    fn skips_comments_and_unsupported_selectors() {
        let sheet = parse("/* note */ p:hover, .ok { color: blue; }");
        assert_eq!(sheet.rules[0].selectors.len(), 1);
    }

    #[test]
    fn does_not_apply_rules_inside_unsupported_at_rules() {
        let sheet = parse(
            "@charset \"utf-8\"; @media print { h1 { color: red } p { color: blue } } h1 { color: green }",
        );
        assert_eq!(sheet.rules.len(), 1);
        assert_eq!(sheet.rules[0].declarations[0].value, "green");
    }

    #[test]
    fn declaration_scanner_keeps_values_and_recovers_after_bad_parts() {
        let declarations = parse_declarations(
            r#"bad; color: RED; data: url("A;B(C)"); broken: ; background-color: #AbCdEf ! IMPORTANT; width: 2px"#,
        );
        assert_eq!(declarations.len(), 4);
        assert_eq!(declarations[0].value, "RED");
        assert_eq!(declarations[1].value, "url(\"A;B(C)\")");
        assert_eq!(declarations[2].value, "#AbCdEf");
        assert!(declarations[2].important);
        assert_eq!(declarations[3].name, "width");
        let sheet =
            parse(r#"p { color: red; content: "/* literal */;}"; } /* x */ p { color: blue }"#);
        assert_eq!(sheet.rules.len(), 2);
        assert_eq!(sheet.rules[1].declarations[0].value, "blue");
        let escaped = parse_declarations(r"content: A\;B; color: green");
        assert_eq!(escaped.len(), 2);
        assert_eq!(escaped[0].value, r"A\;B");
        let recovered = parse("p{color:'broken; } div{color:blue}");
        assert_eq!(recovered.rules.len(), 1);
        assert_eq!(
            recovered.rules[0].selectors[0].parts[0].tag.as_deref(),
            Some("div")
        );
    }
}
