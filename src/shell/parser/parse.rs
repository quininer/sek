use std::ops::ControlFlow;
use logos::Span;
use super::error::{ ParseFailed, ErrorKind };
use super::token::{ Token, TokenId, TokenItem };
use super::syntax::{ self, Node, NodeId };
use crate::util::arena::{ self, Arena };


pub struct Parser {
    tokens: Arena<TokenItem>,
    nodes: Arena<Node>
}

impl Parser {
    pub fn parse(&mut self, input: &str, incomplete: bool) -> NodeId {
        self.tokens.clear();
        self.nodes.clear();

        self.tokens.extend(logos::Lexer::new(input).spanned());
        let null = self.nodes.alloc(Node::Null);

        let _state = State {
            tokens: &self.tokens,
            nodes: &mut self.nodes,

            iter: self.tokens.iter(),

            is_subshell: false,
            has_redirect: false,
            null, incomplete
        };

        todo!()
    }
}

struct State<'p> {
    tokens: &'p Arena<TokenItem>,
    nodes: &'p mut Arena<Node>,

    iter: arena::Iter<'p, TokenItem>,

    null: NodeId,
    is_subshell: bool,
    has_redirect: bool,
    incomplete: bool
}

macro_rules! lookup {
    (
        static $name:ident = [$ty:ty ; $size:expr];
        $( $( $token:path )|* => $val:expr ,)*
        #_ => $default:expr $(,)?
    ) => (
        static $name: [$ty; $size] = {
            let default = $default as $ty;
            let mut table = [default; $size];

            $(
                let val = $val as $ty;
                $(
                    table[$token as usize] = val;
                )*
            )*

            table
        };
    )
}

fn failed(item: &TokenItem)
    -> ParseFailed
{
    let (maybe_token, span) = &item;
    ParseFailed {
        token: maybe_token.as_ref().copied().ok(),
        kind: ErrorKind::InvalidToken,
        span: span.clone()
    }
}

impl syntax::Command {
    fn parse_in(state: &mut State<'_>) -> Result<NodeId, ParseFailed> {
        #[derive(Clone, Copy)]
        struct SubState {
            link: NodeId
        }

        type LookupAction = fn(&mut State<'_>, SubState, NodeId, TokenId)
            -> Result<ControlFlow<(), SubState>, ParseFailed>;

        lookup!{
            static LUT = [LookupAction; Token::size()];

            Token::Text => |state, substate, node, token| {
                let item = &state.tokens[token];
                
                let arg = syntax::Argument::parse_in(state)?;
                let next = state.nodes.alloc(Node::Link(syntax::Link {
                    current: state.null,
                    next: None
                }));

                let link = matches2!(&mut state.nodes[substate.link], Node::Link)
                    .ok_or_else(|| failed(item).with_kind(ErrorKind::Unreachable))?;
                link.current = arg;
                link.next = Some(next);
                Ok(ControlFlow::Continue(SubState { link: next }))
            },
            Token::SingleQuote
                | Token::DoubleQuote
                | Token::ShellOpen
                | Token::Backslash
                | Token::Env
            => |state, substate, _node, token| {
                let item = &state.tokens[token];

                let arg = syntax::Argument::parse_in(state)?;
                let next = state.nodes.alloc(Node::Link(syntax::Link {
                    current: state.null,
                    next: None
                }));

                let link = matches2!(&mut state.nodes[substate.link], Node::Link)
                    .ok_or_else(|| failed(item).with_kind(ErrorKind::Unreachable))?;
                link.current = arg;
                link.next = Some(next);
                Ok(ControlFlow::Continue(SubState { link: next }))
            },
            Token::Pipe
                | Token::Then
                | Token::AndIf
                | Token::OrIf
            => |state, substate, _node, token| {
                // TODO

                Ok(ControlFlow::Break(()))
            },
            Token::Redirect => |state, substate, node, token| {
                // TODO

                Ok(ControlFlow::Continue(substate))
            },
            Token::ShellClose => |state, _substate, _node, token|
                if state.is_subshell {
                    Ok(ControlFlow::Break(()))
                } else {
                    Err(failed(&state.tokens[token]).with_kind(ErrorKind::UnexpectedClose))
                },
            #_ => |_, _, _, _| todo!()
        }
        
        let node_id = state.nodes.alloc(syntax::Node::Command(syntax::Command {
            exe: state.null,
            args: state.null,
            redirect: state.null,
            chain: state.null
        }));

        let mut substate = SubState {
            link: state.null
        };

        // first token
        {
            let token = state.iter.next()
                .ok_or_else(|| ParseFailed {
                    token: None,
                    kind: ErrorKind::EmptyCommand,
                    span: 0..0
                })?;
            let item = &state.tokens[token];
            let (token, span) = item;

            // check first token
            match token {
                Ok(Token::Text) => (),
                Ok(Token::ShellClose) if state.is_subshell =>
                    return Err(failed(item).with_kind(ErrorKind::EmptyCommand)),
                Ok(_) => return Err(failed(item).with_kind(ErrorKind::FirstArgMustLiteral)),
                Err(_) => return Err(failed(item).with_kind(ErrorKind::InvalidToken))
            }

            let exe = state.nodes.alloc(Node::Literal(syntax::Literal(span.clone())));
            let args = state.nodes.alloc(Node::Link(syntax::Link {
                current: state.null,
                next: None
            }));

            let cmd = matches2!(&mut state.nodes[node_id], Node::Command)
                .ok_or_else(|| failed(item).with_kind(ErrorKind::Unreachable))?;
            cmd.exe = exe;
            cmd.args = args;
            substate.link = args;
        }

        // args token
        while let Some(token_id) = state.iter.peek() {
            let (maybe_token, span) = &state.tokens[token_id];
            let &token = maybe_token
                .as_ref()
                .map_err(|_| ParseFailed {
                    token: None,
                    kind: ErrorKind::InvalidToken,
                    span: span.clone()
                })?;

            substate = match LUT[token as usize](state, substate, node_id, token_id)? {
                ControlFlow::Continue(next) => next,
                ControlFlow::Break(()) => break,
            };

            state.iter.next();
        }

        Ok(node_id)        
    }
}

impl syntax::Argument {
    fn parse_in(state: &mut State<'_>) -> Result<NodeId, ParseFailed> {
        todo!()
    }
}
