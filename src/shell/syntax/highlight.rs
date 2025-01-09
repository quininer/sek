use logos::Span;
use crossterm::{ queue, style };
use crossterm::style::{ Color, Attributes };
use crate::config::Style;
use crate::shell::Shell;
use crate::ui::render::Fill;
use crate::util::{ RefWriter, ScopeGuard };
use super::{ ArgSlice, Argument, Chain, Command, DoubleStr, Escape, Literal, Parser, Redirect, SingleStr, StrSlice, SubShell, Variable };


pub fn colour(shell: &Shell, parser: &Parser, cmd: Command, input: &str, term: RefWriter<'_>)
    -> anyhow::Result<()>
{
    let mut term = ScopeGuard(term, |term| {
        let _ = queue!(term,
            style::ResetColor,
            style::SetAttribute(style::Attribute::Reset),
        );
    });
    let term = term.as_mut();

    let input = Input {
        shell, parser,
        buf: input
    };
    let mut state = State {
        current: 0,
        color: None,
        attr: None,
        is_doublestr: false,
    };

    cmd.colour(&mut state, input, term.reborrow())?;

    if state.current < input.buf.len() {
        if let Some(pos) = input.buf[state.current..].find('#') {
            Comment(state.current + pos..input.buf.len())
                .colour(&mut state, input, term.reborrow())?;
        }
    }

    Ok(())
}

#[derive(Clone, Copy)]
struct Input<'a> {
    shell: &'a Shell,
    parser: &'a Parser,
    buf: &'a str,
}

struct State {
    current: usize,
    color: Option<Color>,
    attr: Option<Attributes>,
    is_doublestr: bool,
}

impl State {
    fn set_style(&mut self, style: Style, mut term: RefWriter<'_>) -> anyhow::Result<()> {
        let color = style.color();
        let attr = style.attr();

        if color != self.color {
            queue!(term, style::SetForegroundColor(color.unwrap_or(Color::Reset)))?;
            self.color = color;
        }

        if attr != self.attr {
            if let Some(attr) = attr {
                queue!(term, style::SetAttributes(attr))?;
            } else {
                queue!(term, style::SetAttribute(style::Attribute::Reset))?;
            }

            self.attr = attr;
        }

        Ok(())
    }

    fn fill(&mut self, new_span: Span, mut term: RefWriter<'_>) -> anyhow::Result<()> {
        if let Some(len) = new_span.start.checked_sub(self.current) {
            self.set_style(Style::default(), term.reborrow())?;
            queue!(term, style::Print(Fill(' ', len)))?;
            self.current = new_span.start;
        }

        Ok(())        
    }
    
    fn push_to(&mut self, style: Style, input: Input<'_>, span: Span, mut term: RefWriter<'_>)
        -> anyhow::Result<()>
    {
        self.fill(span.clone(), term.reborrow())?;
        self.set_style(style, term.reborrow())?;
        queue!(term, style::Print(&input.buf[span.clone()]))?;
        self.current += span.len();

        Ok(())
    }
}

struct Exe(Span);
struct Comment(Span);

impl Command {
    fn colour(self, state: &mut State, input: Input<'_>, mut term: RefWriter<'_>) -> anyhow::Result<()> {
        let lit = self.exe(&input.shell.parser);
        Exe(lit.span(&input.shell.parser)).colour(state, input, term.reborrow())?;

        for arg in self.args(&input.shell.parser) {
            arg.colour(state, input, term.reborrow())?;
        }

        for redirect in self.redirect(input.parser) {
            redirect.colour(state, input, term.reborrow())?;
        }

        if let Some(chain) = self.chain(input.parser) {
            chain.colour(state, input, term)?;
        }

        Ok(())
    }
}

impl Exe {
    fn colour(self, state: &mut State, input: Input<'_>, term: RefWriter<'_>) -> anyhow::Result<()> {
        state.push_to(input.shell.config.theme.exe, input, self.0, term)
    }   
}

impl Literal {
    fn colour(self, state: &mut State, input: Input<'_>, term: RefWriter<'_>) -> anyhow::Result<()> {
        let style = state.is_doublestr
            .then_some(input.shell.config.theme.double_str)
            .unwrap_or(input.shell.config.theme.literal);
        state.push_to(style, input, self.span(input.parser), term)        
    }
}

impl Variable {
    fn colour(self, state: &mut State, input: Input<'_>, term: RefWriter<'_>) -> anyhow::Result<()> {
        // TODO check env

        state.push_to(input.shell.config.theme.variable, input, self.span(input.parser), term)        
    }   
}

impl Escape {
    fn colour(self, state: &mut State, input: Input<'_>, term: RefWriter<'_>) -> anyhow::Result<()> {
        let backslash = self.backslash(input.parser);
        let value = self.value(input.parser);
        let span = backslash.start..value.end;

        state.push_to(input.shell.config.theme.escape, input, span, term)
    }   
}

impl SingleStr {
    fn colour(self, state: &mut State, input: Input<'_>, term: RefWriter<'_>) -> anyhow::Result<()> {
        let start = self.start(input.parser).start;
        let end = self.end(input.parser)
            .map(|span| span.end)
            .unwrap_or_else(|| input.buf.len());
        let span = start..end;

        state.push_to(input.shell.config.theme.escape, input, span, term)
    }
}

impl Argument {
    fn colour(self, state: &mut State, input: Input<'_>, mut term: RefWriter<'_>) -> anyhow::Result<()> {
        for arg in self.slice(input.parser) {
            match arg {
                ArgSlice::Literal(node) => node.colour(state, input, term.reborrow())?,
                ArgSlice::Variable(node) => node.colour(state, input, term.reborrow())?,
                ArgSlice::Escape(node) => node.colour(state, input, term.reborrow())?,
                ArgSlice::SingleStr(node) => node.colour(state, input, term.reborrow())?,
                ArgSlice::DoubleStr(node) => node.colour(state, input, term.reborrow())?,
                ArgSlice::SubShell(node) => node.colour(state, input, term.reborrow())?,
            }
        }

        Ok(())        
    }
}

impl DoubleStr {
    fn colour(self, state: &mut State, input: Input<'_>, mut term: RefWriter<'_>) -> anyhow::Result<()> {
        state.is_doublestr = true;
        let mut state = ScopeGuard(state, |state| state.is_doublestr = false);
        let state = state.as_mut();

        let start_token = self.start(input.parser);
        state.push_to(input.shell.config.theme.double_str, input, start_token, term.reborrow())?;

        for seg in self.slice(input.parser) {
            match seg {
                StrSlice::Literal(node) => node.colour(state, input, term.reborrow())?,
                StrSlice::Variable(node) => node.colour(state, input, term.reborrow())?,
                StrSlice::Escape(node) => node.colour(state, input, term.reborrow())?,
                StrSlice::SubShell(node) => node.colour(state, input, term.reborrow())?,
            }
        }

        if let Some(end_token) = self.end(input.parser) {
            state.push_to(input.shell.config.theme.double_str, input, end_token, term)?;
        }

        Ok(())
    }
}

impl SubShell {
    fn colour(self, state: &mut State, input: Input<'_>, mut term: RefWriter<'_>) -> anyhow::Result<()> {
        let start_token = self.start(input.parser);
        state.push_to(input.shell.config.theme.subshell, input, start_token, term.reborrow())?;

        self.command(input.parser).colour(state, input, term.reborrow())?;

        if let Some(end_token) = self.end(input.parser) {
            state.push_to(input.shell.config.theme.subshell, input, end_token, term)?;
        }

        Ok(())        
    }
}

impl Redirect {
    fn colour(self, state: &mut State, input: Input<'_>, mut term: RefWriter<'_>) -> anyhow::Result<()> {
        let token = self.token(input.parser);
        state.push_to(input.shell.config.theme.redirect, input, token, term.reborrow())?;

        self.value(input.parser).colour(state, input, term)
    }
}

impl Chain {
    fn colour(self, state: &mut State, input: Input<'_>, mut term: RefWriter<'_>) -> anyhow::Result<()> {
        let token = self.token(input.parser);
        state.push_to(input.shell.config.theme.chain, input, token, term.reborrow())?;

        self.command(input.parser).colour(state, input, term)        
    }
}

impl Comment {
    fn colour(self, state: &mut State, input: Input<'_>, term: RefWriter<'_>) -> anyhow::Result<()> {
        state.push_to(input.shell.config.theme.comment, input, self.0, term)
    }
}
