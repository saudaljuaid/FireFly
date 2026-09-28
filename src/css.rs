use crate::dom::{Document, Element, NodeId};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Declaration {
    pub name: String,
    pub value: String,
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

fn strip_comments(source: &str) -> String {
    let mut output = String::with_capacity(source.len());
    let mut remaining = source;
    while let Some(start) = remaining.find("/*") {
        output.push_str(&remaining[..start]);
        remaining = &remaining[start + 2..];
        if let Some(end) = remaining.find("*/") {
            remaining = &remaining[end + 2..];
        } else {
            return output;
        }
    }
    output.push_str(remaining);
    output
}

pub fn parse_declarations(source: &str) -> Vec<Declaration> {
    source
        .split(';')
        .filter_map(|part| {
            let (name, value) = part.split_once(':')?;
            let name = name.trim().to_ascii_lowercase();
            let value = value.trim().to_ascii_lowercase();
            if name.is_empty() || value.is_empty() {
                None
            } else {
                Some(Declaration { name, value })
            }
        })
        .collect()
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
    while let Some(open) = remaining.find('{') {
        let Some(close) = remaining[open + 1..].find('}') else {
            break;
        };
        let selector_text = remaining[..open].trim();
        let body = &remaining[open + 1..open + 1 + close];
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
        remaining = &remaining[open + 1 + close + 1..];
    }
    Stylesheet { rules }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::html;

    #[test]
    fn matches_descendant_and_compound_selectors() {
        let document =
            html::parse("<section id='app'><p class='lead hot'>Hello</p></section>").unwrap();
        let sheet = parse("section#app p.lead.hot { color: red; }");
        assert!(sheet.rules[0].selectors[0].matches(&document, 2));
        assert_eq!(sheet.rules[0].selectors[0].specificity, (1, 2, 2));
    }

    #[test]
    fn skips_comments_and_unsupported_selectors() {
        let sheet = parse("/* note */ p:hover, .ok { color: blue; }");
        assert_eq!(sheet.rules[0].selectors.len(), 1);
    }
}
