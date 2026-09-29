/// Position in the newline-normalized input stream. Offset counts Unicode
/// scalar values from zero; line counts from one and column counts UTF-16 code
/// units from one, matching the pinned html5lib and WPT fixture convention.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SourcePosition {
    pub offset: usize,
    pub line: usize,
    pub column: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorPhase {
    Input,
    Tokenizer,
    TreeConstruction,
}

/// Stable, kebab-case HTML Standard code (or a documented tree recovery code).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError {
    pub code: &'static str,
    pub position: SourcePosition,
    pub phase: ErrorPhase,
}
