mod token;
mod parse;
mod highlight;
#[path = "type.rs"] pub mod type_;

#[cfg(test)]
mod tests;

use std::fmt;
pub use token::Token;
pub use parse::{ parse_in, ParseFailed };
pub use highlight::colour;


#[derive(Debug, PartialEq, Copy, Clone)]
#[non_exhaustive]
pub enum ErrorKind {
    FirstArgMustLiteral = 0,
    EmptyCommand,
    UnexpectedToken,
    UnexpectedClose,
    UnsupportedRedirectType,
    UnclosedSubShell,
    UnclosedSingleQuote,
    UnclosedDoubleQuote,
    RedirectNoTarget,
    UnexpectedArgument,
    IncompleteEscape
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
            "incomplete escape"
        ];

        STRINGS[self as usize]
    }
}

impl fmt::Display for ErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}
