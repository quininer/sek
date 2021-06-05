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
    pub cmd: Box<'c, Command<'c>>,
    pub is_closed: bool
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
    pub is_closed: bool
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
    pub span: Span,
    pub kind: StdioKind,
    pub append: bool,
    pub value: Argument<'c>
}

#[derive(Clone, Copy, Debug)]
pub enum StdioKind {
    Out,
    Err
}

impl Argument<'_> {
    pub fn span(&self) -> Span {
        let start = self.0.first().map(|arg| arg.span());
        let end = self.0.last().map(|arg| arg.span());

        match (start, end) {
            (Some(start), Some(end)) => start.start..end.end,
            (Some(span), None) | (None, Some(span)) => span,
            (None, None) => 0..0
        }
    }
}

impl ArgSlice<'_> {
    pub fn span(&self) -> Span {
        match self {
            ArgSlice::Str(val) => val.0.clone(),
            ArgSlice::Env(val) => val.0.clone(),
            ArgSlice::Escape(val) => val.0.clone(),
            ArgSlice::Single(val) => val.0.clone(),
            ArgSlice::Double(val) => val.span.clone(),
            ArgSlice::SubShell(val) => val.span.clone()
        }
    }
}
