use bumpalo::Bump;
use crate::shell::parser::parse_in;
use crate::shell::parser::type_::*;


#[test]
fn test_parse_command() -> anyhow::Result<()> {
    use Output::*;
    use Kind::*;

    let mut bump = Bump::new();

    // simple
    bump.reset();
    {
        let input = "exe 123";
        let cmd = parse_in(&bump, input).unwrap();
        let output = cmd.fix(input);

        assert_eq!(output, Vec(Kind::Command, vec![
            Item(Literal, "exe".into()),
            Vec(Argument, vec![Item(Literal, "123".into())])
        ]));
    }

    // subshell
    bump.reset();
    {
        let input = r#"exe $(exe2 hello world) "$(exe3)""#;
        let cmd = parse_in(&bump, input).unwrap();
        let output = cmd.fix(input);

        assert_eq!(output, Vec(Command, vec![
            Item(Literal, "exe".into()),
            Vec(Argument, vec![One(
                SubShell,
                Box::new(Vec(Command, vec![
                    Item(Literal, "exe2".into()),
                    Vec(Argument, vec![Item(Literal, "hello".into())]),
                    Vec(Argument, vec![Item(Literal, "world".into())])
                ])),
            )]),
            Vec(Argument, vec![Vec(
                DoubleStr,
                vec![One(SubShell,
                    Box::new(Vec(Command,vec![Item(Literal, "exe3".into())]))
                )]
            )]),
        ]));
    }

    // env
    bump.reset();
    {
        let input = r#"exe $EXE $HOME/.config "hello$EXE3 world" $(exe2 $EXE4 "$EXE5"a)"#;
        let cmd = parse_in(&bump, input).unwrap();
        let output = cmd.fix(input);

        assert_eq!(output, Vec(Command, vec![
            Item(Literal, "exe".into()),
            Vec(Argument, vec![Item(Env, "$EXE".into())]),
            Vec(Argument, vec![
                Item(Env, "$HOME".into()),
                Item(Literal, "/.config".into()),
            ]),
            Vec(Argument, vec![Vec(DoubleStr, vec![
                Item(Literal, "hello".into()),
                Item(Env, "$EXE3".into()),
                Item(Literal, " world".into())
            ])]),
            Vec(Argument, vec![One(SubShell, Box::new(Vec(Command, vec![
                Item(Literal, "exe2".into()),
                Vec(Argument, vec![Item(Env, "$EXE4".into())]),
                Vec(Argument, vec![
                    Vec(DoubleStr, vec![Item(Env, "$EXE5".into())]),
                    Item(Literal, "a".into())
                ])
            ])))])
        ]));
    }

    // double string
    bump.reset();
    {
        let input = r#"exe a"b"c""d "#;
        let cmd = parse_in(&bump, input).unwrap();
        let output = cmd.fix(input);

        assert_eq!(output, Vec(Command, vec![
            Item(Literal, "exe".into()),
            Vec(Argument, vec![
                Item(Literal, "a".into()),
                Vec(DoubleStr, vec![Item(Literal, "b".into())]),
                Item(Literal, "c".into()),
                Vec(DoubleStr, vec![]),
                Item(Literal, "d".into())
            ])
        ]));
    }

    // single string
    bump.reset();
    {
        let input = r#"exe '\' '>> #$()'"#;
        let cmd = parse_in(&bump, input).unwrap();
        let output = cmd.fix(input);

        assert_eq!(output, Vec(Command, vec![
            Item(Literal, "exe".into()),
            Vec(Argument, vec![Item(SingleStr, "'\\'".into())]),
            Vec(Argument, vec![Item(SingleStr, "'>> #$()'".into())])
        ]));
    }

    // pipe
    bump.reset();
    {
        let input = r#"exe | exe2 a | exe3 $(exe4 b) && exe5"#;
        let cmd = parse_in(&bump, input).unwrap();
        let output = cmd.fix(input);

        assert_eq!(output, Vec(Command, vec![
            Item(Literal, "exe".into()),
            One(Pipe, Box::new(Vec(Command, vec![
                Item(Literal, "exe2".into()),
                Vec(Argument, vec![Item(Literal, "a".into())]),
                One(Pipe, Box::new(Vec(Command, vec![
                    Item(Literal, "exe3".into()),
                    Vec(Argument, vec![One(SubShell, Box::new(Vec(Command, vec![
                        Item(Literal, "exe4".into()),
                        Vec(Argument, vec![Item(Literal, "b".into())])
                    ])))]),
                    One(AndIf, Box::new(Vec(Command, vec![
                        Item(Literal, "exe5".into())
                    ])))
                ])))
            ])))
        ]));
    }

    // redirect
    bump.reset();
    {
        let input = r#"exe >a 2>> b 2>        c 1>> d"#;
        let cmd = parse_in(&bump, input).unwrap();
        let output = cmd.fix(input);

        assert_eq!(output, Vec(Command, vec![
            Item(Literal, "exe".into()),
            One(RedirectOut, Box::new(Vec(Argument, vec![
                Item(Literal, "a".into())
            ]))),
            One(RedirectErrAppend, Box::new(Vec(Argument, vec![
                Item(Literal, "b".into())
            ]))),
            One(RedirectErr, Box::new(Vec(Argument, vec![
                Item(Literal, "c".into())
            ]))),
            One(RedirectOutAppend, Box::new(Vec(Argument, vec![
                Item(Literal, "d".into())
            ])))
        ]));
    }

    // backslash
    bump.reset();
    {
        let input = r#"exe a\ b\\ \c '\' "\"" "\ " \>>"#;
        let cmd = parse_in(&bump, input).unwrap();
        let output = cmd.fix(input);

        assert_eq!(output, Vec(Command, vec![
            Item(Literal, "exe".into()),
            Vec(Argument, vec![
                Item(Literal, "a".into()),
                Item(Escape, "\\ ".into()),
                Item(Literal, "b".into()),
                Item(Escape, "\\\\".into())
            ]),
            Vec(Argument, vec![
                Item(Escape, "\\c".into())
            ]),
            Vec(Argument, vec![
                Item(SingleStr, "'\\'".into())
            ]),
            Vec(Argument, vec![
                Vec(DoubleStr, vec![Item(Escape, "\\\"".into())])
            ]),
            Vec(Argument, vec![
                Vec(DoubleStr, vec![Item(Escape, "\\ ".into())])
            ]),
            Vec(Argument, vec![Item(Escape, "\\>>".into())])
        ]));
    }

    dbg!();

    // subshell and empty
    bump.reset();
    {
        let input = r#"exe $(exe2 > fd) "#;
        let cmd = parse_in(&bump, input).unwrap();
        let output = cmd.fix(input);

        assert_eq!(output, Vec(Command, vec![
            Item(Literal, "exe".into()),
            Vec(Argument, vec![
                One(SubShell, Box::new(Vec(Command, vec![
                    Item(Literal, "exe2".into()),
                    One(RedirectOut, Box::new(Vec(Argument, vec![
                        Item(Literal, "fd".into())
                    ])))
                ])))
            ])
        ]));
    }

    // redirect and subshell
    bump.reset();
    {
        let input = r#"exe > $(exe1 2> fd1) | exe2 $(exe3 2>> fd4 | exe4 > fd5) > fd6"#;
        let cmd = parse_in(&bump, input).unwrap();
        let output = cmd.fix(input);

        assert_eq!(output, Vec(Command, vec![
            Item(Literal, "exe".into()),
            One(RedirectOut, Box::new(Vec(Argument, vec![
                One(SubShell, Box::new(Vec(Command, vec![
                    Item(Literal, "exe1".into()),
                    One(RedirectErr, Box::new(Vec(Argument, vec![
                        Item(Literal, "fd1".into())
                    ])))
                ])))
            ]))),
            One(Pipe, Box::new(Vec(Command, vec![
                Item(Literal, "exe2".into()),
                Vec(Argument, vec![
                    One(SubShell, Box::new(Vec(Command, vec![
                        Item(Literal, "exe3".into()),
                        One(RedirectErrAppend, Box::new(Vec(Argument, vec![
                            Item(Literal, "fd4".into())
                        ]))),
                        One(Pipe, Box::new(Vec(Command, vec![
                            Item(Literal, "exe4".into()),
                            One(RedirectOut, Box::new(Vec(Argument, vec![
                                Item(Literal, "fd5".into())
                            ])))
                        ]))),
                    ])))
                ]),
                One(RedirectOut, Box::new(Vec(Argument, vec![
                    Item(Literal, "fd6".into())
                ])))
            ]))),
        ]));
    }

    Ok(())
}

#[test]
fn test_bad_command() -> anyhow::Result<()> {
    use crate::shell::parser::{ ErrorKind, Token };
    use Output::*;
    use Kind::*;

    let mut bump = Bump::new();

    // no literal command
    bump.reset();
    {
        let input = "$CC ab.c";
        let err = parse_in(&bump, input).unwrap_err();
        assert_eq!(err.token, Token::Env);
        assert_eq!(&input[err.span], "$CC");
        assert_eq!(err.kind, ErrorKind::FirstArgMustLiteral);
    }

    // empty
    bump.reset();
    {
        let input = " ";
        let err = parse_in(&bump, input).unwrap_err();
        assert_eq!(err.token, Token::Empty);
        assert_eq!(&input[err.span], " ");
        assert_eq!(err.kind, ErrorKind::EmptyCommand);
    }

    // unexpected close
    bump.reset();
    {
        let input = "exe )";
        let err = parse_in(&bump, input).unwrap_err();
        assert_eq!(err.token, Token::ShellClose);
        assert_eq!(&input[err.span], ")");
        assert_eq!(err.kind, ErrorKind::UnexpectedClose);
    }

    // unsupported redirect type
    bump.reset();
    {
        let input = "exe 3> a";
        let err = parse_in(&bump, input).unwrap_err();
        assert_eq!(err.token, Token::Redirect);
        assert_eq!(&input[err.span], "3>");
        assert_eq!(err.kind, ErrorKind::UnsupportedRedirectType);
    }

    // unclosed subshell
    bump.reset();
    {
        let input = "exe $(exe3";
        let err = parse_in(&bump, input).unwrap_err();
        assert_eq!(err.token, Token::ShellOpen);
        assert_eq!(&input[err.span], "$(");
        assert_eq!(err.kind, ErrorKind::UnclosedSubShell);
    }

    // unclosed single quote
    bump.reset();
    {
        let input = "exe 'text";
        let err = parse_in(&bump, input).unwrap_err();
        assert_eq!(err.token, Token::SingleQuote);
        assert_eq!(&input[err.span], "'");
        assert_eq!(err.kind, ErrorKind::UnclosedSingleQuote);
    }

    // unclosed double quote
    bump.reset();
    {
        let input = "exe \"text";
        let err = parse_in(&bump, input).unwrap_err();
        assert_eq!(err.token, Token::DoubleQuote);
        assert_eq!(&input[err.span], "\"");
        assert_eq!(err.kind, ErrorKind::UnclosedDoubleQuote);
    }

    // redirect no target
    bump.reset();
    {
        let input = r#"exe >"#;
        let err = parse_in(&bump, input).unwrap_err();
        assert_eq!(err.token, Token::Redirect);
        assert_eq!(&input[err.span], ">");
        assert_eq!(err.kind, ErrorKind::RedirectNoTarget);
    }

    // redirect no exe
    bump.reset();
    {
        let input = r#"> fd exe"#;
        let err = parse_in(&bump, input).unwrap_err();
        assert_eq!(err.token, Token::Redirect);
        assert_eq!(&input[err.span], ">");
        assert_eq!(err.kind, ErrorKind::FirstArgMustLiteral);
    }

    // redirect then args
    bump.reset();
    {
        let input = r#"exe > fd arg"#;
        let err = parse_in(&bump, input).unwrap_err();
        assert_eq!(err.token, Token::Text);
        assert_eq!(&input[err.span], "arg");
        assert_eq!(err.kind, ErrorKind::UnexpectedArgument);
    }

    Ok(())
}

#[derive(Debug, PartialEq, Copy, Clone)]
enum Kind {
    Command,
    Literal,
    Env,
    Escape,
    SubShell,
    Pipe,
    Then,
    AndIf,
    OrIf,
    SingleStr,
    DoubleStr,
    Argument,
    RedirectOut,
    RedirectOutAppend,
    RedirectErr,
    RedirectErrAppend,
}

#[derive(Debug, PartialEq, Clone)]
enum Output {
    Item(Kind, String),
    One(Kind, Box<Output>),
    Vec(Kind, Vec<Output>)
}

trait Fix {
    fn fix(&self, input: &str) -> Output;
}

impl Fix for Command<'_> {
    fn fix(&self, input: &str) -> Output {
        let mut output = Vec::new();
        output.push(Output::Item(Kind::Literal, input[self.exe.0.clone()].into()));
        for arg in &self.args {
            output.push(arg.fix(input));
        }
        for redirect in &self.redirect {
            output.push(redirect.fix(input));
        }
        if let Some(chain) = self.chain.as_ref() {
            output.push(chain.fix(input));
        }
        Output::Vec(Kind::Command, output)
    }
}

impl Fix for Argument<'_> {
    fn fix(&self, input: &str) -> Output {
        let mut output = Vec::new();
        for arg in &self.0 {
            match arg {
                ArgSlice::Str(Literal(span)) =>
                    output.push(Output::Item(Kind::Literal, input[span.clone()].into())),
                ArgSlice::Env(Env(span)) =>
                    output.push(Output::Item(Kind::Env, input[span.clone()].into())),
                ArgSlice::Escape(Escape(span)) =>
                    output.push(Output::Item(Kind::Escape, input[span.clone()].into())),
                ArgSlice::Single(SingleStr(span)) =>
                    output.push(Output::Item(Kind::SingleStr, input[span.clone()].into())),
                ArgSlice::Double(arg) =>
                    output.push(arg.fix(input)),
                ArgSlice::SubShell(arg) =>
                    output.push(Output::One(Kind::SubShell, Box::new(arg.0.fix(input))))
            }
        }
        Output::Vec(Kind::Argument, output)
    }
}

impl Fix for DoubleStr<'_> {
    fn fix(&self, input: &str) -> Output {
        let mut output = Vec::new();
        for arg in &self.list {
            match arg {
                StrSlice::Str(Literal(span)) =>
                    output.push(Output::Item(Kind::Literal, input[span.clone()].into())),
                StrSlice::Env(Env(span)) =>
                    output.push(Output::Item(Kind::Env, input[span.clone()].into())),
                StrSlice::Escape(Escape(span)) =>
                    output.push(Output::Item(Kind::Escape, input[span.clone()].into())),
                StrSlice::SubShell(arg) =>
                    output.push(Output::One(Kind::SubShell, Box::new(arg.0.fix(input))))
            }
        }
        Output::Vec(Kind::DoubleStr, output)
    }
}

impl Fix for Chain<'_> {
    fn fix(&self, input: &str) -> Output {
        match self {
            Chain::Pipe(subshell) => Output::One(Kind::Pipe, Box::new(subshell.0.fix(input))),
            Chain::Then(subshell) => Output::One(Kind::Then, Box::new(subshell.0.fix(input))),
            Chain::AndIf(subshell) => Output::One(Kind::AndIf, Box::new(subshell.0.fix(input))),
            Chain::OrIf(subshell) => Output::One(Kind::OrIf, Box::new(subshell.0.fix(input)))
        }
    }
}

impl Fix for Redirect<'_> {
    fn fix(&self, input: &str) -> Output {
        let kind = match (self.ty, self.append) {
            (StdioType::Out, false) => Kind::RedirectOut,
            (StdioType::Out, true) => Kind::RedirectOutAppend,
            (StdioType::Err, false) => Kind::RedirectErr,
            (StdioType::Err, true) => Kind::RedirectErrAppend
        };

        Output::One(kind, Box::new(self.value.fix(input)))
    }
}
