use logos::{ Logos, Span };
use super::error::LexingError;
use crate::util::arena;

pub type TokenItem = (Token, Span);
pub type TokenId = arena::Id<TokenItem>;

#[derive(Logos, Debug, PartialEq, Eq, Copy, Clone)]
#[logos(error = LexingError)]
pub enum Token {
    #[token("'")]
    SingleQuote,
    #[token("\"")]
    DoubleQuote,

    #[token(r"$(")]
    ShellOpen,
    #[token(r")")]
    ShellClose,

    #[token("#")]
    Comment,

    #[token(r"\")]
    Backslash,

    #[regex(r"[12*]?\|")]
    Pipe,
    #[token(";")]
    Then,
    #[token("&&")]
    AndIf,
    #[token("||")]
    OrIf,

    #[regex(r"[12*]?>>?")]
    Redirect,

    #[regex(r"\$[\w]+")]
    Variable,

    #[regex(r#"[^#)|\\;"'<>\s]+"#)]
    Text,

    #[regex(r"[\s]+")]
    Empty,
}

impl Token {
    pub const fn size() -> usize {
        const TOKEN_KIND: &[Token; 14] = &[
            Token::SingleQuote,
            Token::DoubleQuote,
            Token::ShellOpen,
            Token::ShellClose,
            Token::Comment,
            Token::Backslash,
            Token::Pipe,
            Token::Then,
            Token::AndIf,
            Token::OrIf,
            Token::Redirect,
            Token::Variable,
            Token::Text,
            Token::Empty,
        ];

        TOKEN_KIND.len()
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_simple_token() {
        let input = "exe --args foo --args2 中文!";

        let result = Token::lexer(input)
            .spanned()
            .filter(|(token, _)| token != &Ok(Token::Empty))
            .map(|(token, span)| (token, &input[span]))
            .collect::<std::vec::Vec<_>>();

        let expected = vec![
            (Ok(Token::Text), "exe"),
            (Ok(Token::Text), "--args"),
            (Ok(Token::Text), "foo"),
            (Ok(Token::Text), "--args2"),
            (Ok(Token::Text), "中文!"),
        ];

        assert_eq!(expected, result);
    }

    #[test]
    fn test_pipe_and_more_token() {
        let input = "exe 2> fd0 >> fd1 | exe2 > fd2 2>> fd3; exe3";

        let result = Token::lexer(input)
            .spanned()
            .filter(|(token, _)| token != &Ok(Token::Empty))
            .map(|(token, span)| (token, &input[span]))
            .collect::<std::vec::Vec<_>>();

        let expected = vec![
            (Ok(Token::Text), "exe"),
            (Ok(Token::Redirect), "2>"),
            (Ok(Token::Text), "fd0"),
            (Ok(Token::Redirect), ">>"),
            (Ok(Token::Text), "fd1"),
            (Ok(Token::Pipe), "|"),
            (Ok(Token::Text), "exe2"),
            (Ok(Token::Redirect), ">"),
            (Ok(Token::Text), "fd2"),
            (Ok(Token::Redirect), "2>>"),
            (Ok(Token::Text), "fd3"),
            (Ok(Token::Then), ";"),
            (Ok(Token::Text), "exe3")
        ];

        assert_eq!(expected, result);
    }

    #[test]
    fn test_pipe_12star_kind() {
        let input = "exe | exe1 1| exe2 2| exe3 *| exe4";

        let result = Token::lexer(input)
            .spanned()
            .filter(|(token, _)| token != &Ok(Token::Empty))
            .map(|(token, span)| (token, &input[span]))
            .collect::<std::vec::Vec<_>>();

        let expected = vec![
            (Ok(Token::Text), "exe"),
            (Ok(Token::Pipe), "|"),
            (Ok(Token::Text), "exe1"),
            (Ok(Token::Pipe), "1|"),
            (Ok(Token::Text), "exe2"),
            (Ok(Token::Pipe), "2|"),
            (Ok(Token::Text), "exe3"),
            (Ok(Token::Pipe), "*|"),
            (Ok(Token::Text), "exe4")
        ];

        assert_eq!(expected, result);
    }

    #[test]
    fn test_string_and_env_token() {
        let input = r#"$CC "aaa'$中文'bbb\"ccc""#;

        let result = Token::lexer(input)
            .spanned()
            .filter(|(token, _)| token != &Ok(Token::Empty))
            .map(|(token, span)| (token, &input[span]))
            .collect::<std::vec::Vec<_>>();

        let expected = vec![
            (Ok(Token::Variable), "$CC"),
            (Ok(Token::DoubleQuote), "\""),
            (Ok(Token::Text), "aaa"),
            (Ok(Token::SingleQuote), "'"),
            (Ok(Token::Variable), "$中文"),
            (Ok(Token::SingleQuote), "'"),
            (Ok(Token::Text), "bbb"),
            (Ok(Token::Backslash), "\\"),
            (Ok(Token::DoubleQuote), "\""),
            (Ok(Token::Text), "ccc"),
            (Ok(Token::DoubleQuote), "\"")
        ];

        assert_eq!(expected, result);
    }

    #[test]
    fn test_subshell_token() {
        let input = r#"exe "$(exe2 --args "$ENV" | exe3)""#;

        let result = Token::lexer(input)
            .spanned()
            .filter(|(token, _)| token != &Ok(Token::Empty))
            .map(|(token, span)| (token, &input[span]))
            .collect::<std::vec::Vec<_>>();

        let expected = vec![
            (Ok(Token::Text), "exe"),
            (Ok(Token::DoubleQuote), "\""),
            (Ok(Token::ShellOpen), "$("),
            (Ok(Token::Text), "exe2"),
            (Ok(Token::Text), "--args"),
            (Ok(Token::DoubleQuote), "\""),
            (Ok(Token::Variable), "$ENV"),
            (Ok(Token::DoubleQuote), "\""),
            (Ok(Token::Pipe), "|"),
            (Ok(Token::Text), "exe3"),
            (Ok(Token::ShellClose), ")"),
            (Ok(Token::DoubleQuote), "\"")
        ];

        assert_eq!(expected, result);
    }

    #[test]
    fn test_bad_arrow_token() {
        let input = r#"exe >>>> fd"#;

        let result = Token::lexer(input)
            .spanned()
            .filter(|(token, _)| token != &Ok(Token::Empty))
            .map(|(token, span)| (token, &input[span]))
            .collect::<std::vec::Vec<_>>();

        let expected = vec![
            (Ok(Token::Text), "exe"),
            (Ok(Token::Redirect), ">>"),
            (Ok(Token::Redirect), ">>"),
            (Ok(Token::Text), "fd"),
        ];

        assert_eq!(expected, result);
    }

    #[test]
    fn test_path_token() {
        let input = r#"exe $HOME/path/foo"#;

        let result = Token::lexer(input)
            .spanned()
            .map(|(token, span)| (token, &input[span]))
            .collect::<std::vec::Vec<_>>();

        let expected = vec![
            (Ok(Token::Text), "exe"),
            (Ok(Token::Empty), " "),
            (Ok(Token::Variable), "$HOME"),
            (Ok(Token::Text), "/path/foo"),
        ];

        assert_eq!(expected, result);
    }
}
