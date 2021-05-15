use bumpalo::boxed::Box;
use bumpalo::collections::Vec;
use logos::Span;


#[derive(Debug)]
pub struct Command<'c> {
    pub exe: Literal,
    pub args: Vec<'c, Argument<'c>>,
    pub redirect: Vec<'c, Redirect<'c>>,
    pub chain: Option<Chain<'c>>,
}

#[derive(Debug)]
pub struct Literal(pub Span);

#[derive(Debug)]
pub struct Env(pub Span);

#[derive(Debug)]
pub struct Escape(pub Span);

#[derive(Debug)]
pub struct SubShell<'c> {
    pub span: Span,
    pub cmd: Box<'c, Command<'c>>
}

#[derive(Debug)]
pub struct ChainShell<'c>(pub Box<'c, Command<'c>>);

#[derive(Debug)]
pub struct Chain<'c> {
    pub span: Span,
    pub kind: ChainKind,
    pub shell: ChainShell<'c>
}

#[derive(Debug)]
pub enum ChainKind {
    Pipe,
    Then,
    AndIf,
    OrIf
}

#[derive(Debug)]
pub struct SingleStr(pub Span);

#[derive(Debug)]
pub struct DoubleStr<'c> {
    pub span: Span,
    pub list: Vec<'c, StrSlice<'c>>,
}

#[derive(Debug)]
pub enum StrSlice<'c> {
    Str(Literal),
    Env(Env),
    Escape(Escape),
    SubShell(SubShell<'c>)
}

#[derive(Debug)]
pub struct Argument<'c>(pub Vec<'c, ArgSlice<'c>>);

#[derive(Debug)]
pub enum ArgSlice<'c> {
    Str(Literal),
    Env(Env),
    Escape(Escape),
    Single(SingleStr),
    Double(DoubleStr<'c>),
    SubShell(SubShell<'c>)
}

#[derive(Debug)]
pub struct Redirect<'c> {
    pub kind: StdioKind,
    pub append: bool,
    pub value: Argument<'c>
}

#[derive(Clone, Copy, Debug)]
pub enum StdioKind {
    Out,
    Err
}
