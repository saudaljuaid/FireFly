use std::collections::{HashSet, VecDeque};

use super::errors::{ErrorPhase, ParseError, SourcePosition};
use crate::dom::Attribute;
pub use crate::dom::Doctype;

#[path = "named_references.rs"]
mod named_references;
use named_references::{MAX_NAMED_REFERENCE_LENGTH, NAMED_REFERENCES};

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
    ProcessingInstruction { target: String, data: String },
    Character(String),
    Eof,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum State {
    Data,
    Plaintext,
    Rcdata,
    Rawtext,
    ScriptData,
    ScriptEscapeStart,
    ScriptEscapeStartDash,
    ScriptEscaped,
    ScriptEscapedDash,
    ScriptEscapedDashDash,
    ScriptEscapedLess,
    ScriptDoubleEscapeStart,
    ScriptDoubleEscaped,
    ScriptDoubleEscapedDash,
    ScriptDoubleEscapedDashDash,
    ScriptDoubleEscapedLess,
    ScriptDoubleEscapeEnd,
    TextLess,
    TextEndName,
    TagOpen,
    ProcessingInstructionOpen,
    ProcessingInstructionTarget,
    AfterProcessingInstructionTarget,
    ProcessingInstructionData,
    ProcessingInstructionQuestionable,
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
    CdataSection,
    CdataSectionBracket,
    CdataSectionEnd,
    BogusComment,
    CommentStart,
    CommentStartDash,
    Comment,
    CommentLess,
    CommentLessBang,
    CommentLessBangDash,
    CommentLessBangDashDash,
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
fn is_noncharacter(ch: char) -> bool {
    let value = ch as u32;
    (0xfdd0..=0xfdef).contains(&value) || (value & 0xfffe == 0xfffe)
}
fn is_disallowed_control(ch: char) -> bool {
    matches!(ch as u32, 0x01..=0x08 | 0x0b | 0x0e..=0x1f | 0x7f..=0x9f)
}

pub struct Tokenizer {
    input: Vec<char>,
    positions: Vec<(u32, u32)>,
    errors: Vec<ParseError>,
    pos: usize,
    state: State,
    text_state: State,
    last_start_tag: String,
    text: String,
    text_boundary_position: Option<SourcePosition>,
    ready: VecDeque<Token>,
    ready_positions: VecDeque<SourcePosition>,
    token_position: SourcePosition,
    tag: Option<Tag>,
    end_tag: bool,
    attribute_name: String,
    attribute_value: String,
    attribute_start: usize,
    attribute_names: HashSet<String>,
    comment: String,
    pi_target: String,
    pi_data: String,
    processing_instructions: bool,
    doctype: Option<Doctype>,
    identifier_public: bool,
    doctype_space_seen: bool,
    doctype_identifier_space_seen: bool,
    last_null_reported: Option<usize>,
    text_end_name: String,
    script_escape_name: String,
    cdata_allowed: bool,
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
        let mut positions = Vec::with_capacity(chars.len() + 1);
        let (mut line, mut column) = (1u32, 1u32);
        let mut errors = Vec::new();
        for (offset, &ch) in chars.iter().enumerate() {
            let position = SourcePosition {
                offset,
                line: line as usize,
                column: column as usize,
            };
            positions.push((line, column));
            if is_noncharacter(ch) {
                errors.push(ParseError {
                    code: "noncharacter-in-input-stream",
                    position,
                    phase: ErrorPhase::Input,
                });
            } else if is_disallowed_control(ch) {
                errors.push(ParseError {
                    code: "control-character-in-input-stream",
                    position,
                    phase: ErrorPhase::Input,
                });
            }
            if ch == '\n' {
                line += 1;
                column = 1;
            } else {
                column += ch.len_utf16() as u32;
            }
        }
        positions.push((line, column));
        Self {
            input: chars,
            positions,
            errors,
            pos: 0,
            state: State::Data,
            text_state: State::Data,
            last_start_tag: String::new(),
            text: String::new(),
            text_boundary_position: None,
            ready: VecDeque::new(),
            ready_positions: VecDeque::new(),
            token_position: SourcePosition {
                offset: 0,
                line: 1,
                column: 1,
            },
            tag: None,
            end_tag: false,
            attribute_name: String::new(),
            attribute_value: String::new(),
            attribute_start: 0,
            attribute_names: HashSet::new(),
            comment: String::new(),
            pi_target: String::new(),
            pi_data: String::new(),
            processing_instructions: true,
            doctype: None,
            identifier_public: false,
            doctype_space_seen: false,
            doctype_identifier_space_seen: false,
            last_null_reported: None,
            text_end_name: String::new(),
            script_escape_name: String::new(),
            cdata_allowed: false,
            finished: false,
        }
    }

    /// Retain the pre-processing-instruction behavior expected by the older
    /// html5lib tokenizer snapshot, which treats every `<?` as a bogus comment.
    pub fn new_legacy_html5lib(input: &str) -> Self {
        let mut tokenizer = Self::new(input);
        tokenizer.processing_instructions = false;
        tokenizer
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
    pub fn enter_plaintext(&mut self) {
        self.state = State::Plaintext;
    }
    pub fn enter_cdata(&mut self) {
        self.state = State::CdataSection;
    }
    pub fn set_last_start_tag(&mut self, tag: &str) {
        self.last_start_tag = tag.to_owned();
    }
    pub fn ignore_next_lf(&mut self) {
        if self.peek() == Some('\n') {
            self.take();
        }
    }
    pub fn set_cdata_allowed(&mut self, allowed: bool) {
        self.cdata_allowed = allowed;
    }
    pub fn errors(&self) -> Vec<ParseError> {
        let mut errors = self.errors.clone();
        errors.sort_by_key(|error| (error.position.offset, error.phase as u8));
        errors
    }
    pub fn into_errors(self) -> Vec<ParseError> {
        let mut errors = self.errors;
        errors.sort_by_key(|error| (error.position.offset, error.phase as u8));
        errors
    }
    fn position_at(&self, offset: usize) -> SourcePosition {
        let (line, column) = self.positions[offset];
        SourcePosition {
            offset,
            line: line as usize,
            column: column as usize,
        }
    }
    pub fn position(&self) -> SourcePosition {
        self.position_at(self.pos)
    }
    pub fn previous_position(&self) -> SourcePosition {
        self.position_at(self.pos.saturating_sub(1))
    }
    pub fn token_position(&self) -> SourcePosition {
        self.token_position
    }
    fn error(&mut self, code: &'static str) {
        self.errors.push(ParseError {
            code,
            position: self.position(),
            phase: ErrorPhase::Tokenizer,
        });
    }
    fn error_previous(&mut self, code: &'static str) {
        self.errors.push(ParseError {
            code,
            position: self.previous_position(),
            phase: ErrorPhase::Tokenizer,
        });
    }
    fn error_at(&mut self, code: &'static str, offset: usize) {
        self.errors.push(ParseError {
            code,
            position: self.position_at(offset),
            phase: ErrorPhase::Tokenizer,
        });
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
            self.ready_positions.push_back(
                self.text_boundary_position
                    .take()
                    .unwrap_or_else(|| self.previous_position()),
            );
        }
    }
    fn emit(&mut self, token: Token) {
        self.flush_text();
        self.ready.push_back(token);
        self.ready_positions.push_back(self.previous_position());
    }
    fn emit_comment(&mut self) {
        let comment = std::mem::take(&mut self.comment);
        self.emit(Token::Comment(comment));
    }
    fn emit_processing_instruction(&mut self) {
        let target = std::mem::take(&mut self.pi_target);
        let data = std::mem::take(&mut self.pi_data);
        self.emit(Token::ProcessingInstruction { target, data });
    }
    fn pi_to_comment(&mut self) {
        self.comment.clear();
        self.comment.push('?');
        self.comment.push_str(&self.pi_target);
        self.pi_target.clear();
        self.pi_data.clear();
        self.state = State::BogusComment;
    }
    fn emit_tag(&mut self) {
        self.finish_attribute();
        if let Some(tag) = self.tag.take() {
            if self.end_tag {
                if !tag.attributes.is_empty() {
                    self.error_previous("end-tag-with-attributes");
                }
                if tag.self_closing {
                    self.error_previous("end-tag-with-trailing-solidus");
                }
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
            } else {
                self.error_at("duplicate-attribute", self.attribute_start);
            }
        }
    }
    fn start_attribute(&mut self) {
        self.finish_attribute();
        self.attribute_start = self.pos;
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
            State::TagOpen | State::EndTagOpen => self.error("eof-before-tag-name"),
            State::TagName
            | State::BeforeAttributeName
            | State::AttributeName
            | State::AfterAttributeName
            | State::BeforeAttributeValue
            | State::DoubleQuotedValue
            | State::SingleQuotedValue
            | State::UnquotedValue
            | State::AfterQuotedValue
            | State::SelfClosingStartTag => self.error("eof-in-tag"),
            State::CommentStart
            | State::CommentStartDash
            | State::Comment
            | State::CommentLess
            | State::CommentLessBang
            | State::CommentLessBangDash
            | State::CommentLessBangDashDash
            | State::CommentEndDash
            | State::CommentEnd
            | State::CommentEndBang => self.error("eof-in-comment"),
            State::BeforeDoctypeName
            | State::DoctypeName
            | State::AfterDoctypeName
            | State::AfterDoctypeKeyword
            | State::BeforeDoctypeIdentifier
            | State::DoctypeIdentifier(_)
            | State::AfterDoctypeIdentifier => self.error("eof-in-doctype"),
            State::CdataSection | State::CdataSectionBracket | State::CdataSectionEnd => {
                self.error("eof-in-cdata")
            }
            State::ScriptEscaped
            | State::ScriptEscapedDash
            | State::ScriptEscapedDashDash
            | State::ScriptDoubleEscaped
            | State::ScriptDoubleEscapedDash
            | State::ScriptDoubleEscapedDashDash => {
                self.error("eof-in-script-html-comment-like-text")
            }
            State::ProcessingInstructionOpen
            | State::ProcessingInstructionTarget
            | State::AfterProcessingInstructionTarget
            | State::ProcessingInstructionData
            | State::ProcessingInstructionQuestionable => {
                self.error("eof-in-processing-instruction")
            }
            _ => {}
        }
        match self.state {
            State::TagOpen => self.text.push('<'),
            State::EndTagOpen => self.text.push_str("</"),
            State::TextLess => self.text.push('<'),
            State::ScriptEscapedLess => self.text.push('<'),
            State::CdataSectionBracket => self.text.push(']'),
            State::CdataSectionEnd => self.text.push_str("]]"),
            State::TextEndName => {
                self.text.push_str("</");
                self.text.push_str(&self.text_end_name);
            }
            State::CommentStart
            | State::CommentStartDash
            | State::Comment
            | State::CommentLess
            | State::CommentLessBang
            | State::CommentLessBangDash
            | State::CommentLessBangDashDash
            | State::CommentEndDash
            | State::CommentEnd
            | State::CommentEndBang
            | State::BogusComment => {
                self.emit_comment();
            }
            State::BeforeDoctypeName
            | State::DoctypeName
            | State::AfterDoctypeName
            | State::AfterDoctypeKeyword
            | State::BeforeDoctypeIdentifier
            | State::DoctypeIdentifier(_) => {
                self.doctype_mut().force_quirks = true;
                self.emit_doctype();
            }
            State::AfterDoctypeIdentifier => {
                self.doctype_mut().force_quirks = true;
                self.emit_doctype();
            }
            State::BogusDoctype => self.emit_doctype(),
            State::ProcessingInstructionOpen
            | State::ProcessingInstructionTarget
            | State::AfterProcessingInstructionTarget
            | State::ProcessingInstructionData
            | State::ProcessingInstructionQuestionable => {}
            State::MarkupDeclarationOpen => {
                self.error("incorrectly-opened-comment");
                self.emit(Token::Comment(String::new()));
            }
            _ => {}
        }
        self.flush_text();
        self.ready.push_back(Token::Eof);
        self.ready_positions.push_back(self.previous_position());
        self.finished = true;
    }

    pub fn next_token(&mut self) -> Option<Token> {
        loop {
            if let Some(token) = self.ready.pop_front() {
                self.token_position = self.ready_positions.pop_front().unwrap();
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
        let start_pos = self.pos;
        let in_cdata = matches!(
            self.state,
            State::CdataSection | State::CdataSectionBracket | State::CdataSectionEnd
        );
        match self.state {
            State::Plaintext => {
                self.take();
                self.text.push(cleaned(c));
            }
            State::Data | State::Rcdata | State::Rawtext | State::ScriptData => match c {
                '<' => {
                    if self.state == State::Data
                        && !self.text.is_empty()
                        && self.text_boundary_position.is_none()
                    {
                        self.text_boundary_position = Some(self.previous_position());
                    }
                    self.take();
                    self.text_state = self.state;
                    self.state = if self.text_state == State::Data {
                        State::TagOpen
                    } else {
                        State::TextLess
                    };
                }
                '&' if matches!(self.state, State::Data | State::Rcdata) => {
                    if self.state == State::Data
                        && !self.text.is_empty()
                        && self.text_boundary_position.is_none()
                    {
                        self.text_boundary_position = Some(self.previous_position());
                    }
                    self.character_reference(false)
                }
                _ => {
                    self.take();
                    self.text.push(if self.state == State::Data {
                        c
                    } else {
                        cleaned(c)
                    });
                }
            },
            State::TextLess => {
                if c == '/' {
                    self.take();
                    self.text_end_name.clear();
                    self.state = State::TextEndName;
                } else if self.text_state == State::ScriptData && c == '!' {
                    self.take();
                    self.text.push_str("<!");
                    self.state = State::ScriptEscapeStart;
                } else {
                    self.text.push('<');
                    self.state = self.text_state;
                }
            }
            State::TextEndName => {
                if c.is_ascii_alphabetic() {
                    self.take();
                    self.text_end_name.push(c);
                } else if !self.text_end_name.is_empty()
                    && self
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
            State::ScriptEscapeStart => {
                if c == '-' {
                    self.take();
                    self.text.push('-');
                    self.state = State::ScriptEscapeStartDash;
                } else {
                    self.state = State::ScriptData;
                }
            }
            State::ScriptEscapeStartDash => {
                if c == '-' {
                    self.take();
                    self.text.push('-');
                    self.state = State::ScriptEscapedDashDash;
                } else {
                    self.state = State::ScriptData;
                }
            }
            State::ScriptEscaped => match c {
                '-' => {
                    self.take();
                    self.text.push('-');
                    self.state = State::ScriptEscapedDash;
                }
                '<' => {
                    self.take();
                    self.state = State::ScriptEscapedLess;
                }
                _ => {
                    self.take();
                    self.text.push(cleaned(c));
                }
            },
            State::ScriptEscapedDash => match c {
                '-' => {
                    self.take();
                    self.text.push('-');
                    self.state = State::ScriptEscapedDashDash;
                }
                '<' => {
                    self.take();
                    self.state = State::ScriptEscapedLess;
                }
                _ => {
                    self.state = State::ScriptEscaped;
                }
            },
            State::ScriptEscapedDashDash => match c {
                '-' => {
                    self.take();
                    self.text.push('-');
                }
                '<' => {
                    self.take();
                    self.state = State::ScriptEscapedLess;
                }
                '>' => {
                    self.take();
                    self.text.push('>');
                    self.state = State::ScriptData;
                }
                _ => {
                    self.state = State::ScriptEscaped;
                }
            },
            State::ScriptEscapedLess => {
                if c == '/' {
                    self.take();
                    self.text_end_name.clear();
                    self.text_state = State::ScriptEscaped;
                    self.state = State::TextEndName;
                } else if c.is_ascii_alphabetic() {
                    self.text.push('<');
                    self.script_escape_name.clear();
                    self.state = State::ScriptDoubleEscapeStart;
                } else {
                    self.text.push('<');
                    self.state = State::ScriptEscaped;
                }
            }
            State::ScriptDoubleEscapeStart | State::ScriptDoubleEscapeEnd => {
                if c.is_ascii_alphabetic() {
                    self.take();
                    self.script_escape_name.push(c.to_ascii_lowercase());
                    self.text.push(c);
                } else if space(c) || c == '/' || c == '>' {
                    self.take();
                    self.text.push(c);
                    self.state = if self.script_escape_name == "script" {
                        if self.state == State::ScriptDoubleEscapeStart {
                            State::ScriptDoubleEscaped
                        } else {
                            State::ScriptEscaped
                        }
                    } else if self.state == State::ScriptDoubleEscapeStart {
                        State::ScriptEscaped
                    } else {
                        State::ScriptDoubleEscaped
                    };
                } else {
                    self.state = if self.state == State::ScriptDoubleEscapeStart {
                        State::ScriptEscaped
                    } else {
                        State::ScriptDoubleEscaped
                    };
                }
            }
            State::ScriptDoubleEscaped => match c {
                '-' => {
                    self.take();
                    self.text.push('-');
                    self.state = State::ScriptDoubleEscapedDash;
                }
                '<' => {
                    self.take();
                    self.text.push('<');
                    self.state = State::ScriptDoubleEscapedLess;
                }
                _ => {
                    self.take();
                    self.text.push(cleaned(c));
                }
            },
            State::ScriptDoubleEscapedDash => match c {
                '-' => {
                    self.take();
                    self.text.push('-');
                    self.state = State::ScriptDoubleEscapedDashDash;
                }
                '<' => {
                    self.take();
                    self.text.push('<');
                    self.state = State::ScriptDoubleEscapedLess;
                }
                _ => self.state = State::ScriptDoubleEscaped,
            },
            State::ScriptDoubleEscapedDashDash => match c {
                '-' => {
                    self.take();
                    self.text.push('-');
                }
                '<' => {
                    self.take();
                    self.text.push('<');
                    self.state = State::ScriptDoubleEscapedLess;
                }
                '>' => {
                    self.take();
                    self.text.push('>');
                    self.state = State::ScriptData;
                }
                _ => self.state = State::ScriptDoubleEscaped,
            },
            State::ScriptDoubleEscapedLess => {
                if c == '/' {
                    self.take();
                    self.text.push('/');
                    self.script_escape_name.clear();
                    self.state = State::ScriptDoubleEscapeEnd;
                } else {
                    self.state = State::ScriptDoubleEscaped;
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
                    if !self.processing_instructions {
                        self.error("unexpected-question-mark-instead-of-tag-name");
                    }
                    self.take();
                    if self.processing_instructions {
                        self.pi_target.clear();
                        self.pi_data.clear();
                        self.state = State::ProcessingInstructionOpen;
                    } else {
                        self.comment.clear();
                        self.comment.push('?');
                        self.state = State::BogusComment;
                    }
                }
                _ if c.is_ascii_alphabetic() => self.start_tag(false),
                _ => {
                    self.error("invalid-first-character-of-tag-name");
                    self.text.push('<');
                    self.state = State::Data;
                }
            },
            State::ProcessingInstructionOpen => {
                if c.is_ascii_alphabetic() || c == '_' {
                    self.state = State::ProcessingInstructionTarget;
                } else {
                    self.error("invalid-first-character-of-processing-instruction-target");
                    self.pi_to_comment();
                }
            }
            State::ProcessingInstructionTarget => {
                if c.is_ascii_alphanumeric() || matches!(c, '-' | '_') {
                    self.take();
                    self.pi_target.push(c);
                } else if space(c) || matches!(c, '?' | '>') {
                    if matches!(
                        self.pi_target.to_ascii_lowercase().as_str(),
                        "xml" | "xml-stylesheet"
                    ) {
                        self.pi_to_comment();
                    } else {
                        self.state = State::AfterProcessingInstructionTarget;
                    }
                } else {
                    self.pi_to_comment();
                }
            }
            State::AfterProcessingInstructionTarget => {
                if space(c) {
                    self.take();
                } else {
                    self.state = State::ProcessingInstructionData;
                }
            }
            State::ProcessingInstructionData => match c {
                '?' => {
                    self.take();
                    self.state = State::ProcessingInstructionQuestionable;
                }
                '>' => {
                    self.take();
                    self.emit_processing_instruction();
                    self.state = State::Data;
                }
                _ => {
                    self.take();
                    self.pi_data.push(c);
                }
            },
            State::ProcessingInstructionQuestionable => {
                if c == '>' {
                    self.take();
                    self.emit_processing_instruction();
                    self.state = State::Data;
                } else {
                    self.pi_data.push('?');
                    self.state = State::ProcessingInstructionData;
                }
            }
            State::EndTagOpen => match c {
                '>' => {
                    self.error("missing-end-tag-name");
                    self.take();
                    self.state = State::Data;
                }
                _ if c.is_ascii_alphabetic() => self.start_tag(true),
                _ => {
                    self.error("invalid-first-character-of-tag-name");
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
                    self.error("unexpected-equals-sign-before-attribute-name");
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
                    if matches!(c, '"' | '\'' | '<') {
                        self.error("unexpected-character-in-attribute-name");
                    }
                    self.take();
                    self.attribute_name.push(lower(cleaned(c)));
                    self.attribute_start = self.pos;
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
                    self.error("missing-attribute-value");
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
                    if matches!(c, '"' | '\'' | '<' | '=' | '`') {
                        self.error("unexpected-character-in-unquoted-attribute-value");
                    }
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
                _ => {
                    self.error("missing-whitespace-between-attributes");
                    self.state = State::BeforeAttributeName;
                }
            },
            State::SelfClosingStartTag => {
                if c == '>' {
                    self.take();
                    self.tag.as_mut().unwrap().self_closing = true;
                    self.emit_tag();
                } else {
                    self.error("unexpected-solidus-in-tag");
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
                    self.doctype_space_seen = false;
                    self.doctype = Some(Doctype {
                        name: None,
                        public_id: None,
                        system_id: None,
                        force_quirks: false,
                    });
                    self.state = State::BeforeDoctypeName;
                } else if self.starts("[CDATA[") && self.cdata_allowed {
                    self.skip(7);
                    self.state = State::CdataSection;
                } else {
                    if self.starts("[CDATA[") {
                        self.error_at("cdata-in-html-content", self.pos + 6);
                    } else {
                        self.error("incorrectly-opened-comment");
                    }
                    self.comment.clear();
                    self.state = State::BogusComment;
                }
            }
            State::CdataSection => {
                self.take();
                if c == ']' {
                    self.state = State::CdataSectionBracket;
                } else {
                    self.text.push(c);
                }
            }
            State::CdataSectionBracket => {
                self.take();
                if c == ']' {
                    self.state = State::CdataSectionEnd;
                } else {
                    self.text.push(']');
                    self.text.push(c);
                    self.state = State::CdataSection;
                }
            }
            State::CdataSectionEnd => {
                self.take();
                match c {
                    ']' => self.text.push(']'),
                    '>' => self.state = State::Data,
                    _ => {
                        self.text.push_str("]]");
                        self.text.push(c);
                        self.state = State::CdataSection;
                    }
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
                    self.error("abrupt-closing-of-empty-comment");
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
                    self.error("abrupt-closing-of-empty-comment");
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
                '<' => {
                    self.take();
                    self.comment.push('<');
                    self.state = State::CommentLess;
                }
                '-' => {
                    self.take();
                    self.state = State::CommentEndDash;
                }
                _ => {
                    self.take();
                    self.comment.push(cleaned(c));
                }
            },
            State::CommentLess => match c {
                '!' => {
                    self.take();
                    self.comment.push('!');
                    self.state = State::CommentLessBang;
                }
                '<' => {
                    self.take();
                    self.comment.push('<');
                }
                _ => self.state = State::Comment,
            },
            State::CommentLessBang => {
                if c == '-' {
                    self.take();
                    self.state = State::CommentLessBangDash;
                } else {
                    self.state = State::Comment;
                }
            }
            State::CommentLessBangDash => {
                if c == '-' {
                    self.take();
                    self.state = State::CommentLessBangDashDash;
                } else {
                    self.state = State::CommentEndDash;
                }
            }
            State::CommentLessBangDashDash => {
                if c != '>' {
                    self.error("nested-comment");
                }
                self.state = State::CommentEnd;
            }
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
                    self.error("incorrectly-closed-comment");
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
                    self.doctype_space_seen = true;
                    self.take();
                }
                '>' => {
                    self.error("missing-doctype-name");
                    self.take();
                    self.doctype_mut().force_quirks = true;
                    self.emit_doctype();
                }
                _ => {
                    if !self.doctype_space_seen {
                        self.error("missing-whitespace-before-doctype-name");
                    }
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
                    self.error("invalid-character-sequence-after-doctype-name");
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
                    self.error(if self.identifier_public {
                        "missing-whitespace-after-doctype-public-keyword"
                    } else {
                        "missing-whitespace-after-doctype-system-keyword"
                    });
                    self.state = State::BeforeDoctypeIdentifier;
                }
                '>' => {
                    self.error(if self.identifier_public {
                        "missing-doctype-public-identifier"
                    } else {
                        "missing-doctype-system-identifier"
                    });
                    self.take();
                    self.doctype_mut().force_quirks = true;
                    self.emit_doctype();
                }
                _ => {
                    self.error(if self.identifier_public {
                        "missing-quote-before-doctype-public-identifier"
                    } else {
                        "missing-quote-before-doctype-system-identifier"
                    });
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
                    self.doctype_identifier_space_seen = false;
                    self.state = State::DoctypeIdentifier(c);
                }
                '>' => {
                    self.error(if self.identifier_public {
                        "missing-doctype-public-identifier"
                    } else {
                        "missing-doctype-system-identifier"
                    });
                    self.take();
                    self.doctype_mut().force_quirks = true;
                    self.emit_doctype();
                }
                _ => {
                    self.error(if self.identifier_public {
                        "missing-quote-before-doctype-public-identifier"
                    } else {
                        "missing-quote-before-doctype-system-identifier"
                    });
                    self.doctype_mut().force_quirks = true;
                    self.state = State::BogusDoctype;
                }
            },
            State::DoctypeIdentifier(quote) => {
                if c == quote {
                    self.take();
                    self.state = State::AfterDoctypeIdentifier;
                } else if c == '>' {
                    self.error(if self.identifier_public {
                        "abrupt-doctype-public-identifier"
                    } else {
                        "abrupt-doctype-system-identifier"
                    });
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
                    self.doctype_identifier_space_seen = true;
                    self.take();
                } else if c == '>' {
                    self.take();
                    self.emit_doctype();
                } else if self.identifier_public && matches!(c, '"' | '\'') {
                    if !self.doctype_identifier_space_seen {
                        self.error(
                            "missing-whitespace-between-doctype-public-and-system-identifiers",
                        );
                    }
                    self.take();
                    self.identifier_public = false;
                    self.doctype_mut().system_id = Some(String::new());
                    self.state = State::DoctypeIdentifier(c);
                } else {
                    self.error(if self.identifier_public {
                        "missing-quote-before-doctype-system-identifier"
                    } else {
                        "unexpected-character-after-doctype-system-identifier"
                    });
                    if self.identifier_public {
                        self.doctype_mut().force_quirks = true;
                    }
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
        if c == '\0' && !in_cdata && self.last_null_reported != Some(start_pos) {
            self.error_at("unexpected-null-character", start_pos);
            self.last_null_reported = Some(start_pos);
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
                } else {
                    self.error("missing-semicolon-after-character-reference");
                }
                if value == 0 {
                    self.error("null-character-reference");
                } else if value > 0x10ffff {
                    self.error("character-reference-outside-unicode-range");
                } else if (0xd800..=0xdfff).contains(&value) {
                    self.error("surrogate-character-reference");
                } else if char::from_u32(value).is_some_and(is_noncharacter) {
                    self.error("noncharacter-character-reference");
                } else if (0x01..=0x08).contains(&value)
                    || matches!(value, 0x0b | 0x0d | 0x0e..=0x1f | 0x7f..=0x9f)
                {
                    self.error("control-character-reference");
                }
                replacement = Some(numeric_reference(value).to_string());
            } else {
                self.error("absence-of-digits-in-numeric-character-reference");
                self.pos = start - if radix == 16 { 2 } else { 1 };
            }
        } else {
            let mut candidate = String::new();
            let mut best = None;
            for &next in self.input[self.pos..]
                .iter()
                .take(MAX_NAMED_REFERENCE_LENGTH)
            {
                if !next.is_ascii_alphanumeric() && next != ';' {
                    break;
                }
                candidate.push(next);
                if let Ok(index) =
                    NAMED_REFERENCES.binary_search_by_key(&candidate.as_str(), |(name, _)| name)
                {
                    best = Some((candidate.len(), NAMED_REFERENCES[index].1));
                }
            }
            if let Some((length, value)) = best {
                let following = self.input.get(self.pos + length);
                let semicolonless = self.input[self.pos + length - 1] != ';';
                if !(attribute
                    && semicolonless
                    && following.is_some_and(|c| c.is_ascii_alphanumeric() || *c == '='))
                {
                    self.skip(length);
                    if semicolonless {
                        self.error("missing-semicolon-after-character-reference");
                    }
                    replacement = Some(value.to_owned());
                }
            } else {
                let semicolon = self.input[self.pos..]
                    .iter()
                    .position(|c| *c == ';' || !c.is_ascii_alphanumeric());
                if let Some(length) = semicolon
                    && length > 0
                    && self.input[self.pos + length] == ';'
                {
                    self.error_at("unknown-named-character-reference", self.pos + length);
                }
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
