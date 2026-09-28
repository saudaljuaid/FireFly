use std::collections::{HashSet, VecDeque};

use crate::dom::Attribute;
pub use crate::dom::Doctype;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tag {
    pub name: String,
    pub attributes: Vec<Attribute>,
    pub self_closing: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Token {
    Doctype(Doctype),
    StartTag(Tag),
    EndTag(Tag),
    Comment(String),
    Character(String),
    Eof,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum State {
    Data,
    Rcdata,
    Rawtext,
    ScriptData,
    TextLess,
    TextEndName,
    TagOpen,
    EndTagOpen,
    TagName,
    BeforeAttributeName,
    AttributeName,
    AfterAttributeName,
    BeforeAttributeValue,
    DoubleQuotedValue,
    SingleQuotedValue,
    UnquotedValue,
    AfterQuotedValue,
    SelfClosingStartTag,
    MarkupDeclarationOpen,
    BogusComment,
    CommentStart,
    CommentStartDash,
    Comment,
    CommentEndDash,
    CommentEnd,
    CommentEndBang,
    BeforeDoctypeName,
    DoctypeName,
    AfterDoctypeName,
    AfterDoctypeKeyword,
    BeforeDoctypeIdentifier,
    DoctypeIdentifier(char),
    AfterDoctypeIdentifier,
    BogusDoctype,
}

fn space(c: char) -> bool {
    matches!(c, '\t' | '\n' | '\u{c}' | ' ')
}
fn lower(c: char) -> char {
    c.to_ascii_lowercase()
}
fn cleaned(c: char) -> char {
    if c == '\0' { '\u{fffd}' } else { c }
}

pub struct Tokenizer {
    input: Vec<char>,
    pos: usize,
    state: State,
    text_state: State,
    last_start_tag: String,
    text: String,
    ready: VecDeque<Token>,
    tag: Option<Tag>,
    end_tag: bool,
    attribute_name: String,
    attribute_value: String,
    attribute_names: HashSet<String>,
    comment: String,
    doctype: Option<Doctype>,
    identifier_public: bool,
    text_end_name: String,
    finished: bool,
}

impl Tokenizer {
    pub fn new(input: &str) -> Self {
        let mut chars = Vec::with_capacity(input.len().min(16 * 1024 * 1024));
        let mut iter = input.chars().peekable();
        while let Some(c) = iter.next() {
            if c == '\r' {
                if iter.peek() == Some(&'\n') {
                    iter.next();
                }
                chars.push('\n');
            } else {
                chars.push(c);
            }
        }
        Self {
            input: chars,
            pos: 0,
            state: State::Data,
            text_state: State::Data,
            last_start_tag: String::new(),
            text: String::new(),
            ready: VecDeque::new(),
            tag: None,
            end_tag: false,
            attribute_name: String::new(),
            attribute_value: String::new(),
            attribute_names: HashSet::new(),
            comment: String::new(),
            doctype: None,
            identifier_public: false,
            text_end_name: String::new(),
            finished: false,
        }
    }

    pub fn enter_rawtext(&mut self, tag: &str) {
        self.enter_text(State::Rawtext, tag);
    }
    pub fn enter_rcdata(&mut self, tag: &str) {
        self.enter_text(State::Rcdata, tag);
    }
    pub fn enter_script_data(&mut self, tag: &str) {
        self.enter_text(State::ScriptData, tag);
    }
    fn enter_text(&mut self, state: State, tag: &str) {
        self.state = state;
        self.last_start_tag = tag.to_owned();
    }

    fn peek(&self) -> Option<char> {
        self.input.get(self.pos).copied()
    }
    fn take(&mut self) -> Option<char> {
        let c = self.peek()?;
        self.pos += 1;
        Some(c)
    }
    fn starts(&self, text: &str) -> bool {
        text.chars()
            .enumerate()
            .all(|(i, c)| self.input.get(self.pos + i) == Some(&c))
    }
    fn starts_ascii_case(&self, text: &str) -> bool {
        text.chars().enumerate().all(|(i, c)| {
            self.input
                .get(self.pos + i)
                .is_some_and(|actual| actual.eq_ignore_ascii_case(&c))
        })
    }
    fn skip(&mut self, count: usize) {
        self.pos += count;
    }
    fn flush_text(&mut self) {
        if !self.text.is_empty() {
            self.ready
                .push_back(Token::Character(std::mem::take(&mut self.text)));
        }
    }
    fn emit(&mut self, token: Token) {
        self.flush_text();
        self.ready.push_back(token);
    }
    fn emit_comment(&mut self) {
        let comment = std::mem::take(&mut self.comment);
        self.emit(Token::Comment(comment));
    }
    fn emit_tag(&mut self) {
        self.finish_attribute();
        if let Some(tag) = self.tag.take() {
            if self.end_tag {
                self.emit(Token::EndTag(tag));
            } else {
                self.last_start_tag = tag.name.clone();
                self.emit(Token::StartTag(tag));
            }
        }
        self.attribute_names.clear();
        self.state = State::Data;
    }
    fn start_tag(&mut self, end: bool) {
        self.tag = Some(Tag {
            name: String::new(),
            attributes: Vec::new(),
            self_closing: false,
        });
        self.end_tag = end;
        self.attribute_names.clear();
        self.state = State::TagName;
    }
    fn finish_attribute(&mut self) {
        if !self.attribute_name.is_empty() {
            let name = std::mem::take(&mut self.attribute_name);
            let value = std::mem::take(&mut self.attribute_value);
            if self.attribute_names.insert(name.clone()) {
                self.tag
                    .as_mut()
                    .unwrap()
                    .attributes
                    .push(Attribute { name, value });
            }
        }
    }
    fn start_attribute(&mut self) {
        self.finish_attribute();
        self.state = State::AttributeName;
    }
    fn emit_doctype(&mut self) {
        if let Some(doctype) = self.doctype.take() {
            self.emit(Token::Doctype(doctype));
        }
        self.state = State::Data;
    }
    fn doctype_mut(&mut self) -> &mut Doctype {
        self.doctype.as_mut().unwrap()
    }
    fn eof(&mut self) {
        match self.state {
            State::TagOpen => self.text.push('<'),
            State::EndTagOpen => self.text.push_str("</"),
            State::TextLess => self.text.push('<'),
            State::TextEndName => {
                self.text.push_str("</");
                self.text.push_str(&self.text_end_name);
            }
            State::CommentStart
            | State::CommentStartDash
            | State::Comment
            | State::CommentEndDash
            | State::CommentEnd
            | State::CommentEndBang
            | State::BogusComment => {
                if self.state == State::CommentStartDash {
                    self.comment.push('-');
                }
                if self.state == State::CommentEndDash {
                    self.comment.push('-');
                }
                if self.state == State::CommentEnd {
                    self.comment.push_str("--");
                }
                if self.state == State::CommentEndBang {
                    self.comment.push_str("--!");
                }
                self.emit_comment();
            }
            State::BeforeDoctypeName
            | State::DoctypeName
            | State::AfterDoctypeName
            | State::AfterDoctypeKeyword
            | State::BeforeDoctypeIdentifier
            | State::DoctypeIdentifier(_)
            | State::AfterDoctypeIdentifier
            | State::BogusDoctype => {
                self.doctype_mut().force_quirks = true;
                self.emit_doctype();
            }
            State::MarkupDeclarationOpen => self.emit(Token::Comment(String::new())),
            _ => {}
        }
        self.flush_text();
        self.ready.push_back(Token::Eof);
        self.finished = true;
    }

    pub fn next_token(&mut self) -> Option<Token> {
        loop {
            if let Some(token) = self.ready.pop_front() {
                return Some(token);
            }
            if self.finished {
                return None;
            }
            if self.peek().is_none() {
                self.eof();
                continue;
            }
            self.step();
        }
    }

    fn step(&mut self) {
        let c = self.peek().unwrap();
        match self.state {
            State::Data | State::Rcdata | State::Rawtext | State::ScriptData => match c {
                '<' => {
                    self.take();
                    self.text_state = self.state;
                    self.state = if self.text_state == State::Data {
                        State::TagOpen
                    } else {
                        State::TextLess
                    };
                }
                '&' if matches!(self.state, State::Data | State::Rcdata) => {
                    self.character_reference(false)
                }
                _ => {
                    self.take();
                    self.text.push(cleaned(c));
                }
            },
            State::TextLess => {
                if c == '/' {
                    self.take();
                    self.text_end_name.clear();
                    self.state = State::TextEndName;
                } else {
                    self.text.push('<');
                    self.state = self.text_state;
                }
            }
            State::TextEndName => {
                if c.is_ascii_alphabetic() {
                    self.take();
                    self.text_end_name.push(c);
                } else if self
                    .text_end_name
                    .eq_ignore_ascii_case(&self.last_start_tag)
                    && (space(c) || c == '/' || c == '>')
                {
                    self.start_tag(true);
                    self.tag.as_mut().unwrap().name = self.text_end_name.to_ascii_lowercase();
                    if c == '>' {
                        self.take();
                        self.emit_tag();
                    } else if c == '/' {
                        self.take();
                        self.state = State::SelfClosingStartTag;
                    } else {
                        self.take();
                        self.state = State::BeforeAttributeName;
                    }
                } else {
                    self.text.push_str("</");
                    self.text.push_str(&self.text_end_name);
                    self.state = self.text_state;
                }
            }
            State::TagOpen => match c {
                '!' => {
                    self.take();
                    self.state = State::MarkupDeclarationOpen;
                }
                '/' => {
                    self.take();
                    self.state = State::EndTagOpen;
                }
                '?' => {
                    self.comment.clear();
                    self.comment.push('?');
                    self.take();
                    self.state = State::BogusComment;
                }
                _ if c.is_ascii_alphabetic() => self.start_tag(false),
                _ => {
                    self.text.push('<');
                    self.state = State::Data;
                }
            },
            State::EndTagOpen => match c {
                '>' => {
                    self.take();
                    self.state = State::Data;
                }
                _ if c.is_ascii_alphabetic() => self.start_tag(true),
                _ => {
                    self.comment.clear();
                    self.state = State::BogusComment;
                }
            },
            State::TagName => match c {
                '>' => {
                    self.take();
                    self.emit_tag();
                }
                '/' => {
                    self.take();
                    self.state = State::SelfClosingStartTag;
                }
                _ if space(c) => {
                    self.take();
                    self.state = State::BeforeAttributeName;
                }
                _ => {
                    self.take();
                    self.tag.as_mut().unwrap().name.push(lower(cleaned(c)));
                }
            },
            State::BeforeAttributeName => match c {
                _ if space(c) => {
                    self.take();
                }
                '>' => {
                    self.take();
                    self.emit_tag();
                }
                '/' => {
                    self.take();
                    self.state = State::SelfClosingStartTag;
                }
                '=' => {
                    self.start_attribute();
                    self.attribute_name.push('=');
                    self.take();
                }
                _ => self.start_attribute(),
            },
            State::AttributeName => match c {
                _ if space(c) => {
                    self.take();
                    self.state = State::AfterAttributeName;
                }
                '=' => {
                    self.take();
                    self.state = State::BeforeAttributeValue;
                }
                '>' => {
                    self.take();
                    self.emit_tag();
                }
                '/' => {
                    self.take();
                    self.state = State::SelfClosingStartTag;
                }
                _ => {
                    self.take();
                    self.attribute_name.push(lower(cleaned(c)));
                }
            },
            State::AfterAttributeName => match c {
                _ if space(c) => {
                    self.take();
                }
                '=' => {
                    self.take();
                    self.state = State::BeforeAttributeValue;
                }
                '>' => {
                    self.take();
                    self.emit_tag();
                }
                '/' => {
                    self.take();
                    self.state = State::SelfClosingStartTag;
                }
                _ => self.start_attribute(),
            },
            State::BeforeAttributeValue => match c {
                _ if space(c) => {
                    self.take();
                }
                '"' => {
                    self.take();
                    self.state = State::DoubleQuotedValue;
                }
                '\'' => {
                    self.take();
                    self.state = State::SingleQuotedValue;
                }
                '>' => {
                    self.take();
                    self.emit_tag();
                }
                _ => self.state = State::UnquotedValue,
            },
            State::DoubleQuotedValue | State::SingleQuotedValue => {
                let quote = if self.state == State::DoubleQuotedValue {
                    '"'
                } else {
                    '\''
                };
                if c == quote {
                    self.take();
                    self.state = State::AfterQuotedValue;
                } else if c == '&' {
                    self.character_reference(true);
                } else {
                    self.take();
                    self.attribute_value.push(cleaned(c));
                }
            }
            State::UnquotedValue => match c {
                _ if space(c) => {
                    self.take();
                    self.finish_attribute();
                    self.state = State::BeforeAttributeName;
                }
                '>' => {
                    self.take();
                    self.emit_tag();
                }
                '&' => self.character_reference(true),
                _ => {
                    self.take();
                    self.attribute_value.push(cleaned(c));
                }
            },
            State::AfterQuotedValue => match c {
                _ if space(c) => {
                    self.take();
                    self.finish_attribute();
                    self.state = State::BeforeAttributeName;
                }
                '/' => {
                    self.take();
                    self.state = State::SelfClosingStartTag;
                }
                '>' => {
                    self.take();
                    self.emit_tag();
                }
                _ => self.start_attribute(),
            },
            State::SelfClosingStartTag => {
                if c == '>' {
                    self.take();
                    self.tag.as_mut().unwrap().self_closing = true;
                    self.emit_tag();
                } else {
                    self.state = State::BeforeAttributeName;
                }
            }
            State::MarkupDeclarationOpen => {
                if self.starts("--") {
                    self.skip(2);
                    self.comment.clear();
                    self.state = State::CommentStart;
                } else if self.starts_ascii_case("DOCTYPE") {
                    self.skip(7);
                    self.doctype = Some(Doctype {
                        name: None,
                        public_id: None,
                        system_id: None,
                        force_quirks: false,
                    });
                    self.state = State::BeforeDoctypeName;
                } else {
                    self.comment.clear();
                    self.state = State::BogusComment;
                }
            }
            State::BogusComment => {
                if c == '>' {
                    self.take();
                    self.emit_comment();
                    self.state = State::Data;
                } else {
                    self.take();
                    self.comment.push(cleaned(c));
                }
            }
            State::CommentStart => match c {
                '-' => {
                    self.take();
                    self.state = State::CommentStartDash;
                }
                '>' => {
                    self.take();
                    self.emit_comment();
                    self.state = State::Data;
                }
                _ => self.state = State::Comment,
            },
            State::CommentStartDash => match c {
                '-' => {
                    self.take();
                    self.state = State::CommentEnd;
                }
                '>' => {
                    self.take();
                    self.emit_comment();
                    self.state = State::Data;
                }
                _ => {
                    self.comment.push('-');
                    self.state = State::Comment;
                }
            },
            State::Comment => match c {
                '-' => {
                    self.take();
                    self.state = State::CommentEndDash;
                }
                _ => {
                    self.take();
                    self.comment.push(cleaned(c));
                }
            },
            State::CommentEndDash => match c {
                '-' => {
                    self.take();
                    self.state = State::CommentEnd;
                }
                _ => {
                    self.comment.push('-');
                    self.state = State::Comment;
                }
            },
            State::CommentEnd => match c {
                '>' => {
                    self.take();
                    self.emit_comment();
                    self.state = State::Data;
                }
                '!' => {
                    self.take();
                    self.state = State::CommentEndBang;
                }
                '-' => {
                    self.take();
                    self.comment.push('-');
                }
                _ => {
                    self.comment.push_str("--");
                    self.state = State::Comment;
                }
            },
            State::CommentEndBang => match c {
                '>' => {
                    self.take();
                    self.emit_comment();
                    self.state = State::Data;
                }
                '-' => {
                    self.comment.push_str("--!");
                    self.take();
                    self.state = State::CommentEndDash;
                }
                _ => {
                    self.comment.push_str("--!");
                    self.state = State::Comment;
                }
            },
            State::BeforeDoctypeName => match c {
                _ if space(c) => {
                    self.take();
                }
                '>' => {
                    self.take();
                    self.doctype_mut().force_quirks = true;
                    self.emit_doctype();
                }
                _ => {
                    self.doctype_mut().name = Some(String::new());
                    self.state = State::DoctypeName;
                }
            },
            State::DoctypeName => match c {
                _ if space(c) => {
                    self.take();
                    self.state = State::AfterDoctypeName;
                }
                '>' => {
                    self.take();
                    self.emit_doctype();
                }
                _ => {
                    self.take();
                    self.doctype_mut()
                        .name
                        .as_mut()
                        .unwrap()
                        .push(lower(cleaned(c)));
                }
            },
            State::AfterDoctypeName => {
                if space(c) {
                    self.take();
                } else if c == '>' {
                    self.take();
                    self.emit_doctype();
                } else if self.starts_ascii_case("PUBLIC") {
                    self.skip(6);
                    self.identifier_public = true;
                    self.state = State::AfterDoctypeKeyword;
                } else if self.starts_ascii_case("SYSTEM") {
                    self.skip(6);
                    self.identifier_public = false;
                    self.state = State::AfterDoctypeKeyword;
                } else {
                    self.doctype_mut().force_quirks = true;
                    self.state = State::BogusDoctype;
                }
            }
            State::AfterDoctypeKeyword => match c {
                _ if space(c) => {
                    self.take();
                    self.state = State::BeforeDoctypeIdentifier;
                }
                '"' | '\'' => {
                    self.state = State::BeforeDoctypeIdentifier;
                }
                '>' => {
                    self.take();
                    self.doctype_mut().force_quirks = true;
                    self.emit_doctype();
                }
                _ => {
                    self.doctype_mut().force_quirks = true;
                    self.state = State::BogusDoctype;
                }
            },
            State::BeforeDoctypeIdentifier => match c {
                _ if space(c) => {
                    self.take();
                }
                '"' | '\'' => {
                    self.take();
                    if self.identifier_public {
                        self.doctype_mut().public_id = Some(String::new());
                    } else {
                        self.doctype_mut().system_id = Some(String::new());
                    }
                    self.state = State::DoctypeIdentifier(c);
                }
                '>' => {
                    self.take();
                    self.doctype_mut().force_quirks = true;
                    self.emit_doctype();
                }
                _ => {
                    self.doctype_mut().force_quirks = true;
                    self.state = State::BogusDoctype;
                }
            },
            State::DoctypeIdentifier(quote) => {
                if c == quote {
                    self.take();
                    self.state = State::AfterDoctypeIdentifier;
                } else if c == '>' {
                    self.take();
                    self.doctype_mut().force_quirks = true;
                    self.emit_doctype();
                } else {
                    self.take();
                    let id = if self.identifier_public {
                        &mut self.doctype_mut().public_id
                    } else {
                        &mut self.doctype_mut().system_id
                    };
                    id.as_mut().unwrap().push(cleaned(c));
                }
            }
            State::AfterDoctypeIdentifier => {
                if space(c) {
                    self.take();
                } else if c == '>' {
                    self.take();
                    self.emit_doctype();
                } else if self.identifier_public && matches!(c, '"' | '\'') {
                    self.take();
                    self.identifier_public = false;
                    self.doctype_mut().system_id = Some(String::new());
                    self.state = State::DoctypeIdentifier(c);
                } else {
                    self.state = State::BogusDoctype;
                }
            }
            State::BogusDoctype => {
                self.take();
                if c == '>' {
                    self.emit_doctype();
                }
            }
        }
    }

    fn character_reference(&mut self, attribute: bool) {
        self.take();
        let mut replacement = None;
        if self.peek() == Some('#') {
            self.take();
            let radix = if matches!(self.peek(), Some('x' | 'X')) {
                self.take();
                16
            } else {
                10
            };
            let start = self.pos;
            let mut value = 0u32;
            while self.peek().is_some_and(|c| c.is_digit(radix)) {
                let digit = self.take().unwrap().to_digit(radix).unwrap();
                value = value.saturating_mul(radix).saturating_add(digit);
            }
            if self.pos != start {
                if self.peek() == Some(';') {
                    self.take();
                }
                replacement = Some(numeric_reference(value).to_string());
            } else {
                self.pos = start - if radix == 16 { 2 } else { 1 };
            }
        } else {
            let best = NAMED_REFERENCES
                .iter()
                .filter(|(name, _)| {
                    self.starts(name)
                        && !(attribute
                            && !name.ends_with(';')
                            && self
                                .input
                                .get(self.pos + name.len())
                                .is_some_and(|c| c.is_ascii_alphanumeric() || *c == '='))
                })
                .max_by_key(|(name, _)| name.len());
            if let Some((name, value)) = best {
                self.skip(name.len());
                replacement = Some((*value).to_owned());
            }
        }
        let target = if attribute {
            &mut self.attribute_value
        } else {
            &mut self.text
        };
        if let Some(value) = replacement {
            target.push_str(&value);
        } else {
            target.push('&');
        }
    }
}

fn numeric_reference(value: u32) -> char {
    const C1: [char; 32] = [
        '€', '\u{81}', '‚', 'ƒ', '„', '…', '†', '‡', 'ˆ', '‰', 'Š', '‹', 'Œ', '\u{8d}', 'Ž',
        '\u{8f}', '\u{90}', '‘', '’', '“', '”', '•', '–', '—', '˜', '™', 'š', '›', 'œ', '\u{9d}',
        'ž', 'Ÿ',
    ];
    if (0x80..=0x9f).contains(&value) {
        return C1[(value - 0x80) as usize];
    }
    if value == 0 || (0xd800..=0xdfff).contains(&value) {
        return '\u{fffd}';
    }
    char::from_u32(value).unwrap_or('\u{fffd}')
}

const NAMED_REFERENCES: &[(&str, &str)] = &[
    ("amp;", "&"),
    ("amp", "&"),
    ("AMP;", "&"),
    ("AMP", "&"),
    ("lt;", "<"),
    ("lt", "<"),
    ("LT;", "<"),
    ("LT", "<"),
    ("gt;", ">"),
    ("gt", ">"),
    ("GT;", ">"),
    ("GT", ">"),
    ("quot;", "\""),
    ("quot", "\""),
    ("QUOT;", "\""),
    ("QUOT", "\""),
    ("apos;", "'"),
    ("nbsp;", "\u{a0}"),
    ("nbsp", "\u{a0}"),
    ("copy;", "©"),
    ("copy", "©"),
    ("reg;", "®"),
    ("reg", "®"),
    ("trade;", "™"),
    ("mdash;", "—"),
    ("ndash;", "–"),
    ("hellip;", "…"),
    ("euro;", "€"),
    ("bull;", "•"),
    ("lsquo;", "‘"),
    ("rsquo;", "’"),
    ("ldquo;", "“"),
    ("rdquo;", "”"),
];

#[cfg(test)]
mod tests {
    use super::*;

    fn tokens(input: &str) -> Vec<Token> {
        let mut tokenizer = Tokenizer::new(input);
        std::iter::from_fn(|| tokenizer.next_token()).collect()
    }

    fn start(name: &str, attrs: &[(&str, &str)], self_closing: bool) -> Token {
        Token::StartTag(Tag {
            name: name.into(),
            attributes: attrs
                .iter()
                .map(|(name, value)| Attribute {
                    name: (*name).into(),
                    value: (*value).into(),
                })
                .collect(),
            self_closing,
        })
    }

    fn end(name: &str) -> Token {
        Token::EndTag(Tag {
            name: name.into(),
            attributes: vec![],
            self_closing: false,
        })
    }

    // Selected cases from html5lib-tests/tokenizer/test1.test, MIT-licensed.
    #[test]
    fn html5lib_basic_tags_and_attributes() {
        let cases = [
            ("<h>", vec![start("h", &[], false), Token::Eof]),
            (
                "<h a='b'>",
                vec![start("h", &[("a", "b")], false), Token::Eof],
            ),
            (
                "<h a=b>",
                vec![start("h", &[("a", "b")], false), Token::Eof],
            ),
            (
                "<h></h>",
                vec![start("h", &[], false), end("h"), Token::Eof],
            ),
            (
                "<h a='b'c='d'>",
                vec![start("h", &[("a", "b"), ("c", "d")], false), Token::Eof],
            ),
            (
                "<h a='b' a='d'>",
                vec![start("h", &[("a", "b")], false), Token::Eof],
            ),
            ("<>", vec![Token::Character("<>".into()), Token::Eof]),
            ("</>", vec![Token::Eof]),
            (
                "<p>One<p>Two",
                vec![
                    start("p", &[], false),
                    Token::Character("One".into()),
                    start("p", &[], false),
                    Token::Character("Two".into()),
                    Token::Eof,
                ],
            ),
        ];
        for (input, expected) in cases {
            assert_eq!(tokens(input), expected, "{input}");
        }
    }

    #[test]
    fn html5lib_doctype_comment_and_eof() {
        let doctype = |name: &str, force_quirks| {
            Token::Doctype(Doctype {
                name: Some(name.into()),
                public_id: None,
                system_id: None,
                force_quirks,
            })
        };
        let cases = [
            ("<!DOCTYPE HTML>", vec![doctype("html", false), Token::Eof]),
            ("<!DOCTYPE HtMl", vec![doctype("html", true), Token::Eof]),
            ("<!DOC>", vec![Token::Comment("DOC".into()), Token::Eof]),
            (
                "<!--comment-->",
                vec![Token::Comment("comment".into()), Token::Eof],
            ),
            ("<!----->", vec![Token::Comment("-".into()), Token::Eof]),
            ("<!--<!-->", vec![Token::Comment("<!".into()), Token::Eof]),
            (
                "<!--comment",
                vec![Token::Comment("comment".into()), Token::Eof],
            ),
            ("<!-->", vec![Token::Comment(String::new()), Token::Eof]),
            ("<!-", vec![Token::Comment("-".into()), Token::Eof]),
            ("&", vec![Token::Character("&".into()), Token::Eof]),
        ];
        for (input, expected) in cases {
            assert_eq!(tokens(input), expected, "{input}");
        }
    }

    // Selected cases from html5lib-tests/tokenizer/numericEntities.test and entities.test.
    #[test]
    fn html5lib_character_references() {
        let cases = [
            ("&#11111111111", "\u{fffd}"),
            ("&#11111111111x", "\u{fffd}x"),
            ("&#x0000;", "\u{fffd}"),
            ("&#0128;", "€"),
            ("&#x00D;", "\r"),
            ("&rrrraannddom;", "&rrrraannddom;"),
        ];
        for (input, expected) in cases {
            assert_eq!(
                tokens(input),
                vec![Token::Character(expected.into()), Token::Eof],
                "{input}"
            );
        }
        assert_eq!(
            tokens("<h a=&#x26; b='&ampx' c=&#0; />"),
            vec![
                start("h", &[("a", "&"), ("b", "&ampx"), ("c", "\u{fffd}")], true),
                Token::Eof
            ]
        );
        assert_eq!(
            tokens("<h a=\"&lang=\">"),
            vec![start("h", &[("a", "&lang=")], false), Token::Eof]
        );
    }

    #[test]
    fn unfinished_tags_are_abandoned_and_rawtext_closes_only_on_appropriate_tag() {
        assert_eq!(
            tokens("abc<tag a='unfinished"),
            vec![Token::Character("abc".into()), Token::Eof]
        );
        assert_eq!(
            tokens("</"),
            vec![Token::Character("</".into()), Token::Eof]
        );
        assert_eq!(tokens("<"), vec![Token::Character("<".into()), Token::Eof]);
        let mut tokenizer = Tokenizer::new("foo</xmpaar>bar</xMp>tail");
        tokenizer.enter_rawtext("xmp");
        let actual: Vec<_> = std::iter::from_fn(|| tokenizer.next_token()).collect();
        assert_eq!(
            actual,
            vec![
                Token::Character("foo</xmpaar>bar".into()),
                end("xmp"),
                Token::Character("tail".into()),
                Token::Eof
            ]
        );
        let mut tokenizer = Tokenizer::new("foo</xmp>");
        tokenizer.enter_rawtext("xmp");
        assert_eq!(
            std::iter::from_fn(|| tokenizer.next_token()).collect::<Vec<_>>(),
            vec![Token::Character("foo".into()), end("xmp"), Token::Eof]
        );
    }

    #[test]
    fn doctype_identifiers_and_newlines() {
        assert_eq!(
            tokens("<!DOCTYPE html PUBLIC 'id' \"system\">"),
            vec![
                Token::Doctype(Doctype {
                    name: Some("html".into()),
                    public_id: Some("id".into()),
                    system_id: Some("system".into()),
                    force_quirks: false
                }),
                Token::Eof
            ]
        );
        assert_eq!(
            tokens("a\r\nb\rc"),
            vec![Token::Character("a\nb\nc".into()), Token::Eof]
        );
    }
}
