use std::fmt;
use logos::Span;
use super::token::Token;


#[derive(Debug)]
pub struct ParseFailed {
    pub token: Option<Token>,
    pub span: Option<Span>,
    pub kind: ErrorKind,
}

#[derive(Debug, PartialEq, Clone, Default)]
pub struct LexingError;

#[derive(Debug, PartialEq, Copy, Clone)]
#[non_exhaustive]
pub enum ErrorKind {
    FirstArgMustLiteral = 0,
    EmptyCommand,
    UnexpectedToken,
    UnexpectedClose,
    UnexpectedArgument,
    UnsupportedRedirectType,
    UnclosedSubShell,
    UnclosedSingleQuote,
    UnclosedDoubleQuote,
    RedirectNoTarget,
    IncompleteEscape,
    UnknownEscape,
    InvalidToken,
    Unreachable
}

impl ParseFailed {
    pub const fn with_kind(mut self, kind: ErrorKind) -> ParseFailed {
        self.kind = kind;
        self
    }
}

impl ErrorKind {
    pub fn as_str(self) -> &'static str {
        const STRINGS: &[&str] = &[
            "the first argument must be a literal",
            "command was empty",
            "unexpected token",
            "unexpected close token",
            "unsupported redirect type",
            "unclosed subshell",
            "unclosed single quote",
            "unclosed double quote",
            "redirect has no target",
            "unexpected argument",
            "incomplete escape",
            "unknown character escape"
        ];

        STRINGS[self as usize]
    }
}

impl fmt::Display for ErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}
