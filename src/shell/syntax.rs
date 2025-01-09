pub mod error;
pub mod token;
pub mod raw;
pub mod parse;

use logos::Span;
use token::Token;
pub use parse::Parser;
pub use raw::{ ChainKind, StdioKind };

#[derive(Clone, Copy)]
pub struct Command(raw::NodeId);

#[derive(Clone, Copy)]
pub struct Argument(raw::NodeId);

#[derive(Clone, Copy)]
pub struct Literal(raw::NodeId);

#[derive(Clone, Copy)]
pub struct Variable(raw::NodeId);

#[derive(Clone, Copy)]
pub struct Escape(raw::NodeId);

#[derive(Clone, Copy)]
pub struct SingleStr(raw::NodeId);

#[derive(Clone, Copy)]
pub struct DoubleStr(raw::NodeId);

#[derive(Clone, Copy)]
pub struct SubShell(raw::NodeId);

#[derive(Clone, Copy)]
pub struct Chain(raw::NodeId);

#[derive(Clone, Copy)]
pub struct Redirect(raw::NodeId);

pub enum ArgSlice {
    Literal(Literal),
    Variable(Variable),
    Escape(Escape),
    SingleStr(SingleStr),
    DoubleStr(DoubleStr),
    SubShell(SubShell)
}

pub enum StrSlice {
    Literal(Literal),
    Variable(Variable),
    Escape(Escape),
    SubShell(SubShell)
}

struct Link<'p> {
    parser: &'p Parser,
    link: Option<raw::NodeId>
}

impl Link<'_> {
    fn new(parser: &Parser, link: raw::NodeId) -> Link<'_> {
        Link { parser, link: Some(link) } 
    }
}

impl Iterator for Link<'_> {
    type Item = raw::NodeId;

    fn next(&mut self) -> Option<Self::Item> {
        let link = self.link.take()?;
        let link = match &self.parser.nodes[link] {
            raw::Node::Link(link) => link,
            raw::Node::Null => return None,
            _ => unreachable!()
        };
        self.link = link.next;
        Some(link.current)
    }
}

impl Command {
    pub fn exe(self, parser: &Parser) -> Option<Literal> {
        let cmd = matches2!(&parser.nodes[self.0], raw::Node::Command).unwrap();
        matches2!(&parser.nodes[cmd.exe], raw::Node::Literal)?;
        Some(Literal(cmd.exe))
    }

    pub fn args(self, parser: &Parser)
        -> impl Iterator<Item = Argument> + use<'_>
    {
        let cmd = matches2!(&parser.nodes[self.0], raw::Node::Command).unwrap();
        Link::new(parser, cmd.args).map(Argument)
    }

    pub fn redirect(self, parser: &Parser)
        -> impl Iterator<Item = Redirect> + use<'_>
    {
        let cmd = matches2!(&parser.nodes[self.0], raw::Node::Command).unwrap();
        Link::new(parser, cmd.args).map(Redirect)
    }

    pub fn chain(self, parser: &Parser)
        -> Option<Chain>
    {
        let cmd = matches2!(&parser.nodes[self.0], raw::Node::Command).unwrap();
        matches2!(&parser.nodes[cmd.chain], raw::Node::Chain)?;
        Some(Chain(cmd.chain))
    }
}

impl Argument {
    pub fn slice(self, parser: &Parser)
        -> impl Iterator<Item = ArgSlice> + use<'_>
    {
        Link::new(parser, self.0)
            .map(move |id| match &parser.nodes[id] {
                raw::Node::Literal(_) => ArgSlice::Literal(Literal(id)),
                raw::Node::Variable(_) => ArgSlice::Variable(Variable(id)),
                raw::Node::Escape(_) => ArgSlice::Escape(Escape(id)),
                raw::Node::SingleStr(_) => ArgSlice::SingleStr(SingleStr(id)),
                raw::Node::DoubleStr(_) => ArgSlice::DoubleStr(DoubleStr(id)),
                raw::Node::SubShell(_) => ArgSlice::SubShell(SubShell(id)),
                _ => unreachable!()
            })
    }
}

impl DoubleStr {
    pub fn slice(self, parser: &Parser)
        -> impl Iterator<Item = StrSlice> + use<'_>
    {
        Link::new(parser, self.0)
            .map(move |id| match &parser.nodes[id] {
                raw::Node::Literal(_) => StrSlice::Literal(Literal(id)),
                raw::Node::Variable(_) => StrSlice::Variable(Variable(id)),
                raw::Node::Escape(_) => StrSlice::Escape(Escape(id)),
                raw::Node::SubShell(_) => StrSlice::SubShell(SubShell(id)),
                _ => unreachable!()
            })
    }
}

impl Literal {
    pub fn span(self, parser: &Parser) -> Span {
        let lit = matches2!(&parser.nodes[self.0], raw::Node::Literal).unwrap();
        lit.0.clone()
    }
}

impl Variable {
    pub fn span(self, parser: &Parser) -> Span {
        let var = matches2!(&parser.nodes[self.0], raw::Node::Variable).unwrap();
        let (token, span) = &parser.tokens[var.0];
        assert_eq!(token, &Token::Text);
        span.clone()
    }
}

impl Escape {
    pub fn backslash(self, parser: &Parser) -> Span {
        let escape = matches2!(&parser.nodes[self.0], raw::Node::Escape).unwrap();
        let (token, span) = &parser.tokens[escape.backslash];
        assert_eq!(token, &Token::Backslash);
        span.clone()       
    }
    
    pub fn value(self, parser: &Parser) -> Span {
        let escape = matches2!(&parser.nodes[self.0], raw::Node::Escape).unwrap();
        let (token, span) = &parser.tokens[escape.value];
        assert_eq!(token, &Token::Text);
        span.clone()
    }
}

impl SingleStr {
    pub fn start(self, parser: &Parser) -> Span {
        let s = matches2!(&parser.nodes[self.0], raw::Node::SingleStr).unwrap();
        let (token, span) = &parser.tokens[s.start_token];
        assert_eq!(token, &Token::SingleQuote);
        span.clone()
    }

    pub fn end(self, parser: &Parser) -> Option<Span> {
        let s = matches2!(&parser.nodes[self.0], raw::Node::SingleStr).unwrap();
        let (token, span) = &parser.tokens[s.end_token?];
        assert_eq!(token, &Token::SingleQuote);
        Some(span.clone())
    }
}

impl SubShell {
    pub fn start(self, parser: &Parser) -> Span {
        let subshell = matches2!(&parser.nodes[self.0], raw::Node::SubShell).unwrap();
        let (token, span) = &parser.tokens[subshell.start_token];
        assert_eq!(token, &Token::ShellOpen);
        span.clone()
    }

    pub fn end(self, parser: &Parser) -> Option<Span> {
        let subshell = matches2!(&parser.nodes[self.0], raw::Node::SubShell).unwrap();
        let (token, span) = &parser.tokens[subshell.end_token?];
        assert_eq!(token, &Token::ShellClose);
        Some(span.clone())
    }

    pub fn command(self, parser: &Parser) -> Command {
        let subshell = matches2!(&parser.nodes[self.0], raw::Node::SubShell).unwrap();
        Command(subshell.cmd)
    }
}

impl Chain {
    pub fn token(self, parser: &Parser) -> Span {
        let chain = matches2!(&parser.nodes[self.0], raw::Node::Chain).unwrap();
        let (token, span) = &parser.tokens[chain.token];
        assert!(matches!(token, Token::Pipe | Token::Then | Token::AndIf | Token::OrIf));
        span.clone()
    }

    pub fn kind(self, parser: &Parser) -> ChainKind {
        let chain = matches2!(&parser.nodes[self.0], raw::Node::Chain).unwrap();
        chain.kind
    }

    pub fn command(self, parser: &Parser) -> Command {
        let chain = matches2!(&parser.nodes[self.0], raw::Node::Chain).unwrap();
        Command(chain.shell)
    }        
}

impl Redirect {
    pub fn token(self, parser: &Parser) -> Span {
        let redirect = matches2!(&parser.nodes[self.0], raw::Node::Redirect).unwrap();
        let (token, span) = &parser.tokens[redirect.token];
        assert!(matches!(token, Token::Redirect));
        span.clone()
    }

    pub fn kind(self, parser: &Parser) -> StdioKind {
        let redirect = matches2!(&parser.nodes[self.0], raw::Node::Redirect).unwrap();
        redirect.kind
    }

    pub fn append(self, parser: &Parser) -> bool {
        let redirect = matches2!(&parser.nodes[self.0], raw::Node::Redirect).unwrap();
        redirect.append
    }

    pub fn value(self, parser: &Parser) -> Argument {
        let redirect = matches2!(&parser.nodes[self.0], raw::Node::Redirect).unwrap();
        Argument(redirect.value)
    }
}
