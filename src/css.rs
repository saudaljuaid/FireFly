use crate::dom::{Document, Element, NodeId};

pub const MAX_CSS_BYTES: usize = 4 * 1024 * 1024;
pub const MAX_RULES: usize = 8192;
pub const MAX_TOTAL_DECLARATIONS: usize = 65_536;
pub const MAX_SELECTORS_PER_RULE: usize = 128;
pub const MAX_SELECTOR_PARTS: usize = 32;
pub const MAX_MEDIA_NESTING: usize = 8;
pub const MAX_MEDIA_ALTERNATIVES: usize = 16;
pub const MAX_MEDIA_FEATURES: usize = 16;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum WidthComparison {
    Less,
    LessEqual,
    Equal,
    GreaterEqual,
    Greater,
}

impl WidthComparison {
    fn matches(self, left: f32, right: f32) -> bool {
        match self {
            Self::Less => left < right,
            Self::LessEqual => left <= right,
            Self::Equal => left == right,
            Self::GreaterEqual => left >= right,
            Self::Greater => left > right,
        }
    }

    fn reverse(self) -> Self {
        match self {
            Self::Less => Self::Greater,
            Self::LessEqual => Self::GreaterEqual,
            Self::Equal => Self::Equal,
            Self::GreaterEqual => Self::LessEqual,
            Self::Greater => Self::Less,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WidthConstraint {
    pub comparison: WidthComparison,
    pub pixels: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MediaQuery {
    pub constraints: Vec<WidthConstraint>,
    pub supported: bool,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct MediaQueryList {
    pub alternatives: Vec<MediaQuery>,
}

impl MediaQueryList {
    pub fn matches(&self, viewport_width: f32) -> bool {
        viewport_width.is_finite()
            && self.alternatives.iter().any(|query| {
                query.supported
                    && query.constraints.iter().all(|constraint| {
                        constraint
                            .comparison
                            .matches(viewport_width, constraint.pixels)
                    })
            })
    }
}

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

// Matching charges both structural visits and bytes inspected. A caller can share
// this counter across the cascade so a large attribute or deep ancestor chain
// cannot multiply the stylesheet bounds into unbounded work.
fn charge(remaining: &mut usize, cost: usize) -> Option<()> {
    if cost > *remaining {
        *remaining = 0;
        None
    } else {
        *remaining -= cost;
        Some(())
    }
}

impl Selector {
    pub fn matches(&self, document: &Document, node: NodeId) -> bool {
        let mut remaining = usize::MAX;
        self.matches_bounded(document, node, &mut remaining)
            .unwrap_or(false)
    }

    /// `None` means the shared matching work budget was exhausted. Attribute
    /// name/value comparisons and class-token scanning are charged in bytes;
    /// each visited node and compound selector costs at least one unit.
    pub fn matches_bounded(
        &self,
        document: &Document,
        node: NodeId,
        remaining: &mut usize,
    ) -> Option<bool> {
        charge(remaining, 1)?;
        let Some(last) = self.parts.last() else {
            return Some(false);
        };
        let Some(element) = document.element(node) else {
            return Some(false);
        };
        if !last.matches_bounded(element, remaining)? {
            return Some(false);
        }
        let mut ancestor = document.nodes[node].parent;
        for part in self.parts[..self.parts.len() - 1].iter().rev() {
            let mut found = None;
            while let Some(id) = ancestor {
                charge(remaining, 1)?;
                if let Some(element) = document.element(id)
                    && part.matches_bounded(element, remaining)?
                {
                    found = Some(id);
                    break;
                }
                ancestor = document.nodes[id].parent;
            }
            let Some(id) = found else {
                return Some(false);
            };
            ancestor = document.nodes[id].parent;
        }
        Some(true)
    }
}

fn bounded_attribute<'a>(
    element: &'a Element,
    name: &str,
    remaining: &mut usize,
) -> Option<Option<&'a str>> {
    for attribute in &element.attributes {
        charge(remaining, 1)?;
        charge(remaining, attribute.name.len())?;
        if attribute.name == name {
            return Some(Some(&attribute.value));
        }
    }
    Some(None)
}

impl CompoundSelector {
    fn matches_bounded(&self, element: &Element, remaining: &mut usize) -> Option<bool> {
        charge(remaining, 1)?;
        if let Some(tag) = &self.tag {
            charge(remaining, element.tag.len().max(tag.len()))?;
            if tag != &element.tag {
                return Some(false);
            }
        }
        if let Some(id) = &self.id {
            let Some(value) = bounded_attribute(element, "id", remaining)? else {
                return Some(false);
            };
            charge(remaining, value.len().max(id.len()))?;
            if value != id {
                return Some(false);
            }
        }
        for class in &self.classes {
            let Some(value) = bounded_attribute(element, "class", remaining)? else {
                return Some(false);
            };
            // Precharge the complete scan. This conservative accounting also
            // covers token comparisons and bounds a hostile giant attribute.
            charge(remaining, value.len())?;
            charge(remaining, class.len())?;
            if !value.split_ascii_whitespace().any(|item| item == class) {
                return Some(false);
            }
        }
        Some(true)
    }
}

#[derive(Debug, Clone)]
pub struct Rule {
    pub selectors: Vec<Selector>,
    pub declarations: Vec<Declaration>,
    /// Nested media lists are conjoined. Rules remain in source order and use
    /// the ordinary author cascade after this condition is evaluated.
    pub media: Vec<MediaQueryList>,
}

impl Rule {
    pub fn is_active(&self, viewport_width: f32) -> bool {
        self.media.iter().all(|list| list.matches(viewport_width))
    }
}

#[derive(Debug, Clone, Default)]
pub struct Stylesheet {
    pub rules: Vec<Rule>,
    /// A parser work/input limit omitted otherwise parseable stylesheet data.
    pub truncated: bool,
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
    parse_declarations_with_status(source).0
}

pub fn parse_declarations_with_status(source: &str) -> (Vec<Declaration>, bool) {
    let mut end = source.len().min(MAX_CSS_BYTES);
    while !source.is_char_boundary(end) {
        end -= 1;
    }
    let clean = strip_comments(&source[..end]);
    let mut truncated = end != source.len();
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
            '(' => {
                depth += 1;
                invalid |= depth > 64;
            }
            ')' => {
                if depth == 0 {
                    invalid = true;
                } else {
                    depth -= 1;
                }
            }
            ';' if depth == 0 => {
                if !invalid && let Some(decl) = parse_declaration(&clean[start..index]) {
                    result.push(decl);
                    if result.len() == 8192 {
                        truncated |= !clean[index + 1..].trim().is_empty();
                        return (result, truncated);
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
    (result, truncated)
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
            if result.classes.len() > 32 {
                return None;
            }
        }
    }
    Some((result, specificity))
}

fn parse_selector(source: &str) -> Option<Selector> {
    if source.len() > 8192 {
        return None;
    }
    let mut parts = Vec::new();
    let mut specificity = (0, 0, 0);
    for word in source.split_ascii_whitespace() {
        let (part, score) = parse_compound(word)?;
        specificity.0 += score.0;
        specificity.1 += score.1;
        specificity.2 += score.2;
        parts.push(part);
        if parts.len() > MAX_SELECTOR_PARTS {
            return None;
        }
    }
    if parts.is_empty() {
        None
    } else {
        Some(Selector { parts, specificity })
    }
}

pub fn parse(source: &str) -> Stylesheet {
    let mut end = source.len().min(MAX_CSS_BYTES);
    while !source.is_char_boundary(end) {
        end -= 1;
    }
    let clean = strip_comments(&source[..end]);
    let mut sheet = Stylesheet {
        rules: Vec::new(),
        truncated: end != source.len(),
    };
    let mut declarations = 0;
    parse_rules(&clean, &[], &mut sheet, &mut declarations);
    sheet
}

fn parse_rules(
    clean: &str,
    media: &[MediaQueryList],
    sheet: &mut Stylesheet,
    declaration_count: &mut usize,
) {
    let mut remaining = clean;
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
        if selector_text
            .get(..6)
            .is_some_and(|prefix| prefix.eq_ignore_ascii_case("@media"))
            && selector_text
                .as_bytes()
                .get(6)
                .is_none_or(|byte| byte.is_ascii_whitespace() || *byte == b'(')
        {
            if media.len() == MAX_MEDIA_NESTING {
                sheet.truncated = true;
            } else {
                let mut conditions = media.to_vec();
                conditions.push(parse_media_query_list(&selector_text[6..]));
                parse_rules(body, &conditions, sheet, declaration_count);
            }
        } else if !selector_text.starts_with('@') {
            let mut selectors = Vec::new();
            for selector in selector_text.split(',') {
                if selectors.len() == MAX_SELECTORS_PER_RULE {
                    sheet.truncated = true;
                    break;
                }
                if selector.len() > 8192
                    || selector.split_ascii_whitespace().take(33).count() > MAX_SELECTOR_PARTS
                    || selector
                        .split_ascii_whitespace()
                        .any(|part| part.bytes().filter(|byte| *byte == b'.').take(33).count() > 32)
                {
                    sheet.truncated = true;
                    continue;
                }
                if let Some(selector) = parse_selector(selector) {
                    selectors.push(selector);
                }
            }
            let (declarations, truncated) = parse_declarations_with_status(body);
            sheet.truncated |= truncated;
            if !selectors.is_empty() && !declarations.is_empty() {
                if sheet.rules.len() == MAX_RULES
                    || declaration_count.saturating_add(declarations.len()) > MAX_TOTAL_DECLARATIONS
                {
                    sheet.truncated = true;
                    return;
                }
                *declaration_count += declarations.len();
                sheet.rules.push(Rule {
                    selectors,
                    declarations,
                    media: media.to_vec(),
                });
            }
        }
        remaining = &remaining[close + 1..];
    }
}

fn media_length(value: &str) -> Option<f32> {
    // MQ font-relative units use the initial 16px font, independently of the
    // document cascade. Viewport units, percentages, and calculations are
    // deliberately outside this media-value subset.
    let lower = value.trim().to_ascii_lowercase();
    let (number, factor) = if let Some(number) = lower.strip_suffix("rem") {
        (number, 16.0)
    } else if let Some(number) = lower.strip_suffix("em") {
        (number, 16.0)
    } else if let Some(number) = lower.strip_suffix("px") {
        (number, 1.0)
    } else if lower == "0" {
        return Some(0.0);
    } else {
        return None;
    };
    if number.is_empty() || number.bytes().any(|byte| byte.is_ascii_whitespace()) {
        return None;
    }
    let number: f32 = number.parse().ok()?;
    (number.is_finite() && (0.0..=16_384.0).contains(&number)).then_some(number * factor)
}

fn media_feature(value: &str) -> Option<Vec<WidthConstraint>> {
    if let Some((name, value)) = value.split_once(':') {
        let comparison = match name.trim() {
            "min-width" => WidthComparison::GreaterEqual,
            "max-width" => WidthComparison::LessEqual,
            "width" => WidthComparison::Equal,
            _ => return None,
        };
        return Some(vec![WidthConstraint {
            comparison,
            pixels: media_length(value)?,
        }]);
    }
    let mut operands = Vec::new();
    let mut operators = Vec::new();
    let mut start = 0;
    let bytes = value.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if !matches!(bytes[index], b'<' | b'>' | b'=') {
            index += 1;
            continue;
        }
        operands.push(value[start..index].trim());
        let first = bytes[index];
        index += 1;
        let equal = first != b'=' && bytes.get(index) == Some(&b'=');
        if equal {
            index += 1;
        }
        operators.push(match (first, equal) {
            (b'<', false) => WidthComparison::Less,
            (b'<', true) => WidthComparison::LessEqual,
            (b'>', false) => WidthComparison::Greater,
            (b'>', true) => WidthComparison::GreaterEqual,
            _ => WidthComparison::Equal,
        });
        if operators.len() > 2 {
            return None;
        }
        start = index;
    }
    operands.push(value[start..].trim());
    match operands.as_slice() {
        ["width", right] => Some(vec![WidthConstraint {
            comparison: *operators.first()?,
            pixels: media_length(right)?,
        }]),
        [left, "width"] => Some(vec![WidthConstraint {
            comparison: operators.first()?.reverse(),
            pixels: media_length(left)?,
        }]),
        [left, "width", right] => {
            // A chained CSS range must use one direction and no equality-only
            // operator; rejecting contradictory syntax preserves browser rules.
            let first = *operators.first()?;
            let second = *operators.get(1)?;
            let less =
                |operator| matches!(operator, WidthComparison::Less | WidthComparison::LessEqual);
            let greater = |operator| {
                matches!(
                    operator,
                    WidthComparison::Greater | WidthComparison::GreaterEqual
                )
            };
            if !(less(first) && less(second) || greater(first) && greater(second)) {
                return None;
            }
            Some(vec![
                WidthConstraint {
                    comparison: first.reverse(),
                    pixels: media_length(left)?,
                },
                WidthConstraint {
                    comparison: second,
                    pixels: media_length(right)?,
                },
            ])
        }
        _ => None,
    }
}

fn media_query(source: &str) -> Option<MediaQuery> {
    let lower = source.trim().to_ascii_lowercase();
    let mut remaining = lower.as_str();
    let mut constraints = Vec::new();
    if remaining.is_empty() {
        return None;
    }
    if !remaining.starts_with('(') {
        let end = remaining
            .find(char::is_whitespace)
            .unwrap_or(remaining.len());
        let mut media_type = &remaining[..end];
        remaining = remaining[end..].trim_start();
        if media_type == "only" {
            let end = remaining
                .find(char::is_whitespace)
                .unwrap_or(remaining.len());
            media_type = &remaining[..end];
            remaining = remaining[end..].trim_start();
        }
        if !matches!(media_type, "all" | "screen") {
            return None;
        }
        if remaining.is_empty() {
            return Some(MediaQuery {
                constraints,
                supported: true,
            });
        }
        remaining = remaining.strip_prefix("and")?;
        if !remaining.as_bytes().first()?.is_ascii_whitespace() {
            return None;
        }
        remaining = remaining.trim_start();
    }
    let mut feature_count = 0;
    loop {
        remaining = remaining.strip_prefix('(')?;
        let close = remaining.find(')')?;
        let feature = &remaining[..close];
        if feature.contains('(') {
            return None;
        }
        constraints.extend(media_feature(feature)?);
        feature_count += 1;
        if feature_count > MAX_MEDIA_FEATURES {
            return None;
        }
        remaining = &remaining[close + 1..];
        if remaining.trim().is_empty() {
            return Some(MediaQuery {
                constraints,
                supported: true,
            });
        }
        if !remaining.as_bytes().first()?.is_ascii_whitespace() {
            return None;
        }
        remaining = remaining.trim_start().strip_prefix("and")?;
        if !remaining.as_bytes().first()?.is_ascii_whitespace() {
            return None;
        }
        remaining = remaining.trim_start();
    }
}

pub fn parse_media_query_list(source: &str) -> MediaQueryList {
    if source.trim().is_empty() {
        return MediaQueryList {
            alternatives: vec![MediaQuery {
                constraints: Vec::new(),
                supported: true,
            }],
        };
    }
    if source.len() > 4096 {
        return MediaQueryList::default();
    }
    let Some(parts) = crate::values::comma_components(source) else {
        return MediaQueryList::default();
    };
    if parts.len() > MAX_MEDIA_ALTERNATIVES {
        return MediaQueryList::default();
    }
    MediaQueryList {
        alternatives: parts
            .into_iter()
            .map(|part| {
                media_query(part).unwrap_or(MediaQuery {
                    constraints: Vec::new(),
                    supported: false,
                })
            })
            .collect(),
    }
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
    fn bounded_matching_charges_ancestor_and_attribute_work() {
        let document = html::parse(
            "<section id='app'><div data-note='x'><p class='lead hot'>Hello</p></div></section>",
        )
        .unwrap();
        let node = document
            .nodes
            .iter()
            .position(|node| matches!(&node.kind, NodeKind::Element(element) if element.tag == "p"))
            .unwrap();
        let selector = &parse("section#app p.lead.hot{color:red}").rules[0].selectors[0];
        let mut ample = 1024;
        assert_eq!(
            selector.matches_bounded(&document, node, &mut ample),
            Some(true)
        );
        assert!(ample < 1024);
        let mut limited = 1;
        assert_eq!(
            selector.matches_bounded(&document, node, &mut limited),
            None
        );
        assert_eq!(limited, 0);
        let mut zero = 0;
        assert_eq!(selector.matches_bounded(&document, node, &mut zero), None);

        let source = format!("<p class='{} target'></p>", "x".repeat(100_000));
        let large = html::parse(&source).unwrap();
        let node = large
            .nodes
            .iter()
            .position(|node| matches!(&node.kind, NodeKind::Element(element) if element.tag == "p"))
            .unwrap();
        let selector = &parse("p.target{color:red}").rules[0].selectors[0];
        let mut limited = 100;
        assert_eq!(selector.matches_bounded(&large, node, &mut limited), None);
        assert_eq!(limited, 0);
        assert!(selector.matches(&large, node));
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
        assert_eq!(sheet.rules.len(), 3);
        assert!(!sheet.rules[0].is_active(800.0));
        assert!(!sheet.rules[1].is_active(800.0));
        assert!(sheet.rules[2].is_active(800.0));
        assert_eq!(sheet.rules[2].declarations[0].value, "green");
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

    #[test]
    fn width_media_queries_match_inclusive_and_strict_ranges() {
        for (query, below, boundary, above) in [
            ("(min-width: 600px)", false, true, true),
            ("(max-width: 600px)", true, true, false),
            ("(width: 600px)", false, true, false),
            ("(width > 600px)", false, false, true),
            ("(600px <= width)", false, true, true),
        ] {
            let media = parse_media_query_list(query);
            assert_eq!(media.matches(599.0), below, "{query}");
            assert_eq!(media.matches(600.0), boundary, "{query}");
            assert_eq!(media.matches(601.0), above, "{query}");
        }
        let chained = parse_media_query_list("screen and (400px < width <= 900px)");
        assert!(!chained.matches(400.0));
        assert!(chained.matches(640.0));
        assert!(chained.matches(900.0));
        assert!(!chained.matches(901.0));
        assert!(parse_media_query_list("(900px >= width > 400px)").matches(640.0));
        assert!(!parse_media_query_list("(900px <= width > 400px)").matches(1000.0));
    }

    #[test]
    fn media_alternatives_recover_independently_and_use_initial_font_units() {
        let media = parse_media_query_list(
            "(broken: 1), only screen and (min-width: 40em) and (max-width: 60rem), print",
        );
        assert!(!media.matches(639.0));
        assert!(media.matches(640.0));
        assert!(media.matches(960.0));
        assert!(!media.matches(961.0));
        for invalid in [
            "not screen",
            "(orientation: landscape)",
            "(min-width: -1px)",
            "(width == 600px)",
            "(min-width: NaNpx)",
            "(width > 1e300px)",
            "(min-width: 40vw)",
            "(width >= 600px) or (width < 400px)",
            "screen and(min-width: 600px)",
            "(width > 600px)and (width < 900px)",
            "((width > 600px))",
            "(width > 600px",
            "(min-width: 600 px)",
        ] {
            assert!(!parse_media_query_list(invalid).matches(800.0), "{invalid}");
        }
        assert!(parse_media_query_list("(width == 600px), (min-width: 700px)").matches(800.0));
        assert!(parse_media_query_list("").matches(800.0));
        assert!(!parse_media_query_list("screen").matches(f32::NAN));
    }

    #[test]
    fn nested_media_rules_keep_original_source_order_and_conjoin_conditions() {
        let sheet = parse(
            "p{width:10px}@media(min-width:500px){p{width:20px}@media (max-width:800px){p{width:30px}}p{width:40px}}@supports(display:grid){p{width:50px}}p{width:60px}",
        );
        assert_eq!(sheet.rules.len(), 5);
        assert_eq!(
            sheet
                .rules
                .iter()
                .map(|rule| rule.declarations[0].value.as_str())
                .collect::<Vec<_>>(),
            ["10px", "20px", "30px", "40px", "60px"]
        );
        assert_eq!(
            sheet
                .rules
                .iter()
                .map(|rule| rule.is_active(400.0))
                .collect::<Vec<_>>(),
            [true, false, false, false, true]
        );
        assert_eq!(
            sheet
                .rules
                .iter()
                .map(|rule| rule.is_active(640.0))
                .collect::<Vec<_>>(),
            [true, true, true, true, true]
        );
        assert!(!sheet.rules[2].is_active(900.0));
        assert!(!sheet.truncated);
    }

    #[test]
    fn css_work_bounds_report_truncation_and_invalid_expressions_recover() {
        let sheet = parse(&"p{width:1px}".repeat(MAX_RULES + 1));
        assert_eq!(sheet.rules.len(), MAX_RULES);
        assert!(sheet.truncated);
        let nested = format!(
            "{}p{{width:1px}}{}p{{width:2px}}",
            "@media screen{".repeat(9),
            "}".repeat(9)
        );
        let sheet = parse(&nested);
        assert_eq!(sheet.rules.len(), 1);
        assert_eq!(sheet.rules[0].declarations[0].value, "2px");
        assert!(sheet.truncated);
        let declarations = format!("width:{}1px{};color:blue", "(".repeat(65), ")".repeat(65));
        assert_eq!(
            parse_declarations(&declarations),
            vec![Declaration {
                name: "color".into(),
                value: "blue".into(),
                important: false
            }]
        );
        let (declarations, truncated) = parse_declarations_with_status(&"color:red;".repeat(8193));
        assert_eq!(declarations.len(), 8192);
        assert!(truncated);
        assert!(parse(&format!("{}p{{width:1px}}", "x ".repeat(33))).truncated);
        assert!(!parse_media_query_list(&format!("{}screen", "print,".repeat(16))).matches(800.0));
    }
}
