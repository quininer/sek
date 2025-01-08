use std::mem;
use std::ops::ControlFlow;
use super::error::{ ParseFailed, ErrorKind };
use super::token::{ Token, TokenId, TokenItem };
use super::syntax::{ self, Node, NodeId };
use crate::util::arena::{ self, Arena };
use crate::util::ScopeGuard;


#[derive(Default)]
pub struct Parser {
    tokens: Arena<TokenItem>,
    nodes: Arena<Node>
}

impl Parser {
    pub fn new(&mut self, input: &str) -> Result<NodeId, ParseFailed> {
        self.tokens.clear();
        self.nodes.clear();

        let tokens = logos::Lexer::new(input)
            .spanned()
            .map(|(token, span)| token
                .map(|token| (token, span.clone()))
                .map_err(|err| (err, span))
            )
            .collect::<Result<Vec<_>, _>>()
            .map_err(|(_, span)| ParseFailed {
                token: None,
                span: Some(span),
                kind: ErrorKind::InvalidToken
            })?;
        self.tokens.extend(tokens);
        let null = self.nodes.alloc(Node::Null);

        let mut state = State {
            source: input,
            tokens: &self.tokens,
            nodes: &mut self.nodes,

            iter: self.tokens.iter(),

            is_subshell: false,
            null
        };

        state.command()
    }
}

struct State<'p> {
    source: &'p str,
    tokens: &'p Arena<TokenItem>,
    nodes: &'p mut Arena<Node>,

    iter: arena::Iter<'p, TokenItem>,

    null: NodeId,
    is_subshell: bool,
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
    let (token, span) = &item;
    ParseFailed {
        token: Some(*token),
        span: Some(span.clone()),
        kind: ErrorKind::InvalidToken,
    }
}

impl State<'_> {
    fn chain_to(&mut self, link: NodeId, current: NodeId, token: TokenId) -> Result<NodeId, ParseFailed> {
        let item = &self.tokens[token];

        {
            let link_node = matches2!(&mut self.nodes[link], Node::Link)
                .ok_or_else(|| failed(item).with_kind(ErrorKind::Unreachable))?;
            if link_node.current == self.null {
                link_node.current = current;
                return Ok(link);
            }  
        }
        
        let next = self.nodes.alloc(Node::Link(syntax::Link {
            current,
            next: None
        }));

        let link = matches2!(&mut self.nodes[link], Node::Link)
            .ok_or_else(|| failed(item).with_kind(ErrorKind::Unreachable))?;
        link.next = Some(next);

        Ok(next)
    }
    
    fn command(&mut self) -> Result<NodeId, ParseFailed> {
        #[derive(Clone, Copy)]
        enum SubState {
            Argument(NodeId),
            Redirect(NodeId) 
        }

        type LookupAction = fn(&mut State<'_>, SubState, NodeId, TokenId)
            -> Result<ControlFlow<(), SubState>, ParseFailed>;

        lookup!{
            static LUT = [LookupAction; Token::size()];

            Token::Text
                | Token::SingleQuote
                | Token::DoubleQuote
                | Token::ShellOpen
                | Token::Backslash
                | Token::Variable
            => |state, substate, _, token| {
                let link = matches2!(substate, SubState::Argument)
                    .ok_or_else(|| failed(&state.tokens[token]).with_kind(ErrorKind::UnexpectedToken))?;

                let arg = state.arg()?;
                let next = state.chain_to(link, arg, token)?;
                Ok(ControlFlow::Continue(SubState::Argument(next)))
            },
            Token::Redirect => |state, substate, node, token| {
                let link = match substate {
                    SubState::Argument(_) => {
                        let item = &state.tokens[token];
                        let link = state.nodes.alloc(Node::Link(syntax::Link {
                            current: state.null,
                            next: None
                        }));
                        let cmd = matches2!(&mut state.nodes[node], Node::Command)
                            .ok_or_else(|| failed(item).with_kind(ErrorKind::Unreachable))?;
                        cmd.redirect = link;
                        link                  
                    },
                    SubState::Redirect(link) => link,
                };

                let node = state.redirect()?;
                let next = state.chain_to(link, node, token)?;
                Ok(ControlFlow::Continue(SubState::Redirect(next)))
            },
            Token::Pipe
                | Token::Then
                | Token::AndIf
                | Token::OrIf
            => |state, _, node, token| {
                let chain = state.chain()?;
                
                let item = &state.tokens[token];
                let cmd = matches2!(&mut state.nodes[node], Node::Command)
                    .ok_or_else(|| failed(item).with_kind(ErrorKind::Unreachable))?;
                assert_eq!(cmd.chain, state.null);
                cmd.chain = chain;

                Ok(ControlFlow::Break(()))
            },
            Token::ShellClose => |state, _substate, _, token|
                if state.is_subshell {
                    Ok(ControlFlow::Break(()))
                } else {
                    Err(failed(&state.tokens[token]).with_kind(ErrorKind::UnexpectedClose))
                },
            Token::Empty => |state, substate, _, _| {
                state.iter.bump();
                Ok(ControlFlow::Continue(substate))
            },
            Token::Comment => |_, _, _, _| Ok(ControlFlow::Break(())),
            #_ => |state, _, _, token| Err(failed(&state.tokens[token]).with_kind(ErrorKind::UnexpectedToken)),
        }
        
        let node_id = self.nodes.alloc(syntax::Node::Command(syntax::Command {
            exe: self.null,
            args: self.null,
            redirect: self.null,
            chain: self.null
        }));

        // first token
        let mut substate = {
            let token = self.iter.next()
                .ok_or_else(|| ParseFailed {
                    token: None,
                    kind: ErrorKind::EmptyCommand,
                    span: None
                })?;
            let item = &self.tokens[token];
            let (token, span) = item;

            // check first token
            match token {
                Token::Text => (),
                Token::ShellClose if self.is_subshell =>
                    return Err(failed(item).with_kind(ErrorKind::EmptyCommand)),
                _ => return Err(failed(item).with_kind(ErrorKind::FirstArgMustLiteral)),
            }

            let exe = self.nodes.alloc(Node::Literal(syntax::Literal(span.clone())));
            let args = self.nodes.alloc(Node::Link(syntax::Link {
                current: self.null,
                next: None
            }));

            let cmd = matches2!(&mut self.nodes[node_id], Node::Command)
                .ok_or_else(|| failed(item).with_kind(ErrorKind::Unreachable))?;
            cmd.exe = exe;
            cmd.args = args;
            SubState::Argument(args)
        };

        // args token
        while let Some(token_id) = self.iter.peek() {
            let (token, _span) = &self.tokens[token_id];
            let &token = token;

            match LUT[token as usize](self, substate, node_id, token_id)? {
                ControlFlow::Continue(next) => substate = next,
                ControlFlow::Break(()) => break,
            }
        }

        Ok(node_id)        
    }

    fn arg(&mut self) -> Result<NodeId, ParseFailed> {
        #[derive(Clone, Copy)]
        struct SubState {
            link: NodeId
        }

        type LookupAction = fn(&mut State<'_>, SubState, TokenId)
            -> Result<ControlFlow<(), SubState>, ParseFailed>;

        lookup!{
            static LUT = [LookupAction; Token::size()];

            Token::SingleQuote => |state, substate, token| {
                let node = state.single_str()?;
                let next = state.chain_to(substate.link, node, token)?;
                Ok(ControlFlow::Continue(SubState { link: next }))
            },
            Token::DoubleQuote => |state, substate, token| {
                let node = state.double_str()?;
                let next = state.chain_to(substate.link, node, token)?;
                Ok(ControlFlow::Continue(SubState { link: next }))
            },
            Token::Variable => |state, substate, token| {
                state.iter.bump();
                let node = state.nodes.alloc(Node::Variable(syntax::Variable(token)));
                let next = state.chain_to(substate.link, node, token)?;
                Ok(ControlFlow::Continue(SubState { link: next }))
            },
            Token::ShellOpen => |state, substate, token| {
                let node = state.subshell()?;
                let next = state.chain_to(substate.link, node, token)?;
                Ok(ControlFlow::Continue(SubState { link: next }))
            },
            Token::ShellClose => |state, _, token| if state.is_subshell {
                Ok(ControlFlow::Break(()))
            } else {
                Err(failed(&state.tokens[token]).with_kind(ErrorKind::UnexpectedClose))
            },
            Token::Text => |state, substate, token| {
                state.iter.bump();
                let (_, span) = &state.tokens[token];
                let node = state.nodes.alloc(Node::Literal(syntax::Literal(span.clone())));
                let next = state.chain_to(substate.link, node, token)?;
                Ok(ControlFlow::Continue(SubState { link: next }))               
            },
            Token::Backslash => |state, substate, token| {
                let node = state.escape()?;
                let next = state.chain_to(substate.link, node, token)?;
                Ok(ControlFlow::Continue(SubState { link: next }))
            },
            Token::Empty
                | Token::Comment
                | Token::Pipe
                | Token::Then
                | Token::AndIf
                | Token::OrIf
            => |_, _, _| Ok(ControlFlow::Break(())),
            #_ => |state, _, token| Err(failed(&state.tokens[token]).with_kind(ErrorKind::UnexpectedToken)),
        }

        let link = self.nodes.alloc(Node::Link(syntax::Link {
            current: self.null,
            next: None
        }));
        let mut substate = SubState { link };

        while let Some(token_id) = self.iter.peek() {
            let (token, _span) = &self.tokens[token_id];
            let &token = token;
            
            match LUT[token as usize](self, substate, token_id)? {
                ControlFlow::Continue(next) => substate = next,
                ControlFlow::Break(()) => break
            }
        }

        Ok(link)
    }

    fn single_str(&mut self) -> Result<NodeId, ParseFailed> {
        let start_token = self.iter.next().unwrap();
        assert!(matches!(&self.tokens[start_token], (Token::SingleQuote, _)));

        let mut end_token = None;

        while let Some(token) = self.iter.next() {
            if matches!(&self.tokens[token], (Token::SingleQuote, _)) {
                end_token = Some(token);
                break
            }
        }

        Ok(self.nodes.alloc(Node::SingleStr(syntax::SingleStr {
            start_token, end_token
        })))
    }

    fn double_str(&mut self) -> Result<NodeId, ParseFailed> {
        #[derive(Clone, Copy)]
        struct SubState {
            link: NodeId
        }

        type LookupAction = fn(&mut State<'_>, SubState, TokenId)
            -> Result<ControlFlow<TokenId, SubState>, ParseFailed>;

        lookup!{
            static LUT = [LookupAction; Token::size()];

            Token::SingleQuote
                | Token::Text
                | Token::Empty
                | Token::ShellClose
                | Token::Comment
                | Token::Pipe
                | Token::Then
                | Token::AndIf
                | Token::OrIf
                | Token::Redirect => |state, substate, token|
            {
                let item = &state.tokens[token];
                let (_, span) = item;

                let current = matches2!(&state.nodes[substate.link], Node::Link)
                    .map(|link| link.current)
                    .ok_or_else(|| failed(item).with_kind(ErrorKind::Unreachable))?;

                let link = if let Some(lit) = matches2!(&mut state.nodes[current], Node::Literal) {
                    lit.0.end = span.end;
                    substate.link
                } else {
                    let lit = state.nodes.alloc(Node::Literal(syntax::Literal(span.clone())));
                    state.chain_to(substate.link, lit, token)?
                };
                
                state.iter.bump();
                Ok(ControlFlow::Continue(SubState { link }))
            },
            Token::DoubleQuote => |state, _, token| {
                state.iter.bump();
                Ok(ControlFlow::Break(token))
            },
            Token::ShellOpen => |state, substate, token| {
                let node = state.subshell()?;
                let next = state.chain_to(substate.link, node, token)?;
                Ok(ControlFlow::Continue(SubState { link: next }))
            },
            Token::Variable => |state, substate, token| {
                state.iter.bump();
                let node = state.nodes.alloc(Node::Variable(syntax::Variable(token)));
                let next = state.chain_to(substate.link, node, token)?;
                Ok(ControlFlow::Continue(SubState { link: next }))               
            },
            Token::Backslash => |state, substate, token| {
                let node = state.escape()?;
                let next = state.chain_to(substate.link, node, token)?;
                Ok(ControlFlow::Continue(SubState { link: next }))
            },
            #_ => |state, _, token| Err(failed(&state.tokens[token]).with_kind(ErrorKind::UnexpectedToken)),
        }

        let start_token = self.iter.next().unwrap();
        assert!(matches!(&self.tokens[start_token], (Token::DoubleQuote, _)));        

        let link = self.nodes.alloc(Node::Link(syntax::Link {
            current: self.null,
            next: None
        }));
        let mut substate = SubState { link };
        let mut end_token = None;

        while let Some(token_id) = self.iter.peek() {
            let (token, _span) = &self.tokens[token_id];
            let &token = token;

            match LUT[token as usize](self, substate, token_id)? {
                ControlFlow::Continue(next) => substate = next,
                ControlFlow::Break(last) => {
                    end_token = Some(last);
                    break
                }
            }
        }

        Ok(self.nodes.alloc(Node::DoubleStr(syntax::DoubleStr {
            start_token, end_token,
            list: link
        })))        
    }

    fn escape(&mut self) -> Result<NodeId, ParseFailed> {
        let token = self.iter.next().unwrap();
        assert!(matches!(&self.tokens[token], (Token::Backslash, _)));

        let value = self.iter.peek()
            .ok_or_else(|| failed(&self.tokens[token]).with_kind(ErrorKind::IncompleteEscape))?;

        if let (Token::Text, _) = &self.tokens[value] {
            Err(failed(&self.tokens[token]).with_kind(ErrorKind::UnknownEscape))
        } else {
            self.iter.bump();
            Ok(self.nodes.alloc(Node::Escape(syntax::Escape { token, value })))
        }
    }

    fn subshell(&mut self) -> Result<NodeId, ParseFailed> {
        let start_token = self.iter.next().unwrap();
        assert!(matches!(&self.tokens[start_token], (Token::ShellOpen, _)));

        // set is_subshell
        let prev_subshell = mem::replace(&mut self.is_subshell, true);
        let mut state = ScopeGuard(self, |state| state.is_subshell = prev_subshell);
        let state = state.as_mut();

        let cmd = state.command()?;

        let mut end_token = None;

        if let Some(token) = state.iter.peek() {
            if matches!(&state.tokens[token], (Token::ShellClose, _)) {
                state.iter.bump();
                end_token = Some(token);
            }
        }

        Ok(state.nodes.alloc(Node::SubShell(syntax::SubShell {
            start_token, end_token, cmd
        })))
    }

    fn redirect(&mut self) -> Result<NodeId, ParseFailed> {
        use syntax::StdioKind;
        
        fn parse_redirect(token: &str) -> Option<(StdioKind, bool)> {
            match token {
                ">" | "1>" => Some((StdioKind::Out, false)),
                ">>" | "1>>" => Some((StdioKind::Out, true)),
                "2>" => Some((StdioKind::Err, false)),
                "2>>" => Some((StdioKind::Err, true)),
                "*>" => Some((StdioKind::All, false)),
                "*>>" => Some((StdioKind::All, true)),
                _ => None
            }
        }

        let token_id = self.iter.next().unwrap();
        let item = &self.tokens[token_id];
        let (token, span) = item;
        assert!(matches!(token, Token::Redirect));

        let (kind, append) = parse_redirect(&self.source[span.clone()])
            .ok_or_else(|| failed(item).with_kind(ErrorKind::UnknownRedirect))?;

        // skip empty
        while let Some(token) = self.iter.peek() {
            if matches!(&self.tokens[token], (Token::Empty, _)) {
                self.iter.bump();
            } else {
                break
            }
        }

        let node = self.arg()?;

        Ok(self.nodes.alloc(Node::Redirect(syntax::Redirect {
            token: token_id,
            value: node,
            kind, append
        })))
    }

    fn chain(&mut self) -> Result<NodeId, ParseFailed> {
        use syntax::{ ChainKind, StdioKind };
        
        let token_id = self.iter.next().unwrap();
        let item = &self.tokens[token_id];
        let (token, span) = item;

        let kind = match token {
            Token::Pipe => {
                let stdio = match &self.source[span.clone()] {
                    "|" | "1|" => StdioKind::Out,
                    "2|" => StdioKind::Err,
                    "*|" => StdioKind::All,
                    token => panic!("unexpected token: {:?}", token),
                };

                ChainKind::Pipe(stdio)
            },
            Token::Then => ChainKind::Then,
            Token::AndIf => ChainKind::AndIf,
            Token::OrIf => ChainKind::OrIf,
            token => panic!("unexpected token: {:?}", token),
        };

        let shell = self.command()?;

        Ok(self.nodes.alloc(Node::Chain(syntax::Chain {
            token: token_id,
            kind, shell
        })))
    }
}
