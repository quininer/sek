use std::fmt;
use logos::Span;
use annotate_snippets::{ Level, Snippet, Group, AnnotationKind };
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
    EmptyCommand = 0,
    UnexpectedToken,
    UnexpectedClose,
    UnknownRedirect,
    IncompleteEscape,
    InvalidToken,
    ExpectedClose,
    ExpectedVariableName,
    Unreachable
}

impl ParseFailed {
    pub const fn with_kind(mut self, kind: ErrorKind) -> ParseFailed {
        self.kind = kind;
        self
    }

    pub fn to_message<'a>(&self, input: &'a str) -> Group<'a> {
        let span = self.span.clone().unwrap_or(0..input.len());
        
        Level::ERROR
            .primary_title("Syntax error")
            .element(Snippet::source(input)
                .fold(true)
                .annotation(AnnotationKind::Primary
                    .span(span)
                    .label(self.kind.as_str())
                )
            )
    }
}

impl ErrorKind {
    pub fn as_str(self) -> &'static str {
        const STRINGS: &[&str] = &[
            "command was empty",
            "unexpected token",
            "unexpected close token",
            "unknown redirect target",
            "incomplete escape",
            "invalid token",
            "expected close token",
            "expected variable name",
        ];

        STRINGS[self as usize]
    }
}

impl fmt::Display for ErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}
