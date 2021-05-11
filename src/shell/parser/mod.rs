mod token;
mod parse;
mod highlight;
#[path = "type.rs"] pub mod type_;

#[cfg(test)]
mod tests;

use std::fmt;
pub use token::Token;
pub use parse::parse_in;
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
    UnexpectedArgument
}

impl ErrorKind {
    pub fn as_str(self) -> &'static str {
        const STRINGS: &[&str] = &[
            "The first argument must be a literal",
            "Command was empty",
            "Unexpected token",
            "Unexpected close token",
            "Unsupported redirect type",
            "Unclosed subshell",
            "Unclosed single quote",
            "Unclosed double quote",
            "Redirect has no target",
            "Unexpected argument"
        ];

        STRINGS[self as usize]
    }
}

impl fmt::Display for ErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}
