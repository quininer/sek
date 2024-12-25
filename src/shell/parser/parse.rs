use la_arena::Arena;
use logos::Span;
use super::error::LexingError;
use super::token::{ Token, TokenId, TokenArena };
use super::syntax::{ Node, NodeId };


pub struct Parser {
    tokens: TokenArena,
    nodes: Arena<Node>
}

impl Parser {
    pub fn parse(&mut self, input: &str) -> NodeId {
        self.tokens.clear();
        self.nodes.clear();

        self.tokens.extend(logos::Lexer::new(input).spanned());

        todo!()
    }
}

struct State<'p, I> {
    tokens: &'p TokenArena,
    nodes: &'p mut Arena<Node>,

    iter: I,
    
    is_subshell: bool,
    has_redirect: bool,
    incomplete: bool
}
