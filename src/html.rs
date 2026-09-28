pub mod tokenizer;

use crate::Error;
use crate::dom::{Attribute, Document, Element, NodeId, NodeKind};
use tokenizer::{Tag, Token, Tokenizer};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum InsertionMode {
    Initial,
    BeforeHtml,
    BeforeHead,
    InHead,
    AfterHead,
    InBody,
    Text,
    AfterBody,
    AfterAfterBody,
}

fn is_void(name: &str) -> bool {
    matches!(
        name,
        "area"
            | "base"
            | "basefont"
            | "bgsound"
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

fn html_space(c: char) -> bool {
    matches!(c, '\t' | '\n' | '\u{c}' | '\r' | ' ')
}

fn split_initial_space(text: &str) -> (&str, &str) {
    let end = text
        .char_indices()
        .find(|&(_, c)| !html_space(c))
        .map_or(text.len(), |(index, _)| index);
    text.split_at(end)
}

fn implied_tag(name: &str) -> Tag {
    Tag {
        name: name.into(),
        attributes: Vec::new(),
        self_closing: false,
    }
}

fn closes_p(name: &str) -> bool {
    matches!(
        name,
        "address"
            | "article"
            | "aside"
            | "blockquote"
            | "center"
            | "details"
            | "dialog"
            | "dir"
            | "div"
            | "dl"
            | "fieldset"
            | "figcaption"
            | "figure"
            | "footer"
            | "form"
            | "h1"
            | "h2"
            | "h3"
            | "h4"
            | "h5"
            | "h6"
            | "header"
            | "hgroup"
            | "hr"
            | "main"
            | "menu"
            | "nav"
            | "ol"
            | "p"
            | "pre"
            | "search"
            | "section"
            | "summary"
            | "table"
            | "ul"
    )
}

fn is_special(name: &str) -> bool {
    closes_p(name)
        || matches!(
            name,
            "applet"
                | "body"
                | "button"
                | "caption"
                | "col"
                | "colgroup"
                | "dd"
                | "dt"
                | "html"
                | "li"
                | "marquee"
                | "object"
                | "option"
                | "optgroup"
                | "select"
                | "tbody"
                | "td"
                | "template"
                | "textarea"
                | "tfoot"
                | "th"
                | "thead"
                | "tr"
        )
}

struct TreeBuilder {
    document: Document,
    open: Vec<NodeId>,
    mode: InsertionMode,
    original_mode: InsertionMode,
    html: Option<NodeId>,
    head: Option<NodeId>,
    body: Option<NodeId>,
}

impl TreeBuilder {
    fn new() -> Self {
        Self {
            document: Document::default(),
            open: Vec::new(),
            mode: InsertionMode::Initial,
            original_mode: InsertionMode::Initial,
            html: None,
            head: None,
            body: None,
        }
    }

    fn current(&self) -> NodeId {
        self.open.last().copied().unwrap_or(0)
    }

    fn name(&self, id: NodeId) -> &str {
        &self
            .document
            .element(id)
            .expect("open node is an element")
            .tag
    }

    fn append_text(&mut self, parent: NodeId, text: &str) {
        if text.is_empty() {
            return;
        }
        if let Some(&last) = self.document.nodes[parent].children.last()
            && let NodeKind::Text(existing) = &mut self.document.nodes[last].kind
        {
            existing.push_str(text);
        } else {
            self.document.append(parent, NodeKind::Text(text.into()));
        }
    }

    fn insert_at(&mut self, parent: NodeId, tag: &Tag) -> Result<NodeId, Error> {
        if !is_void(&tag.name) && self.open.len() >= 256 {
            return Err(Error::InvalidInput(
                "HTML nesting exceeds the 256 element limit".into(),
            ));
        }
        let id = self.document.append(
            parent,
            NodeKind::Element(Element {
                tag: tag.name.clone(),
                attributes: tag.attributes.clone(),
            }),
        );
        if !is_void(&tag.name) {
            self.open.push(id);
        }
        Ok(id)
    }

    fn insert(&mut self, tag: &Tag) -> Result<NodeId, Error> {
        self.insert_at(self.current(), tag)
    }

    fn merge_attributes(&mut self, id: NodeId, attributes: &[Attribute]) {
        let NodeKind::Element(element) = &mut self.document.nodes[id].kind else {
            return;
        };
        for attribute in attributes {
            if element.attribute(&attribute.name).is_none() {
                element.attributes.push(attribute.clone());
            }
        }
    }

    fn text_element(
        &mut self,
        tag: &Tag,
        parent: NodeId,
        tokenizer: &mut Tokenizer,
    ) -> Result<(), Error> {
        self.insert_at(parent, tag)?;
        match tag.name.as_str() {
            "title" | "textarea" => tokenizer.enter_rcdata(&tag.name),
            "script" => tokenizer.enter_script_data(&tag.name),
            _ => tokenizer.enter_rawtext(&tag.name),
        }
        self.original_mode = self.mode;
        self.mode = InsertionMode::Text;
        Ok(())
    }

    fn head_start(
        &mut self,
        tag: &Tag,
        parent: NodeId,
        tokenizer: &mut Tokenizer,
    ) -> Result<bool, Error> {
        match tag.name.as_str() {
            "base" | "basefont" | "bgsound" | "link" | "meta" => {
                self.insert_at(parent, tag)?;
            }
            "title" | "style" | "script" | "noframes" => {
                self.text_element(tag, parent, tokenizer)?;
            }
            _ => return Ok(false),
        }
        Ok(true)
    }

    fn position(&self, name: &str) -> Option<usize> {
        self.open.iter().rposition(|&id| self.name(id) == name)
    }

    fn p_in_button_scope(&self) -> bool {
        for &id in self.open.iter().rev() {
            match self.name(id) {
                "p" => return true,
                "button" | "table" | "html" | "template" => return false,
                _ => {}
            }
        }
        false
    }

    fn close_p(&mut self) {
        if let Some(index) = self.position("p") {
            self.open.truncate(index);
        }
    }

    fn close_previous_li(&mut self) {
        for index in (0..self.open.len()).rev() {
            let name = self.name(self.open[index]);
            if name == "li" {
                self.open.truncate(index);
                return;
            }
            if is_special(name) && !matches!(name, "address" | "div" | "p") {
                return;
            }
        }
    }

    fn close_block(&mut self, name: &str) {
        for index in (0..self.open.len()).rev() {
            let current = self.name(self.open[index]);
            if current == name {
                self.open.truncate(index);
                return;
            }
            if matches!(current, "body" | "html" | "table" | "template") {
                return;
            }
        }
    }

    fn generic_end(&mut self, name: &str) {
        for index in (0..self.open.len()).rev() {
            let current = self.name(self.open[index]);
            if current == name {
                self.open.truncate(index);
                return;
            }
            if is_special(current) {
                return;
            }
        }
    }

    fn body_start(&mut self, tag: &Tag, tokenizer: &mut Tokenizer) -> Result<(), Error> {
        match tag.name.as_str() {
            "html" => {
                if let Some(id) = self.html {
                    self.merge_attributes(id, &tag.attributes);
                }
            }
            "body" => {
                if let Some(id) = self.body {
                    self.merge_attributes(id, &tag.attributes);
                }
            }
            "head" => {}
            "base" | "basefont" | "bgsound" | "link" | "meta" | "title" | "style" | "script"
            | "noframes" => {
                self.head_start(tag, self.current(), tokenizer)?;
            }
            "textarea" | "xmp" | "iframe" | "noembed" => {
                if tag.name == "xmp" && self.p_in_button_scope() {
                    self.close_p();
                }
                self.text_element(tag, self.current(), tokenizer)?;
            }
            "li" => {
                self.close_previous_li();
                if self.p_in_button_scope() {
                    self.close_p();
                }
                self.insert(tag)?;
            }
            _ => {
                if closes_p(&tag.name) && self.p_in_button_scope() {
                    self.close_p();
                }
                self.insert(tag)?;
            }
        }
        Ok(())
    }

    fn body_end(&mut self, name: &str) -> Result<bool, Error> {
        match name {
            "body" => {
                if self.body.is_some() {
                    self.mode = InsertionMode::AfterBody;
                }
            }
            "html" => {
                if self.body.is_some() {
                    self.mode = InsertionMode::AfterBody;
                    return Ok(true);
                }
            }
            "p" => {
                if !self.p_in_button_scope() {
                    self.insert(&implied_tag("p"))?;
                }
                self.close_p();
            }
            "li" => self.close_block("li"),
            "br" => {
                self.insert(&implied_tag("br"))?;
            }
            name if closes_p(name) => self.close_block(name),
            _ => self.generic_end(name),
        }
        Ok(false)
    }

    fn consume(&mut self, mut token: Token, tokenizer: &mut Tokenizer) -> Result<(), Error> {
        for _ in 0..16 {
            if let Token::Character(text) = &token {
                let (spaces, rest) = split_initial_space(text);
                match self.mode {
                    InsertionMode::Initial
                    | InsertionMode::BeforeHtml
                    | InsertionMode::BeforeHead => {
                        if !spaces.is_empty() {
                            if rest.is_empty() {
                                return Ok(());
                            }
                            token = Token::Character(rest.into());
                            continue;
                        }
                    }
                    InsertionMode::InHead | InsertionMode::AfterHead if !spaces.is_empty() => {
                        self.append_text(self.current(), spaces);
                        if rest.is_empty() {
                            return Ok(());
                        }
                        token = Token::Character(rest.into());
                        continue;
                    }
                    _ => {}
                }
            }

            let reprocess = match self.mode {
                InsertionMode::Initial => match &token {
                    Token::Comment(data) => {
                        self.document.append(0, NodeKind::Comment(data.clone()));
                        false
                    }
                    Token::Doctype(doctype) => {
                        self.document.append(0, NodeKind::Doctype(doctype.clone()));
                        self.mode = InsertionMode::BeforeHtml;
                        false
                    }
                    _ => {
                        self.mode = InsertionMode::BeforeHtml;
                        true
                    }
                },
                InsertionMode::BeforeHtml => match &token {
                    Token::Comment(data) => {
                        self.document.append(0, NodeKind::Comment(data.clone()));
                        false
                    }
                    Token::Doctype(_) => false,
                    Token::StartTag(tag) if tag.name == "html" => {
                        self.html = Some(self.insert(tag)?);
                        self.mode = InsertionMode::BeforeHead;
                        false
                    }
                    Token::EndTag(tag)
                        if !matches!(tag.name.as_str(), "head" | "body" | "html" | "br") =>
                    {
                        false
                    }
                    _ => {
                        self.html = Some(self.insert(&implied_tag("html"))?);
                        self.mode = InsertionMode::BeforeHead;
                        true
                    }
                },
                InsertionMode::BeforeHead => match &token {
                    Token::Comment(data) => {
                        self.document
                            .append(self.current(), NodeKind::Comment(data.clone()));
                        false
                    }
                    Token::Doctype(_) => false,
                    Token::StartTag(tag) if tag.name == "html" => {
                        self.merge_attributes(self.html.unwrap(), &tag.attributes);
                        false
                    }
                    Token::StartTag(tag) if tag.name == "head" => {
                        self.head = Some(self.insert(tag)?);
                        self.mode = InsertionMode::InHead;
                        false
                    }
                    Token::EndTag(tag)
                        if !matches!(tag.name.as_str(), "head" | "body" | "html" | "br") =>
                    {
                        false
                    }
                    _ => {
                        self.head = Some(self.insert(&implied_tag("head"))?);
                        self.mode = InsertionMode::InHead;
                        true
                    }
                },
                InsertionMode::InHead => match &token {
                    Token::Comment(data) => {
                        self.document
                            .append(self.current(), NodeKind::Comment(data.clone()));
                        false
                    }
                    Token::Doctype(_) => false,
                    Token::StartTag(tag) if tag.name == "html" => {
                        self.merge_attributes(self.html.unwrap(), &tag.attributes);
                        false
                    }
                    Token::StartTag(tag) if self.head_start(tag, self.current(), tokenizer)? => {
                        false
                    }
                    Token::EndTag(tag) if tag.name == "head" => {
                        self.open.pop();
                        self.mode = InsertionMode::AfterHead;
                        false
                    }
                    Token::EndTag(tag) if !matches!(tag.name.as_str(), "body" | "html" | "br") => {
                        false
                    }
                    _ => {
                        self.open.pop();
                        self.mode = InsertionMode::AfterHead;
                        true
                    }
                },
                InsertionMode::AfterHead => match &token {
                    Token::Comment(data) => {
                        self.document
                            .append(self.current(), NodeKind::Comment(data.clone()));
                        false
                    }
                    Token::Doctype(_) => false,
                    Token::StartTag(tag) if tag.name == "html" => {
                        self.merge_attributes(self.html.unwrap(), &tag.attributes);
                        false
                    }
                    Token::StartTag(tag) if tag.name == "body" => {
                        self.body = Some(self.insert(tag)?);
                        self.mode = InsertionMode::InBody;
                        false
                    }
                    Token::StartTag(tag)
                        if self.head_start(tag, self.head.unwrap(), tokenizer)? =>
                    {
                        false
                    }
                    Token::EndTag(tag) if !matches!(tag.name.as_str(), "body" | "html" | "br") => {
                        false
                    }
                    _ => {
                        self.body = Some(self.insert(&implied_tag("body"))?);
                        self.mode = InsertionMode::InBody;
                        true
                    }
                },
                InsertionMode::InBody => match &token {
                    Token::Character(text) => {
                        self.append_text(self.current(), text);
                        false
                    }
                    Token::Comment(data) => {
                        self.document
                            .append(self.current(), NodeKind::Comment(data.clone()));
                        false
                    }
                    Token::Doctype(_) => false,
                    Token::StartTag(tag) => {
                        self.body_start(tag, tokenizer)?;
                        false
                    }
                    Token::EndTag(tag) => self.body_end(&tag.name)?,
                    Token::Eof => false,
                },
                InsertionMode::Text => match &token {
                    Token::Character(text) => {
                        self.append_text(self.current(), text);
                        false
                    }
                    Token::EndTag(_) => {
                        self.open.pop();
                        self.mode = self.original_mode;
                        false
                    }
                    Token::Eof => {
                        self.open.pop();
                        self.mode = self.original_mode;
                        true
                    }
                    _ => false,
                },
                InsertionMode::AfterBody => match &token {
                    Token::Character(text) if text.chars().all(html_space) => {
                        self.append_text(self.current(), text);
                        false
                    }
                    Token::Comment(data) => {
                        self.document
                            .append(self.html.unwrap(), NodeKind::Comment(data.clone()));
                        false
                    }
                    Token::Doctype(_) => false,
                    Token::StartTag(tag) if tag.name == "html" => {
                        self.merge_attributes(self.html.unwrap(), &tag.attributes);
                        false
                    }
                    Token::EndTag(tag) if tag.name == "html" => {
                        self.mode = InsertionMode::AfterAfterBody;
                        false
                    }
                    Token::Eof => false,
                    _ => {
                        self.mode = InsertionMode::InBody;
                        true
                    }
                },
                InsertionMode::AfterAfterBody => match &token {
                    Token::Character(text) if text.chars().all(html_space) => {
                        self.append_text(self.current(), text);
                        false
                    }
                    Token::Comment(data) => {
                        self.document.append(0, NodeKind::Comment(data.clone()));
                        false
                    }
                    Token::Doctype(_) | Token::Eof => false,
                    Token::StartTag(tag) if tag.name == "html" => {
                        self.merge_attributes(self.html.unwrap(), &tag.attributes);
                        false
                    }
                    _ => {
                        self.mode = InsertionMode::InBody;
                        true
                    }
                },
            };
            if !reprocess {
                return Ok(());
            }
        }
        Err(Error::InvalidInput(
            "HTML insertion-mode loop exceeded".into(),
        ))
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
