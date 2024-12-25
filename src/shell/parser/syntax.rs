use logos::Span;
use super::token::TokenId;

pub type NodeId = la_arena::Idx<Node>;

pub enum Node {
    /// Meta node
    Null,
    Link(Link),

    /// Syntax node    
    Command(Command),
    Literal(Literal),
    Env(Env),
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
    /// redirect to stdout
    pub redirect_out: NodeId,
    /// redirect to stderr
    pub redirect_err: NodeId,
    /// chain shell
    pub chain: NodeId,    
}

#[derive(Debug)]
pub struct Link {
    pub current: NodeId,
    pub next: Option<NodeId>
}

/// text token
#[derive(Debug)]
pub struct Literal(pub TokenId);

#[derive(Debug)]
pub struct Env {
    /// env token
    token: TokenId,
    /// text token
    ident: TokenId
}

#[derive(Debug)]
pub struct Escape {
    /// escape token
    token: TokenId,
    /// text token
    ident: TokenId
}

#[derive(Debug)]
pub struct SingleStr {
    /// string range
    pub span: Span,
    /// start single quote token
    pub start_token: TokenId,
    /// end single quote token
    pub end_token: Option<TokenId>,
}

#[derive(Debug)]
pub struct DoubleStr {
    /// string range
    pub span: Span,
    /// start double quote token
    pub start_token: TokenId,
    /// end double quote token
    pub end_end: Option<TokenId>,

    /// string slice list
    pub list: NodeId,
}

#[derive(Debug)]
pub struct SubShell {
    /// subshell range
    pub span: Span,
    /// shell open token
    pub start_token: TokenId,
    /// shell close token
    pub end_token: Option<TokenId>,

    /// command
    pub cmd: NodeId
}

#[derive(Debug)]
pub struct Chain {
    /// chain range
    pub span: Span,
    /// chain token
    pub token: TokenId,

    pub stdio: StdioKind,
    pub kind: ChainKind,

    /// command
    pub shell: NodeId,
}

#[derive(Debug)]
pub struct Redirect {
    /// redirect range
    pub span: Span,
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
