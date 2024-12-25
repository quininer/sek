use std::ops::ControlFlow;
use la_arena::Arena;
use logos::Span;
use super::error::{ ParseFailed, ErrorKind };
use super::token::{ Token, TokenId, TokenArena };
use super::syntax::{ self, Node, NodeId };


pub struct Parser {
    tokens: TokenArena,
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

            is_subshell: false,
            has_redirect: false,
            null, incomplete
        };

        todo!()
    }
}

struct State<'p> {
    tokens: &'p TokenArena,
    nodes: &'p mut Arena<Node>,

    null: NodeId,
    is_subshell: bool,
    has_redirect: bool,
    incomplete: bool
}

type LookupAction = for<'p> fn(&mut State<'p>, NodeId) -> Result<ControlFlow<()>, ParseFailed>;

impl syntax::Command {
    fn parse_in(state: &mut State<'_>, mut iter: impl Iterator<Item = TokenId>)
        -> Result<NodeId, ParseFailed>
    {
        let node_id = state.nodes.alloc(syntax::Node::Command(syntax::Command {
            exe: state.null,
            args: state.null,
            redirect_out: state.null,
            redirect_err: state.null,
            chain: state.null
        }));
        
        while let Some(token) = iter.next() {
            todo!()
        }
        
        todo!()
    }
}
