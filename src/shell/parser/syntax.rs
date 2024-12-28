use logos::Span;
use super::token::TokenId;
use crate::util::arena;

pub type NodeId = arena::Id<Node>;

#[derive(Debug)]
pub enum Node {
    /// Meta node
    Null,
    Link(Link),

    /// Syntax node    
    Command(Command),
    Literal(Literal),
    Variable(Variable),
    Escape(Escape),
    SingleStr(SingleStr),
    DoubleStr(DoubleStr),
    SubShell(SubShell),
    Chain(Chain),
    Redirect(Redirect),
}

#[derive(Debug)]
pub struct Command {
    /// literal
    pub exe: NodeId,
    /// arg list
    pub args: NodeId,
    /// redirect list
    pub redirect: NodeId,
    /// chain shell
    pub chain: NodeId,    
}

#[derive(Debug, Clone)]
pub struct Link {
    pub current: NodeId,
    pub next: Option<NodeId>,
}

#[derive(Debug)]
pub struct Argument {
    pub span: Span,

    /// arg slice list
    pub link: NodeId,
}

/// text token
#[derive(Debug)]
pub struct Literal(pub Span);

#[derive(Debug)]
pub struct Variable(pub TokenId);

#[derive(Debug)]
pub struct Escape {
    /// escape token
    pub token: TokenId,
    /// value token
    pub value: TokenId
}

#[derive(Debug)]
pub struct SingleStr {
    /// start single quote token
    pub start_token: TokenId,
    /// end single quote token
    pub end_token: Option<TokenId>,
}

#[derive(Debug)]
pub struct DoubleStr {
    /// start double quote token
    pub start_token: TokenId,
    /// end double quote token
    pub end_end: Option<TokenId>,

    /// string slice list
    pub list: NodeId,
}

#[derive(Debug)]
pub struct SubShell {
    /// shell open token
    pub start_token: TokenId,
    /// shell close token
    pub end_token: Option<TokenId>,

    /// command
    pub cmd: NodeId
}

#[derive(Debug)]
pub struct Chain {
    /// chain token
    pub token: TokenId,

    pub stdio: StdioKind,
    pub kind: ChainKind,

    /// command
    pub shell: NodeId,
}

#[derive(Debug)]
pub struct Redirect {
    /// redirect token
    pub token: TokenId,

    pub kind: StdioKind,
    pub append: bool,

    /// arg list
    pub value: NodeId
}

#[derive(Debug)]
pub enum ChainKind {
    Pipe,
    Then,
    AndIf,
    OrIf
}

#[derive(Clone, Copy, Debug)]
pub enum StdioKind {
    Out,
    Err,
    All,
}
