use bumpalo::Bump;
use bumpalo::collections::Vec;
use logos::{ Logos, Source, Span };
use crate::shell::parser::token::{ Token, TOKEN_KIND };
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
    last: Option<Token>,
    point: usize
}

enum Action {
    Continue,
    Break
}

impl<'c> Command<'c> {
    fn parse_in<'i>(bump: &'c Bump, state: &mut State<'i>) -> Result<Self, ParseFailed> {
        type Lookup = for<'c, 'i> fn(&'c Bump, &mut State<'i>, &mut Command<'c>)
            -> Result<Action, ParseFailed>;

        lookup!{
            static LUT = [Lookup; TOKEN_KIND.len()];

            Token::SingleQuote
                | Token::DoubleQuote
                | Token::ShellOpen
                | Token::Backslash
                | Token::Env
                | Token::Text => |bump, state, cmd|
            {
                cmd.args.push(Argument::parse_in(bump, state)?);
                Ok(Action::Continue)
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
            Token::Comment => |_, _, _| Ok(Action::Break),
            Token::Empty => |_, _, _| Ok(Action::Continue),
            _ => |_, _, _| todo!()
        };

        let mut cmd = Command {
            args: Vec::with_capacity_in(8, bump),
            chain: None,
            redirect: Vec::new_in(bump)
        };

        while let Some(token) = state.last.take()
            .or_else(|| state.lex.next())
        {
            state.last = Some(token);
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
        todo!()
    }
}

impl<'c> Chain<'c> {
    fn parse_in<'i>(bump: &'c Bump, state: &mut State<'i>) -> Result<Self, ParseFailed> {
        todo!()
    }
}

impl SingleStr {
    fn parse_in<'c, 'i>(bump: &'c Bump, state: &mut State<'i>) -> Result<Self, ParseFailed> {
        todo!()
    }
}

impl<'c> DoubleStr<'c> {
    fn parse_in<'i>(bump: &'c Bump, state: &mut State<'i>) -> Result<Self, ParseFailed> {
        todo!()
    }
}

impl<'c> Argument<'c> {
    fn parse_in<'i>(bump: &'c Bump, state: &mut State<'i>) -> Result<Self, ParseFailed> {
        type Lookup = for<'c, 'i> fn(&'c Bump, &mut State<'i>, &mut Argument<'c>)
            -> Result<Action, ParseFailed>;

        lookup!{
            static LUT = [Lookup; TOKEN_KIND.len()];

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
            Token::Env => |bump, state, arg| {
                arg.0.push(ArgSlice::Env(Env(state.lex.span())));
                Ok(Action::Continue)
            },
            Token::Text => |bump, state, arg| {
                arg.0.push(ArgSlice::Str(Literal(state.lex.span())));
                Ok(Action::Continue)
            },
            Token::Empty => |_, _, _| Ok(Action::Break),
            Token::Backslash => |_, _, _| todo!(),
            _ => |_, _, _| todo!()
        }

        let mut arg = Argument(Vec::with_capacity_in(4, bump));

        while let Some(token) = state.last.take()
            .or_else(|| state.lex.next())
        {
            state.last = Some(token);
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

        let (ty, append) = parse_redirect(state.lex.slice())
            .ok_or_else(|| todo!())?;

        let value = Argument::parse_in(bump, state)?;

        Ok(Redirect { ty, append, value })
    }
}
