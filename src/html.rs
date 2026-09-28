use crate::Error;
use crate::dom::{Attribute, Document, Element, NodeId, NodeKind};

struct Cursor {
    chars: Vec<char>,
    position: usize,
}

impl Cursor {
    fn new(input: &str) -> Self {
        Self {
            chars: input.chars().collect(),
            position: 0,
        }
    }

    fn peek(&self) -> Option<char> {
        self.chars.get(self.position).copied()
    }

    fn starts_with(&self, text: &str) -> bool {
        text.chars()
            .enumerate()
            .all(|(offset, expected)| self.chars.get(self.position + offset) == Some(&expected))
    }

    fn starts_with_ascii_case(&self, text: &str) -> bool {
        text.chars().enumerate().all(|(offset, expected)| {
            self.chars
                .get(self.position + offset)
                .is_some_and(|actual| actual.eq_ignore_ascii_case(&expected))
        })
    }

    fn consume(&mut self) -> Option<char> {
        let value = self.peek()?;
        self.position += 1;
        Some(value)
    }

    fn consume_while(&mut self, predicate: impl Fn(char) -> bool) -> String {
        let mut result = String::new();
        while self.peek().is_some_and(&predicate) {
            result.push(self.consume().unwrap());
        }
        result
    }

    fn whitespace(&mut self) {
        self.consume_while(char::is_whitespace);
    }

    fn skip_until(&mut self, pattern: &str) {
        while self.peek().is_some() && !self.starts_with(pattern) {
            self.consume();
        }
        if self.starts_with(pattern) {
            self.position += pattern.chars().count();
        }
    }
}

fn name_character(character: char) -> bool {
    character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | ':')
}

fn decode_entities(source: &str) -> String {
    let mut result = String::with_capacity(source.len());
    let mut remaining = source;
    while let Some(index) = remaining.find('&') {
        result.push_str(&remaining[..index]);
        remaining = &remaining[index + 1..];
        let Some(end) = remaining.find(';').filter(|&end| end <= 12) else {
            result.push('&');
            continue;
        };
        let entity = &remaining[..end];
        let decoded = match entity {
            "amp" => Some('&'),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "quot" => Some('"'),
            "apos" | "#39" => Some('\''),
            "nbsp" => Some('\u{a0}'),
            _ => entity
                .strip_prefix("#x")
                .or_else(|| entity.strip_prefix("#X"))
                .and_then(|digits| u32::from_str_radix(digits, 16).ok())
                .or_else(|| {
                    entity
                        .strip_prefix('#')
                        .and_then(|digits| digits.parse().ok())
                })
                .and_then(char::from_u32),
        };
        if let Some(character) = decoded {
            result.push(character);
            remaining = &remaining[end + 1..];
        } else {
            result.push('&');
        }
    }
    result.push_str(remaining);
    result
}

fn is_void(tag: &str) -> bool {
    matches!(
        tag,
        "area"
            | "base"
            | "br"
            | "col"
            | "embed"
            | "hr"
            | "img"
            | "input"
            | "link"
            | "meta"
            | "param"
            | "source"
            | "track"
            | "wbr"
    )
}

fn append_text(document: &mut Document, parent: NodeId, text: String) {
    if text.is_empty() {
        return;
    }
    if let Some(&last) = document.nodes[parent].children.last()
        && let NodeKind::Text(existing) = &mut document.nodes[last].kind
    {
        existing.push_str(&text);
        return;
    }
    document.append(parent, NodeKind::Text(text));
}

pub fn parse(input: &str) -> Result<Document, Error> {
    let mut cursor = Cursor::new(input);
    let mut document = Document::default();
    let mut stack = vec![0];

    while let Some(character) = cursor.peek() {
        if character != '<' {
            let text = cursor.consume_while(|value| value != '<');
            append_text(
                &mut document,
                *stack.last().unwrap(),
                decode_entities(&text),
            );
            continue;
        }
        if cursor.starts_with("<!--") {
            cursor.position += 4;
            cursor.skip_until("-->");
            continue;
        }
        if cursor.starts_with("<!") || cursor.starts_with("<?") {
            cursor.skip_until(">");
            continue;
        }
        if cursor.starts_with("</") {
            cursor.position += 2;
            cursor.whitespace();
            let tag = cursor.consume_while(name_character).to_ascii_lowercase();
            cursor.skip_until(">");
            if let Some(index) = stack.iter().rposition(|&id| {
                document
                    .element(id)
                    .is_some_and(|element| element.tag == tag)
            }) {
                stack.truncate(index);
            }
            continue;
        }

        cursor.consume();
        let tag = cursor.consume_while(name_character).to_ascii_lowercase();
        if tag.is_empty() {
            append_text(&mut document, *stack.last().unwrap(), "<".into());
            continue;
        }
        let mut attributes = Vec::new();
        let mut self_closing = false;
        loop {
            cursor.whitespace();
            match cursor.peek() {
                None => break,
                Some('>') => {
                    cursor.consume();
                    break;
                }
                Some('/') if cursor.chars.get(cursor.position + 1) == Some(&'>') => {
                    cursor.position += 2;
                    self_closing = true;
                    break;
                }
                _ => {}
            }
            let name = cursor.consume_while(name_character).to_ascii_lowercase();
            if name.is_empty() {
                cursor.consume();
                continue;
            }
            cursor.whitespace();
            let value = if cursor.peek() == Some('=') {
                cursor.consume();
                cursor.whitespace();
                match cursor.peek() {
                    Some(quote @ ('\'' | '"')) => {
                        cursor.consume();
                        let text = cursor.consume_while(|character| character != quote);
                        if cursor.peek() == Some(quote) {
                            cursor.consume();
                        }
                        text
                    }
                    _ => cursor
                        .consume_while(|character| !character.is_whitespace() && character != '>'),
                }
            } else {
                String::new()
            };
            if !attributes
                .iter()
                .any(|attribute: &Attribute| attribute.name == name)
            {
                attributes.push(Attribute {
                    name,
                    value: decode_entities(&value),
                });
            }
        }
        if matches!(tag.as_str(), "p" | "li")
            && stack.last().is_some_and(|&id| {
                document
                    .element(id)
                    .is_some_and(|element| element.tag == tag)
            })
        {
            stack.pop();
        }
        let parent = *stack.last().unwrap();
        let id = document.append(parent, NodeKind::Element(Element { tag, attributes }));
        if self_closing || is_void(document.element(id).unwrap().tag.as_str()) {
            continue;
        }
        if matches!(
            document.element(id).unwrap().tag.as_str(),
            "style" | "script"
        ) {
            let tag = document.element(id).unwrap().tag.clone();
            let closing = format!("</{tag}");
            let mut text = String::new();
            while cursor.peek().is_some() && !cursor.starts_with_ascii_case(&closing) {
                text.push(cursor.consume().unwrap());
            }
            append_text(&mut document, id, text);
            if cursor.peek().is_some() {
                cursor.skip_until(">");
            }
        } else {
            if stack.len() >= 256 {
                return Err(Error::InvalidInput(
                    "HTML nesting exceeds the 256 element limit".into(),
                ));
            }
            stack.push(id);
        }
    }
    Ok(document)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_text_attributes_and_entities() {
        let document = parse("<p class='lead'>A &amp; B <b>bold</b></p>").unwrap();
        let paragraph = document.element(1).unwrap();
        assert!(paragraph.has_class("lead"));
        assert_eq!(document.nodes[2].kind, NodeKind::Text("A & B ".into()));
        assert_eq!(document.nodes[4].kind, NodeKind::Text("bold".into()));
    }

    #[test]
    fn recovers_from_mismatched_closing_tags() {
        let document = parse("<div><p>one</div><p>two</p>").unwrap();
        assert_eq!(document.nodes[0].children.len(), 2);
    }

    #[test]
    fn rejects_excessive_nesting() {
        let source = "<div>".repeat(256);
        assert!(parse(&source).is_err());
    }
}
