pub mod tokenizer;

use crate::Error;
use crate::dom::{Document, Element, NodeId, NodeKind};
use tokenizer::{Tag, Token, Tokenizer};

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

struct TreeBuilder {
    document: Document,
    stack: Vec<NodeId>,
}

impl TreeBuilder {
    fn new() -> Self {
        Self {
            document: Document::default(),
            stack: vec![0],
        }
    }

    fn consume(&mut self, token: Token, tokenizer: &mut Tokenizer) -> Result<(), Error> {
        match token {
            Token::Character(text) => {
                append_text(&mut self.document, *self.stack.last().unwrap(), text)
            }
            Token::StartTag(Tag {
                name,
                attributes,
                self_closing,
            }) => {
                if matches!(name.as_str(), "p" | "li")
                    && self.stack.last().is_some_and(|&id| {
                        self.document
                            .element(id)
                            .is_some_and(|element| element.tag == name)
                    })
                {
                    self.stack.pop();
                }
                let parent = *self.stack.last().unwrap();
                let id = self.document.append(
                    parent,
                    NodeKind::Element(Element {
                        tag: name.clone(),
                        attributes,
                    }),
                );
                if is_void(&name) {
                    return Ok(());
                }
                // HTML self-closing flags on non-void HTML elements are ignored by tree construction.
                let _ = self_closing;
                if self.stack.len() >= 256 {
                    return Err(Error::InvalidInput(
                        "HTML nesting exceeds the 256 element limit".into(),
                    ));
                }
                self.stack.push(id);
                match name.as_str() {
                    "style" | "xmp" | "iframe" | "noembed" | "noframes" => {
                        tokenizer.enter_rawtext(&name)
                    }
                    "script" => tokenizer.enter_script_data(&name),
                    "title" | "textarea" => tokenizer.enter_rcdata(&name),
                    _ => {}
                }
            }
            Token::EndTag(tag) => {
                if let Some(index) = self.stack.iter().rposition(|&id| {
                    self.document
                        .element(id)
                        .is_some_and(|element| element.tag == tag.name)
                }) {
                    self.stack.truncate(index);
                }
            }
            Token::Doctype(_) | Token::Comment(_) | Token::Eof => {}
        }
        Ok(())
    }
}

pub fn parse(input: &str) -> Result<Document, Error> {
    if input.len() > 16 * 1024 * 1024 {
        return Err(Error::InvalidInput("HTML input exceeds 16 MiB".into()));
    }
    let mut tokenizer = Tokenizer::new(input);
    let mut builder = TreeBuilder::new();
    while let Some(token) = tokenizer.next_token() {
        let eof = token == Token::Eof;
        builder.consume(token, &mut tokenizer)?;
        if eof {
            break;
        }
    }
    Ok(builder.document)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_text_attributes_and_entities() {
        let document = parse("<p class='lead'>A &amp; B <b>bold</b></p>").unwrap();
        assert!(document.element(1).unwrap().has_class("lead"));
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
        assert!(parse(&"<div>".repeat(256)).is_err());
    }

    #[test]
    fn raw_text_and_rcdata_keep_rendering_behavior() {
        let document = parse("<style>p::before { content: '<b>&amp;' }</style><script>if (a < b) x='&amp;';</script><title>A &amp; B</title><p>ok</p>").unwrap();
        assert!(document.stylesheets().contains("'<b>&amp;'"));
        assert_eq!(
            document.nodes[4].kind,
            NodeKind::Text("if (a < b) x='&amp;';".into())
        );
        assert_eq!(document.nodes[6].kind, NodeKind::Text("A & B".into()));
        assert!(
            document
                .nodes
                .iter()
                .any(|node| node.kind == NodeKind::Text("ok".into()))
        );
    }
}
