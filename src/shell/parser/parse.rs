use std::mem;
use bumpalo::Bump;
use bumpalo::boxed::Box;
use bumpalo::collections::Vec;
use logos::{ Logos, Span };
use scopeguard::guard;
use if_chain::if_chain;
use crate::shell::parser::{ Token, ErrorKind };
use crate::shell::parser::type_::*;


#[derive(Debug)]
pub struct ParseFailed {
    pub token: Token,
    pub kind: ErrorKind,
    pub span: Span,
}

enum Action {
    Continue,
    Break
}

macro_rules! lookup {
    (
        static $name:ident = [$ty:ty ; $size:expr];
        $( $( $enum:ident :: $token:ident )|* => $val:expr ,)*
        _ => $default:expr $(,)?
    ) => (
        static $name: [$ty; $size] = {
            let default = $default as $ty;
            let mut table = [default; $size];

            $(
                let val = $val as $ty;
                $(
                    table[$enum :: $token as usize] = val;
                )*
            )*

            table
        };
    )
}

struct State<'i> {
    lex: logos::Lexer<'i, Token>,
    last: Token,
    is_subshell: bool,
    has_redirect: bool,
    incomplete: bool
}

fn bad(state: &State<'_>, kind: ErrorKind) -> ParseFailed {
    ParseFailed {
        token: state.last,
        span: state.lex.span(),
        kind
    }
}

pub fn parse_in<'c>(bump: &'c Bump, input: &str) -> Result<Command<'c>, ParseFailed> {
    let mut state = State {
        lex: Token::lexer(input),
        last: Token::Unknown,
        is_subshell: false,
        has_redirect: false,
        incomplete: false
    };

    Command::parse_in(bump, &mut state)
}

pub fn incomplete_parse_in<'c>(bump: &'c Bump, input: &str) -> Result<Command<'c>, ParseFailed> {
    let mut state = State {
        lex: Token::lexer(input),
        last: Token::Unknown,
        is_subshell: false,
        has_redirect: false,
        incomplete: true
    };

    Command::parse_in(bump, &mut state)
}

impl<'c> Command<'c> {
    fn parse_in<'i>(bump: &'c Bump, state: &mut State<'i>) -> Result<Self, ParseFailed> {
        type Lookup = for<'c, 'i> fn(&'c Bump, &mut State<'i>, &mut Option<Command<'c>>)
            -> Result<Action, ParseFailed>;

        lookup!{
            static LUT = [Lookup; Token::size()];

            Token::Text => |bump, state, cmd| if state.has_redirect {
                Err(bad(state, ErrorKind::UnexpectedArgument))
            } else if let Some(cmd) = cmd {
                cmd.args.push(Argument::parse_in(bump, state)?);
                Ok(if state.is_subshell && Token::ShellClose == state.last {
                    Action::Break
                } else {
                    Action::Continue
                })
            } else {
                *cmd = Some(Command {
                    exe: Literal(state.lex.span()),
                    args: Vec::with_capacity_in(8, bump),
                    redirect: Vec::new_in(bump),
                    chain: None,
                });
                Ok(Action::Continue)
            },
            Token::SingleQuote
                | Token::DoubleQuote
                | Token::ShellOpen
                | Token::Backslash
                | Token::Env => |bump, state, cmd|
            if state.has_redirect {
                Err(bad(state, ErrorKind::UnexpectedArgument))
            } else if let Some(cmd) = cmd.as_mut() {
                cmd.args.push(Argument::parse_in(bump, state)?);
                Ok(if state.is_subshell && Token::ShellClose == state.last {
                    Action::Break
                } else {
                    Action::Continue
                })
            } else {
                Err(bad(state, ErrorKind::FirstArgMustLiteral))
            },
            Token::Pipe
                | Token::Then
                | Token::AndIf
                | Token::OrIf => |bump, state, cmd|
            if let Some(cmd) = cmd.as_mut() {
                cmd.chain = Some(Chain::parse_in(bump, state)?);
                Ok(Action::Break)
            } else {
                Err(bad(state, ErrorKind::FirstArgMustLiteral))
            },
            Token::Redirect => |bump, state, cmd| if let Some(cmd) = cmd.as_mut() {
                cmd.redirect.push(Redirect::parse_in(bump, state)?);

                if !state.is_subshell || state.last != Token::ShellClose {
                    Ok(Action::Continue)
                } else {
                    Ok(Action::Break)
                }
            } else {
                Err(bad(state, ErrorKind::FirstArgMustLiteral))
            },
            Token::ShellClose => |_, state, cmd| if state.is_subshell && cmd.is_some() {
                Ok(Action::Break)
            } else {
                Err(bad(state, ErrorKind::UnexpectedClose))
            },
            Token::Comment => |_, _, _| Ok(Action::Break),
            Token::Empty => |_, _, _| Ok(Action::Continue),
            _ => |_, state, _| Err(bad(state, ErrorKind::UnexpectedToken))
        };

        let mut cmd = None;
        let start = state.lex.span().start;

        while let Some(token) = state.lex.next() {
            state.last = token;
            match LUT[token as usize](bump, state, &mut cmd)? {
                Action::Continue => (),
                Action::Break => break
            }
        }

        cmd.ok_or_else(|| ParseFailed {
            token: state.last,
            kind: ErrorKind::EmptyCommand,
            span: start..state.lex.span().end
        })
    }
}

impl<'c> SubShell<'c> {
    fn parse_in<'i>(bump: &'c Bump, state: &mut State<'i>) -> Result<Self, ParseFailed> {
        let prev_subshell = mem::replace(&mut state.is_subshell, true);
        let prev_redirect = mem::replace(&mut state.has_redirect, false);
        let mut state = guard(state, |state| {
            state.is_subshell = prev_subshell;
            state.has_redirect = prev_redirect;
        });

        let span = state.lex.span();
        let cmd = Command::parse_in(bump, &mut state)?;
        let cmd = Box::new_in(cmd, bump);

        if state.incomplete || state.last == Token::ShellClose {
            Ok(SubShell(cmd))
        } else {
            Err(ParseFailed {
                token: Token::ShellOpen,
                kind: ErrorKind::UnclosedSubShell,
                span
            })
        }
    }
}

impl<'c> ChainShell<'c> {
    fn parse_in<'i>(bump: &'c Bump, state: &mut State<'i>) -> Result<Self, ParseFailed> {
        state.has_redirect = false;
        let cmd = Command::parse_in(bump, state)?;
        let cmd = Box::new_in(cmd, bump);
        Ok(ChainShell(cmd))
    }
}

impl<'c> Chain<'c> {
    fn parse_in<'i>(bump: &'c Bump, state: &mut State<'i>) -> Result<Self, ParseFailed> {
        let kind = state.last;
        let span = state.lex.span();
        let subshell = ChainShell::parse_in(bump, state)?;

        match kind {
            Token::Pipe => Ok(Chain::Pipe(subshell)),
            Token::Then => Ok(Chain::Then(subshell)),
            Token::AndIf => Ok(Chain::AndIf(subshell)),
            Token::OrIf => Ok(Chain::OrIf(subshell)),
            token => panic!("Unexpected token: {:?}", token),
        }
    }
}

impl SingleStr {
    fn parse_in<'c, 'i>(_bump: &'c Bump, state: &mut State<'i>) -> Result<Self, ParseFailed> {
        let span = state.lex.span();
        let mut end = None;

        while let Some(token) = state.lex.next() {
            state.last = token;

            if let Token::SingleQuote = token {
                end = Some(state.lex.span().end);
                break
            }
        }

        if let Some(end) = end {
            Ok(SingleStr(span.start..end))
        } else if state.incomplete {
            let end = state.lex.span().end;
            Ok(SingleStr(span.start..end))
        } else {
            Err(ParseFailed {
                token: Token::SingleQuote,
                kind: ErrorKind::UnclosedSingleQuote,
                span
            })
        }
    }
}

impl<'c> DoubleStr<'c> {
    fn parse_in<'i>(bump: &'c Bump, state: &mut State<'i>) -> Result<Self, ParseFailed> {
        type Lookup = for<'c, 'i> fn(&'c Bump, &mut State<'i>, &mut Vec<'c, StrSlice<'c>>)
            -> Result<Action, ParseFailed>;

        lookup!{
            static LUT = [Lookup; Token::size()];

            Token::SingleQuote
                | Token::Text
                | Token::Empty
                | Token::ShellClose
                | Token::Comment
                | Token::Pipe
                | Token::Then
                | Token::AndIf
                | Token::OrIf
                | Token::Redirect => |_, state, list|
            {
                let span = state.lex.span();

                if_chain!{
                    if let Some(StrSlice::Str(Literal(prev))) = list.last_mut();
                    if prev.end == span.start;
                    then {
                        prev.end = span.end;
                    } else {
                        list.push(StrSlice::Str(Literal(state.lex.span())));
                    }
                }

                Ok(Action::Continue)
            },
            Token::DoubleQuote => |_, _, _| Ok(Action::Break),
            Token::ShellOpen => |bump, state, list| {
                list.push(StrSlice::SubShell(SubShell::parse_in(bump, state)?));
                Ok(Action::Continue)
            },
            Token::Env => |_, state, list| {
                list.push(StrSlice::Env(Env(state.lex.span())));
                Ok(Action::Continue)
            },
            Token::Backslash => |_, state, list| {
                let start = state.lex.span().start;
                let _token = state.lex.next();
                let end = state.lex.span().end;
                list.push(StrSlice::Escape(Escape(start..end)));
                Ok(Action::Continue)
            },
            _ => |_, state, _| Err(bad(state, ErrorKind::UnexpectedToken))
        }

        let mut list = Vec::with_capacity_in(8, bump);
        let span = state.lex.span();

        while let Some(token) = state.lex.next() {
            state.last = token;
            match LUT[token as usize](bump, state, &mut list)? {
                Action::Continue => (),
                Action::Break => break
            }
        }

        if state.incomplete || state.last == Token::DoubleQuote {
            let end = state.lex.span().end;
            Ok(DoubleStr {
                span: span.start..end,
                list
            })
        } else {
            Err(ParseFailed {
                token: Token::DoubleQuote,
                kind: ErrorKind::UnclosedDoubleQuote,
                span
            })
        }
    }
}

impl<'c> Argument<'c> {
    fn parse_in<'i>(bump: &'c Bump, state: &mut State<'i>) -> Result<Self, ParseFailed> {
        type Lookup = for<'c, 'i> fn(&'c Bump, &mut State<'i>, &mut Argument<'c>)
            -> Result<Action, ParseFailed>;

        lookup!{
            static LUT = [Lookup; Token::size()];

            Token::SingleQuote => |bump, state, arg| {
                arg.0.push(ArgSlice::Single(SingleStr::parse_in(bump, state)?));
                Ok(Action::Continue)
            },
            Token::DoubleQuote => |bump, state, arg| {
                arg.0.push(ArgSlice::Double(DoubleStr::parse_in(bump, state)?));
                Ok(Action::Continue)
            },
            Token::ShellOpen => |bump, state, arg| {
                arg.0.push(ArgSlice::SubShell(SubShell::parse_in(bump, state)?));
                Ok(Action::Continue)
            },
            Token::ShellClose => |_, state, arg| if state.is_subshell {
                Ok(Action::Break)
            } else {
                Err(bad(state, ErrorKind::UnexpectedClose))
            },
            Token::Env => |_, state, arg| {
                arg.0.push(ArgSlice::Env(Env(state.lex.span())));
                Ok(Action::Continue)
            },
            Token::Text => |_, state, arg| {
                let span = state.lex.span();

                if_chain!{
                    if let Some(ArgSlice::Str(Literal(prev))) = arg.0.last_mut();
                    if prev.end == span.start;
                    then {
                        prev.end = span.end;
                    } else {
                        arg.0.push(ArgSlice::Str(Literal(state.lex.span())));
                    }
                }

                Ok(Action::Continue)
            },
            Token::Empty => |_, _, _| Ok(Action::Break),
            Token::Backslash => |_, state, arg| {
                let start = state.lex.span().start;
                let _token = state.lex.next();
                let end = state.lex.span().end;
                arg.0.push(ArgSlice::Escape(Escape(start..end)));
                Ok(Action::Continue)
            },
            _ => |_, state, _| Err(bad(state, ErrorKind::UnexpectedToken))
        }

        let mut arg = Argument(Vec::with_capacity_in(8, bump));
        let mut last = Some(state.last);

        while let Some(token) = last.take().or_else(|| state.lex.next()) {
            state.last = token;
            match LUT[token as usize](bump, state, &mut arg)? {
                Action::Continue => (),
                Action::Break => break
            }
        }

        Ok(arg)
    }
}

impl<'c> Redirect<'c> {
    fn parse_in<'i>(bump: &'c Bump, state: &mut State<'i>) -> Result<Self, ParseFailed> {
        fn parse_redirect(token: &str) -> Option<(StdioType, bool)> {
            match token {
                ">" | "1>" => Some((StdioType::Out, false)),
                ">>" | "1>>" => Some((StdioType::Out, true)),
                "2>" => Some((StdioType::Err, false)),
                "2>>" => Some((StdioType::Err, true)),
                _ => None
            }
        }

        state.has_redirect = true;

        let (ty, append) = parse_redirect(state.lex.slice())
            .ok_or_else(|| bad(state, ErrorKind::UnsupportedRedirectType))?;
        let span = state.lex.span();
        let mut eof = true;

        while let Some(token) = state.lex.next() {
            if token != Token::Empty {
                state.last = token;
                eof = false;
                break
            }
        }

        if !state.incomplete && eof {
            return Err(ParseFailed {
                token: Token::Redirect,
                kind: ErrorKind::RedirectNoTarget,
                span
            });
        }

        let value = Argument::parse_in(bump, state)?;

        if state.incomplete || !value.0.is_empty() {
            Ok(Redirect { ty, append, value })
        } else {
            Err(ParseFailed {
                token: Token::Redirect,
                kind: ErrorKind::RedirectNoTarget,
                span
            })
        }
    }
}
