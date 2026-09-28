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
    InTable,
    InTableText,
    InTableBody,
    InRow,
    InCell,
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
    foster_parenting: bool,
    pending_table_text: String,
    table_text_mode: InsertionMode,
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
            foster_parenting: false,
            pending_table_text: String::new(),
            table_text_mode: InsertionMode::InTable,
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

    fn insertion_location(&self) -> (NodeId, Option<NodeId>) {
        let current = self.current();
        if self.foster_parenting
            && matches!(
                self.name(current),
                "table" | "tbody" | "tfoot" | "thead" | "tr"
            )
            && let Some(&table) = self.open.iter().rev().find(|&&id| self.name(id) == "table")
            && let Some(parent) = self.document.nodes[table].parent
        {
            return (parent, Some(table));
        }
        (current, None)
    }

    fn insert_text(&mut self, text: &str) {
        if text.is_empty() {
            return;
        }
        let (parent, reference) = self.insertion_location();
        let Some(reference) = reference else {
            self.append_text(parent, text);
            return;
        };
        let index = self.document.nodes[parent]
            .children
            .iter()
            .rposition(|&id| id == reference)
            .expect("table must be a child of its parent");
        if index > 0 {
            let previous = self.document.nodes[parent].children[index - 1];
            if let NodeKind::Text(existing) = &mut self.document.nodes[previous].kind {
                existing.push_str(text);
                return;
            }
        }
        self.document
            .insert_before(parent, Some(reference), NodeKind::Text(text.into()));
    }

    fn insert_at(&mut self, parent: NodeId, tag: &Tag) -> Result<NodeId, Error> {
        if !is_void(&tag.name) && self.open.len() >= 256 {
            return Err(Error::InvalidInput(
                "HTML nesting exceeds the 256 element limit".into(),
            ));
        }
        let id = self.document.insert_before(
            parent,
            None,
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
        if !self.foster_parenting {
            return self.insert_at(self.current(), tag);
        }
        if !is_void(&tag.name) && self.open.len() >= 256 {
            return Err(Error::InvalidInput(
                "HTML nesting exceeds the 256 element limit".into(),
            ));
        }
        let (parent, reference) = self.insertion_location();
        let id = self.document.insert_before(
            parent,
            reference,
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

    fn in_table_scope(&self, name: &str) -> bool {
        for &id in self.open.iter().rev() {
            let current = self.name(id);
            if current == name {
                return true;
            }
            if matches!(current, "table" | "template" | "html") {
                return false;
            }
        }
        false
    }

    fn in_section_scope(&self) -> bool {
        ["tbody", "thead", "tfoot"]
            .iter()
            .any(|name| self.in_table_scope(name))
    }

    fn clear_to(&mut self, context: &[&str]) {
        while !context.contains(&self.name(self.current())) {
            self.open.pop();
        }
    }

    fn reset_mode(&mut self) {
        self.mode = self
            .open
            .iter()
            .rev()
            .find_map(|&id| match self.name(id) {
                "td" | "th" => Some(InsertionMode::InCell),
                "tr" => Some(InsertionMode::InRow),
                "tbody" | "thead" | "tfoot" => Some(InsertionMode::InTableBody),
                "table" => Some(InsertionMode::InTable),
                "body" => Some(InsertionMode::InBody),
                _ => None,
            })
            .unwrap_or(InsertionMode::InBody);
    }

    fn close_cell(&mut self) {
        if let Some(index) = self
            .open
            .iter()
            .rposition(|&id| matches!(self.name(id), "td" | "th"))
        {
            self.open.truncate(index);
            self.mode = InsertionMode::InRow;
        }
    }

    fn close_row(&mut self) {
        self.clear_to(&["tr", "template", "html"]);
        if self.name(self.current()) == "tr" {
            self.open.pop();
            self.mode = InsertionMode::InTableBody;
        }
    }

    fn close_section(&mut self) {
        self.clear_to(&["tbody", "thead", "tfoot", "template", "html"]);
        if matches!(self.name(self.current()), "tbody" | "thead" | "tfoot") {
            self.open.pop();
            self.mode = InsertionMode::InTable;
        }
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
            "table" => {
                if self.p_in_button_scope() {
                    self.close_p();
                }
                self.insert(tag)?;
                self.mode = InsertionMode::InTable;
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

    fn process_in_body(
        &mut self,
        token: &Token,
        tokenizer: &mut Tokenizer,
        foster: bool,
    ) -> Result<bool, Error> {
        self.foster_parenting = foster;
        let result = match token {
            Token::Character(text) => {
                self.insert_text(text);
                Ok(false)
            }
            Token::Comment(data) => {
                self.document
                    .append(self.current(), NodeKind::Comment(data.clone()));
                Ok(false)
            }
            Token::Doctype(_) | Token::Eof => Ok(false),
            Token::StartTag(tag) => {
                self.body_start(tag, tokenizer)?;
                Ok(false)
            }
            Token::EndTag(tag) => self.body_end(&tag.name),
        };
        self.foster_parenting = false;
        result
    }

    fn process_in_table(
        &mut self,
        token: &Token,
        tokenizer: &mut Tokenizer,
    ) -> Result<bool, Error> {
        match token {
            Token::Character(_)
                if matches!(
                    self.name(self.current()),
                    "table" | "tbody" | "tfoot" | "thead" | "tr"
                ) =>
            {
                self.pending_table_text.clear();
                self.table_text_mode = self.mode;
                self.mode = InsertionMode::InTableText;
                Ok(true)
            }
            Token::Comment(data) => {
                self.document
                    .append(self.current(), NodeKind::Comment(data.clone()));
                Ok(false)
            }
            Token::Doctype(_) => Ok(false),
            Token::StartTag(tag) if matches!(tag.name.as_str(), "tbody" | "thead" | "tfoot") => {
                self.clear_to(&["table", "template", "html"]);
                self.insert(tag)?;
                self.mode = InsertionMode::InTableBody;
                Ok(false)
            }
            Token::StartTag(tag) if matches!(tag.name.as_str(), "tr" | "td" | "th") => {
                self.clear_to(&["table", "template", "html"]);
                self.insert(&implied_tag("tbody"))?;
                self.mode = InsertionMode::InTableBody;
                Ok(true)
            }
            Token::StartTag(tag) if tag.name == "table" => {
                if self.in_table_scope("table") {
                    let index = self.position("table").unwrap();
                    self.open.truncate(index);
                    self.reset_mode();
                    Ok(true)
                } else {
                    Ok(false)
                }
            }
            Token::EndTag(tag) if tag.name == "table" => {
                if self.in_table_scope("table") {
                    let index = self.position("table").unwrap();
                    self.open.truncate(index);
                    self.reset_mode();
                }
                Ok(false)
            }
            Token::EndTag(tag)
                if matches!(
                    tag.name.as_str(),
                    "body"
                        | "caption"
                        | "col"
                        | "colgroup"
                        | "html"
                        | "tbody"
                        | "td"
                        | "tfoot"
                        | "th"
                        | "thead"
                        | "tr"
                ) =>
            {
                Ok(false)
            }
            Token::StartTag(tag) if matches!(tag.name.as_str(), "style" | "script") => {
                self.head_start(tag, self.current(), tokenizer)?;
                Ok(false)
            }
            Token::StartTag(tag)
                if tag.name == "input"
                    && tag.attributes.iter().any(|attr| {
                        attr.name == "type" && attr.value.eq_ignore_ascii_case("hidden")
                    }) =>
            {
                self.insert(tag)?;
                Ok(false)
            }
            Token::Eof => Ok(false),
            _ => self.process_in_body(token, tokenizer, true),
        }
    }

    fn process_in_table_body(
        &mut self,
        token: &Token,
        tokenizer: &mut Tokenizer,
    ) -> Result<bool, Error> {
        match token {
            Token::StartTag(tag) if tag.name == "tr" => {
                self.clear_to(&["tbody", "thead", "tfoot", "template", "html"]);
                self.insert(tag)?;
                self.mode = InsertionMode::InRow;
                Ok(false)
            }
            Token::StartTag(tag) if matches!(tag.name.as_str(), "td" | "th") => {
                self.clear_to(&["tbody", "thead", "tfoot", "template", "html"]);
                self.insert(&implied_tag("tr"))?;
                self.mode = InsertionMode::InRow;
                Ok(true)
            }
            Token::EndTag(tag) if matches!(tag.name.as_str(), "tbody" | "thead" | "tfoot") => {
                if self.in_table_scope(&tag.name) {
                    self.close_section();
                }
                Ok(false)
            }
            Token::StartTag(tag)
                if matches!(
                    tag.name.as_str(),
                    "caption" | "col" | "colgroup" | "tbody" | "thead" | "tfoot"
                ) =>
            {
                if self.in_section_scope() {
                    self.close_section();
                    Ok(true)
                } else {
                    Ok(false)
                }
            }
            Token::EndTag(tag) if tag.name == "table" => {
                if self.in_section_scope() {
                    self.close_section();
                    Ok(true)
                } else {
                    Ok(false)
                }
            }
            Token::EndTag(tag)
                if matches!(
                    tag.name.as_str(),
                    "body" | "caption" | "col" | "colgroup" | "html" | "td" | "th" | "tr"
                ) =>
            {
                Ok(false)
            }
            _ => self.process_in_table(token, tokenizer),
        }
    }

    fn process_in_row(&mut self, token: &Token, tokenizer: &mut Tokenizer) -> Result<bool, Error> {
        match token {
            Token::StartTag(tag) if matches!(tag.name.as_str(), "td" | "th") => {
                self.clear_to(&["tr", "template", "html"]);
                self.insert(tag)?;
                self.mode = InsertionMode::InCell;
                Ok(false)
            }
            Token::EndTag(tag) if tag.name == "tr" => {
                if self.in_table_scope("tr") {
                    self.close_row();
                }
                Ok(false)
            }
            Token::StartTag(tag)
                if matches!(
                    tag.name.as_str(),
                    "caption" | "col" | "colgroup" | "tbody" | "thead" | "tfoot" | "tr"
                ) =>
            {
                if self.in_table_scope("tr") {
                    self.close_row();
                    Ok(true)
                } else {
                    Ok(false)
                }
            }
            Token::EndTag(tag) if tag.name == "table" => {
                if self.in_table_scope("tr") {
                    self.close_row();
                    Ok(true)
                } else {
                    Ok(false)
                }
            }
            Token::EndTag(tag) if matches!(tag.name.as_str(), "tbody" | "thead" | "tfoot") => {
                if self.in_table_scope(&tag.name) && self.in_table_scope("tr") {
                    self.close_row();
                    Ok(true)
                } else {
                    Ok(false)
                }
            }
            Token::EndTag(tag)
                if matches!(
                    tag.name.as_str(),
                    "body" | "caption" | "col" | "colgroup" | "html" | "td" | "th"
                ) =>
            {
                Ok(false)
            }
            _ => self.process_in_table(token, tokenizer),
        }
    }

    fn process_in_cell(&mut self, token: &Token, tokenizer: &mut Tokenizer) -> Result<bool, Error> {
        match token {
            Token::EndTag(tag) if matches!(tag.name.as_str(), "td" | "th") => {
                if self.in_table_scope(&tag.name) {
                    self.close_cell();
                }
                Ok(false)
            }
            Token::StartTag(tag)
                if matches!(
                    tag.name.as_str(),
                    "caption"
                        | "col"
                        | "colgroup"
                        | "tbody"
                        | "td"
                        | "tfoot"
                        | "th"
                        | "thead"
                        | "tr"
                ) =>
            {
                if self.in_table_scope("td") || self.in_table_scope("th") {
                    self.close_cell();
                    Ok(true)
                } else {
                    Ok(false)
                }
            }
            Token::EndTag(tag)
                if matches!(
                    tag.name.as_str(),
                    "table" | "tbody" | "tfoot" | "thead" | "tr"
                ) =>
            {
                if self.in_table_scope(&tag.name) {
                    self.close_cell();
                    Ok(true)
                } else {
                    Ok(false)
                }
            }
            Token::EndTag(tag)
                if matches!(
                    tag.name.as_str(),
                    "body" | "caption" | "col" | "colgroup" | "html"
                ) =>
            {
                Ok(false)
            }
            _ => self.process_in_body(token, tokenizer, false),
        }
    }

    fn consume(&mut self, mut token: Token, tokenizer: &mut Tokenizer) -> Result<(), Error> {
        for _ in 0..32 {
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
                InsertionMode::InBody => self.process_in_body(&token, tokenizer, false)?,
                InsertionMode::InTable => self.process_in_table(&token, tokenizer)?,
                InsertionMode::InTableText => match &token {
                    Token::Character(text) => {
                        self.pending_table_text
                            .extend(text.chars().filter(|&c| c != '\0'));
                        false
                    }
                    _ => {
                        let pending = std::mem::take(&mut self.pending_table_text);
                        if pending.chars().all(html_space) {
                            self.append_text(self.current(), &pending);
                        } else {
                            self.process_in_body(&Token::Character(pending), tokenizer, true)?;
                        }
                        self.mode = self.table_text_mode;
                        true
                    }
                },
                InsertionMode::InTableBody => self.process_in_table_body(&token, tokenizer)?,
                InsertionMode::InRow => self.process_in_row(&token, tokenizer)?,
                InsertionMode::InCell => self.process_in_cell(&token, tokenizer)?,
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
