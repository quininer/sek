use std::mem;
use bumpalo::Bump;
use bumpalo::boxed::Box;
use bumpalo::collections::Vec;
use logos::{ Logos, Span };
use scopeguard::guard;
use if_chain::if_chain;
use crate::shell::parser::Token;
use crate::shell::parser::type_::*;


#[derive(Debug)]
pub struct ParseFailed {
    pub token: Token,
    pub span: Span
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
    is_subshell: bool
}

enum Action {
    Continue,
    Break
}

fn bad(state: &State<'_>) -> ParseFailed {
    ParseFailed {
        token: state.last,
        span: state.lex.span()
    }
}

pub fn parse_in<'c>(bump: &'c Bump, input: &str) -> Result<Command<'c>, ParseFailed> {
    let mut state = State {
        lex: Token::lexer(input),
        last: Token::Unknown,
        is_subshell: false
    };

    Command::parse_in(bump, &mut state)
}

impl<'c> Command<'c> {
    fn parse_in<'i>(bump: &'c Bump, state: &mut State<'i>) -> Result<Self, ParseFailed> {
        type Lookup = for<'c, 'i> fn(&'c Bump, &mut State<'i>, &mut Command<'c>)
            -> Result<Action, ParseFailed>;

        lookup!{
            static LUT = [Lookup; Token::size()];

            Token::SingleQuote
                | Token::DoubleQuote
                | Token::ShellOpen
                | Token::Backslash
                | Token::Env
                | Token::Text => |bump, state, cmd|
            {
                cmd.args.push(Argument::parse_in(bump, state)?);
                Ok(if state.is_subshell && Token::ShellClose == state.last {
                    Action::Break
                } else {
                    Action::Continue
                })
            },
            Token::Pipe
                | Token::Then
                | Token::AndIf
                | Token::OrIf => |bump, state, cmd|
            {
                cmd.chain = Some(Chain::parse_in(bump, state)?);
                Ok(Action::Break)
            },
            Token::Redirect => |bump, state, cmd| {
                cmd.redirect.push(Redirect::parse_in(bump, state)?);
                Ok(Action::Continue)
            },
            Token::ShellClose => |_, state, _| if state.is_subshell {
                Ok(Action::Break)
            } else {
                Err(bad(state))
            },
            Token::Comment => |_, _, _| Ok(Action::Break),
            Token::Empty => |_, _, _| Ok(Action::Continue),
            _ => |_, state, _| {
                Err(bad(state))
            }
        };

        let mut cmd = Command {
            args: Vec::with_capacity_in(8, bump),
            chain: None,
            redirect: Vec::new_in(bump)
        };

        while let Some(token) = state.lex.next() {
            state.last = token;
            match LUT[token as usize](bump, state, &mut cmd)? {
                Action::Continue => (),
                Action::Break => break
            }
        }

        Ok(cmd)
    }
}

impl<'c> SubShell<'c> {
    fn parse_in<'i>(bump: &'c Bump, state: &mut State<'i>) -> Result<Self, ParseFailed> {
        let prev_substate = mem::replace(&mut state.is_subshell, true);
        let mut state = guard(state, |state| state.is_subshell = prev_substate);

        let span = state.lex.span();
        let cmd = Command::parse_in(bump, &mut state)?;
        let cmd = Box::new_in(cmd, bump);

        if state.last == Token::ShellClose {
            Ok(SubShell(cmd))
        } else {
            Err(ParseFailed { token: Token::ShellOpen, span })
        }
    }
}

impl<'c> ChainShell<'c> {
    fn parse_in<'i>(bump: &'c Bump, state: &mut State<'i>) -> Result<Self, ParseFailed> {
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

        if !subshell.0.args.is_empty() {
            match kind {
                Token::Pipe => Ok(Chain::Pipe(subshell)),
                Token::Then => Ok(Chain::Then(subshell)),
                Token::AndIf => Ok(Chain::AndIf(subshell)),
                Token::OrIf => Ok(Chain::OrIf(subshell)),
                token => Err(ParseFailed { token, span }),
            }
        } else {
            Err(ParseFailed { token: kind, span })
        }
    }
}

impl SingleStr {
    fn parse_in<'c, 'i>(bump: &'c Bump, state: &mut State<'i>) -> Result<Self, ParseFailed> {
        let span = state.lex.span();
        let mut end = None;

        while let Some(token) = state.lex.next() {
            state.last = token;

            if let Token::SingleQuote = token {
                end = Some(state.lex.span().start);
                break
            }
        }

        if let Some(end) = end {
            Ok(SingleStr(span.end..end))
        } else {
            Err(ParseFailed { token: Token::SingleQuote, span })
        }
    }
}

impl<'c> DoubleStr<'c> {
    fn parse_in<'i>(bump: &'c Bump, state: &mut State<'i>) -> Result<Self, ParseFailed> {
        type Lookup = for<'c, 'i> fn(&'c Bump, &mut State<'i>, &mut DoubleStr<'c>)
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
                | Token::Redirect => |_, state, string|
            {
                let span = state.lex.span();

                if_chain!{
                    if let Some(StrSlice::Str(Literal(prev))) = string.0.last_mut();
                    if prev.end == span.start;
                    then {
                        prev.end = span.end;
                    } else {
                        string.0.push(StrSlice::Str(Literal(state.lex.span())));
                    }
                }

                Ok(Action::Continue)
            },
            Token::DoubleQuote => |_, _, _| Ok(Action::Break),
            Token::ShellOpen => |bump, state, string| {
                string.0.push(StrSlice::SubShell(SubShell::parse_in(bump, state)?));
                Ok(Action::Continue)
            },
            Token::Env => |_, state, string| {
                string.0.push(StrSlice::Env(Env(state.lex.span())));
                Ok(Action::Continue)
            },
            Token::Backslash => |_, state, string| {
                let _token = state.lex.next();
                string.0.push(StrSlice::Str(Literal(state.lex.span())));
                Ok(Action::Continue)
            },
            _ => |_, state, _| Err(bad(state))
        }

        let mut string = DoubleStr(Vec::with_capacity_in(8, bump));
        let span = state.lex.span();

        while let Some(token) = state.lex.next() {
            state.last = token;
            match LUT[token as usize](bump, state, &mut string)? {
                Action::Continue => (),
                Action::Break => break
            }
        }

        if state.last == Token::DoubleQuote {
            Ok(string)
        } else {
            Err(ParseFailed { token: Token::DoubleQuote, span })
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
            Token::ShellClose => |bump, state, arg| if state.is_subshell {
                Ok(Action::Break)
            } else {
                Err(bad(state))
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
                let _token = state.lex.next();
                arg.0.push(ArgSlice::Str(Literal(state.lex.span())));
                Ok(Action::Continue)
            },
            _ => |_, state, _| Err(bad(state))
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
                ">" => Some((StdioType::Out, false)),
                ">>" => Some((StdioType::Out, true)),
                "2>" => Some((StdioType::Err, false)),
                "2>>" => Some((StdioType::Err, true)),
                _ => None
            }
        }

        let (ty, append) = parse_redirect(state.lex.slice()).ok_or_else(|| bad(state))?;
        let span = state.lex.span();
        let value = Argument::parse_in(bump, state)?;

        if !value.0.is_empty() {
            Ok(Redirect { ty, append, value })
        } else {
            Err(ParseFailed { token: Token::Redirect, span })
        }
    }
}
