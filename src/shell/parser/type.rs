use bumpalo::boxed::Box;
use bumpalo::collections::Vec;
use logos::Span;


#[derive(Debug)]
pub struct Command<'c> {
    pub exe: Literal,
    pub args: Vec<'c, Argument<'c>>,
    pub chain: Option<Chain<'c>>,
    pub redirect: Vec<'c, Redirect<'c>>,
}

#[derive(Debug)]
pub struct Literal(pub Span);

#[derive(Debug)]
pub struct Env(pub Span);

#[derive(Debug)]
pub struct SubShell<'c>(pub Box<'c, Command<'c>>);

#[derive(Debug)]
pub struct ChainShell<'c>(pub Box<'c, Command<'c>>);

#[derive(Debug)]
pub enum Chain<'c> {
    Pipe(ChainShell<'c>),
    Then(ChainShell<'c>),
    AndIf(ChainShell<'c>),
    OrIf(ChainShell<'c>)
}

#[derive(Debug)]
pub struct SingleStr(pub Span);

#[derive(Debug)]
pub struct DoubleStr<'c>(pub Vec<'c, StrSlice<'c>>);

#[derive(Debug)]
pub enum StrSlice<'c> {
    Str(Literal),
    Env(Env),
    SubShell(SubShell<'c>)
}

#[derive(Debug)]
pub struct Argument<'c>(pub Vec<'c, ArgSlice<'c>>);

#[derive(Debug)]
pub enum ArgSlice<'c> {
    Str(Literal),
    Env(Env),
    Single(SingleStr),
    Double(DoubleStr<'c>),
    SubShell(SubShell<'c>)
}

#[derive(Debug)]
pub struct Redirect<'c> {
    pub ty: StdioType,
    pub append: bool,
    pub value: Argument<'c>
}

#[derive(Clone, Copy, Debug)]
pub enum StdioType {
    Out,
    Err
}
