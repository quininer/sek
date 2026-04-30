use std::cmp;
use logos::Span;
use crossterm::{ queue, style };
use crossterm::style::{ Color, Attributes };
use crate::editor::Mode;
use crate::shell::Shell;
use crate::config::Style;
use crate::ui::render::Fill;
use crate::util::{ RefWriter, ScopeGuard };
use super::{
    ArgSlice, Argument, Chain, Command, DoubleStr, Escape, Literal,
    Parser, Redirect, SingleStr, StrSlice, SubShell, Variable
};


pub fn colour(shell: &Shell, cmd: Command, input: &str, term: RefWriter<'_>)
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
        shell, parser: &shell.parser,
        buf: input
    };
    let mut state = State::default();

    cmd.colour(&mut state, input, term.reborrow())?;

    if let Some(tail) = input.buf.get(state.current..)
        .filter(|buf| !buf.is_empty())
    {
        let pos = tail
            .find('#')
            .unwrap_or(tail.len());

        Comment(state.current + pos..input.buf.len())
            .colour(&mut state, input, term.reborrow())?;
    }

    Ok(())
}

#[derive(Clone, Copy)]
struct Input<'a> {
    shell: &'a Shell,
    parser: &'a Parser,
    buf: &'a str,
}

#[derive(Default)]
struct State {
    current: usize,
    color: Option<Color>,
    background: Option<Color>,
    attr: Option<Attributes>,
    is_doublestr: bool,
}

struct SelectedBoundary {
    head: Span,
    selected: Span,
    tail: Span
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

    fn set_background_color(&mut self, color: Option<Color>, mut term: RefWriter<'_>) -> anyhow::Result<()> {
        if color != self.background {
            queue!(term, style::SetBackgroundColor(color.unwrap_or(Color::Reset)))?;
            self.background = color;
        }

        Ok(())
    }


    fn fill(&mut self,
        shell: &Shell,
        new_start: usize,
        selected: Span,
        mut term: RefWriter<'_>
    ) -> anyhow::Result<()> {
        if self.current < new_start {
            let boundary = selected_boundary(self.current..new_start, selected);

            self.set_style(Style::default(), term.reborrow())?;

            if !boundary.head.is_empty() {
                self.set_background_color(None, term.reborrow())?;
                queue!(term, style::Print(Fill(' ', boundary.head.len())))?;
            }

            if !boundary.selected.is_empty() {
                self.set_background_color(shell.config.theme.selected.color(), term.reborrow())?;
                queue!(term, style::Print(Fill(' ', boundary.selected.len())))?;
            }

            if !boundary.tail.is_empty() {
                self.set_background_color(None, term.reborrow())?;
                queue!(term, style::Print(Fill(' ', boundary.tail.len())))?;
            }

            self.current = new_start;
        }

        Ok(())        
    }
    
    fn push_to(&mut self, style: Style, input: Input<'_>, span: Span, mut term: RefWriter<'_>)
        -> anyhow::Result<()>
    {
        let insert_cursor = matches!(input.shell.editor.mode, Mode::Visual)
            .then(|| input.shell.editor.insert.cursor_inclusive())
            .unwrap_or_else(|| input.shell.editor.insert.cursor());
        let selected = input.shell.editor.insert.span(insert_cursor);
        self.fill(input.shell, span.start, selected.clone(), term.reborrow())?;

        let boundary = selected_boundary(span.clone(), selected);

        self.set_style(style, term.reborrow())?;

        if !boundary.head.is_empty() {
            self.set_background_color(None, term.reborrow())?;
            queue!(term, style::Print(&input.buf[boundary.head]))?;
        }

        if !boundary.selected.is_empty() {
            self.set_background_color(input.shell.config.theme.selected.color(), term.reborrow())?;
            queue!(term, style::Print(&input.buf[boundary.selected]))?;
        }

        if !boundary.tail.is_empty() {
            self.set_background_color(None, term.reborrow())?;
            queue!(term, style::Print(&input.buf[boundary.tail]))?;
        }        

        self.current += span.len();

        Ok(())
    }
}

struct Exe(Argument);
struct Comment(Span);

impl Command {
    fn colour(self, state: &mut State, input: Input<'_>, mut term: RefWriter<'_>) -> anyhow::Result<()> {
        let exe = self.exe(&input.shell.parser);
        Exe(exe).colour(state, input, term.reborrow())?;

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
        // TODO check lit and exist

        self.0.colour(state, input, term)
    }   
}

impl Literal {
    fn colour(self, state: &mut State, input: Input<'_>, term: RefWriter<'_>) -> anyhow::Result<()> {
        let style = if state.is_doublestr
            { input.shell.config.theme.double_str }
            else { input.shell.config.theme.literal };
        state.push_to(style, input, self.span(input.parser), term)        
    }
}

impl Variable {
    fn colour(self, state: &mut State, input: Input<'_>, term: RefWriter<'_>) -> anyhow::Result<()> {
        let color = if input.buf[self.span(input.parser)]
            .strip_prefix('$')
            .and_then(|name| input.shell.env.get(name.as_ref()))
            .is_some()
        {
            input.shell.config.theme.variable
        } else {
            input.shell.config.theme.error
        };

        state.push_to(color, input, self.span(input.parser), term)        
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


fn selected_boundary(span: Span, selected_span: Span) -> SelectedBoundary {
    let head = span.start..cmp::min(selected_span.start, span.end);
    let selected = cmp::max(selected_span.start, span.start)
        ..cmp::min(selected_span.end, span.end);
    let tail = cmp::max(selected_span.end, span.start)..span.end;

    assert_eq!(span.len(), head.len() + selected.len() + tail.len(), "{:?} vs {:?}", span, (head, selected, tail));

    SelectedBoundary { head, selected, tail }
}    

#[test]
fn test_selected_boundary() {
    // [ ( ) ]
    let boundary = selected_boundary(0..10, 3..7);
    assert_eq!(boundary.head, 0..3);
    assert_eq!(boundary.selected, 3..7);
    assert_eq!(boundary.tail, 7..10);

    // [ | ]
    let boundary = selected_boundary(0..10, 3..3);
    assert_eq!(boundary.head, 0..3);
    assert_eq!(boundary.selected, 3..3);
    assert_eq!(boundary.tail, 3..10);

    // [ ( ] )
    let boundary = selected_boundary(0..7, 3..10);
    assert_eq!(boundary.head, 0..3);
    assert_eq!(boundary.selected, 3..7);
    assert_eq!(boundary.tail, 10..7);

    // ( [ ) ]
    let boundary = selected_boundary(3..10, 0..7);
    assert_eq!(boundary.head, 3..0);
    assert_eq!(boundary.selected, 3..7);
    assert_eq!(boundary.tail, 7..10);

    // ( [ ] )
    let boundary = selected_boundary(3..7, 0..10);
    assert_eq!(boundary.head, 3..0);
    assert_eq!(boundary.selected, 3..7);
    assert_eq!(boundary.tail, 10..7);

    // [ ] ( )
    let boundary = selected_boundary(0..3, 7..10);
    assert_eq!(boundary.head, 0..3);
    assert_eq!(boundary.selected, 7..3);
    assert_eq!(boundary.tail, 10..3);

    // ( ) [ ]
    let boundary = selected_boundary(7..10, 0..3);
    assert_eq!(boundary.head, 7..0);
    assert_eq!(boundary.selected, 7..3);
    assert_eq!(boundary.tail, 7..10);    
}
