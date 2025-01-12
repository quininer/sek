use std::fmt;
use logos::Span;
use annotate_snippets::{ Level, Snippet, Message };
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
    IncompleteEscape,
    UnknownEscape,
    UnknownRedirect,
    InvalidToken,
    Unreachable
}

impl ParseFailed {
    pub const fn with_kind(mut self, kind: ErrorKind) -> ParseFailed {
        self.kind = kind;
        self
    }

    pub fn to_message<'a>(&self, input: &'a str) -> Message<'a> {
        let span = self.span.clone().unwrap_or_else(|| 0..input.len());
        
        Level::Error
            .title("Syntax error")
            .snippet(Snippet::source(input)
                .fold(true)
                .annotation(Level::Error
                    .span(span)
                    .label(self.kind.as_str())
                )
            )
    }
}

impl ErrorKind {
    pub fn as_str(self) -> &'static str {
        const STRINGS: &[&str] = &[
            "the first argument must be a literal",
            "command was empty",
            "unexpected token",
            "unexpected close token",
            "unexpected argument",
            "incomplete escape",
            "unknown character escape",
            "unknown redirect target",
            "invalid token",
        ];

        STRINGS[self as usize]
    }
}

impl fmt::Display for ErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}
