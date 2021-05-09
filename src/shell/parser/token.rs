use logos::{ Logos, Source, Span };


pub const TOKEN_KIND: &[Token; 15] = &[
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
    Token::Env,
    Token::Text,
    Token::Empty,
    Token::Error
];

#[derive(Logos, Debug, PartialEq, Copy, Clone)]
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

    #[token("|")]
    Pipe,
    #[token(";")]
    Then,
    #[token("&&")]
    AndIf,
    #[token("||")]
    OrIf,

    #[regex(r"[\d]?>>?")]
    Redirect,

    #[regex(r"\$[\w]+")]
    Env,

    #[regex(r#"[^#$&()|\\;"'<>\s]+"#)]
    Text,

    #[regex(r"[\s]+")]
    Empty,

    #[error]
    Error
}

#[cfg(test)]
mod test {
    use bstr::ByteSlice;
    use logos::Lexer;
    use super::*;

    #[test]
    fn test_simple_token() {
        let input = "exe --args foo --args2 中文!";

        let result = Token::lexer(input)
            .spanned()
            .filter(|(token, _)| token != &Token::Empty)
            .map(|(token, span)| (token, &input[span]))
            .collect::<std::vec::Vec<_>>();

        let expected = vec![
            (Token::Text, "exe"),
            (Token::Text, "--args"),
            (Token::Text, "foo"),
            (Token::Text, "--args2"),
            (Token::Text, "中文!"),
        ];

        assert_eq!(expected, result);
    }

    #[test]
    fn test_pipe_and_more_token() {
        let input = "exe 2> fd0 >> fd1 | exe2 > fd2 2>> fd3; exe3";

        let result = Token::lexer(input)
            .spanned()
            .filter(|(token, _)| token != &Token::Empty)
            .map(|(token, span)| (token, &input[span]))
            .collect::<std::vec::Vec<_>>();

        let expected = vec![
            (Token::Text, "exe"),
            (Token::Redirect, "2>"),
            (Token::Text, "fd0"),
            (Token::Redirect, ">>"),
            (Token::Text, "fd1"),
            (Token::Pipe, "|"),
            (Token::Text, "exe2"),
            (Token::Redirect, ">"),
            (Token::Text, "fd2"),
            (Token::Redirect, "2>>"),
            (Token::Text, "fd3"),
            (Token::Then, ";"),
            (Token::Text, "exe3")
        ];

        assert_eq!(expected, result);
    }

    #[test]
    fn test_string_and_env_token() {
        let input = r#"$CC "aaa'$中文'bbb\"ccc""#;

        let result = Token::lexer(input)
            .spanned()
            .filter(|(token, _)| token != &Token::Empty)
            .map(|(token, span)| (token, &input[span]))
            .collect::<std::vec::Vec<_>>();

        let expected = vec![
            (Token::Env, "$CC"),
            (Token::DoubleQuote, "\""),
            (Token::Text, "aaa"),
            (Token::SingleQuote, "'"),
            (Token::Env, "$中文"),
            (Token::SingleQuote, "'"),
            (Token::Text, "bbb"),
            (Token::Backslash, "\\"),
            (Token::DoubleQuote, "\""),
            (Token::Text, "ccc"),
            (Token::DoubleQuote, "\"")
        ];

        assert_eq!(expected, result);
    }

    #[test]
    fn test_subshell_token() {
        let input = r#"exe "$(exe2 --args "$ENV" | exe3)""#;

        let result = Token::lexer(input)
            .spanned()
            .filter(|(token, _)| token != &Token::Empty)
            .map(|(token, span)| (token, &input[span]))
            .collect::<std::vec::Vec<_>>();

        let expected = vec![
            (Token::Text, "exe"),
            (Token::DoubleQuote, "\""),
            (Token::ShellOpen, "$("),
            (Token::Text, "exe2"),
            (Token::Text, "--args"),
            (Token::DoubleQuote, "\""),
            (Token::Env, "$ENV"),
            (Token::DoubleQuote, "\""),
            (Token::Pipe, "|"),
            (Token::Text, "exe3"),
            (Token::ShellClose, ")"),
            (Token::DoubleQuote, "\"")
        ];

        assert_eq!(expected, result);
    }

    #[test]
    fn test_bad_arrow_token() {
        let input = r#"exe >>>> fd"#;

        let result = Token::lexer(input)
            .spanned()
            .filter(|(token, _)| token != &Token::Empty)
            .map(|(token, span)| (token, &input[span]))
            .collect::<std::vec::Vec<_>>();

        let expected = vec![
            (Token::Text, "exe"),
            (Token::Redirect, ">>"),
            (Token::Redirect, ">>"),
            (Token::Text, "fd"),
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
            (Token::Text, "exe"),
            (Token::Empty, ""),
            (Token::Env, "$HOME"),
            (Token::Text, "/path/foo"),
        ];

        assert_eq!(expected, result);
    }
}
