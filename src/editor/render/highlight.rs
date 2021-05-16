use std::{ io, mem };
use logos::Span;
use crossterm::{ queue, style };
use crossterm::style::{ Color, Attributes };
use unicode_width::{ UnicodeWidthStr, UnicodeWidthChar };
use scopeguard::guard;
use crate::util::{ Fill, DynWriter };
use crate::shell;
use crate::shell::config::Style;
use crate::shell::parser::type_::*;


pub struct ShellRef<'a> {
    pub env: &'a shell::Env,
    pub theme: &'a shell::Theme,
    pub prompt_len: usize,
    pub columns: usize,
    pub cursor_position: usize
}

pub fn colour(shell: ShellRef<'_>, input: &str, term: &mut dyn io::Write, cmd: &Command<'_>) -> anyhow::Result<()> {
    let term = DynWriter(term);
    let mut term = guard(term, |mut term| {
        let _ = queue!(term,
            style::ResetColor,
            style::SetAttribute(style::Attribute::Reset),
        );
    });

    let line_end = shell.prompt_len;
    let mut state = State {
        shell, line_end,
        color: None,
        attr: None,
        is_doublestr: false,
        bytes_count: 0,
        line_count: 0
    };

    cmd.colour(input, &mut state, &mut *term)?;

    Ok(())
}

struct State<'a> {
    shell: ShellRef<'a>,
    color: Option<Color>,
    attr: Option<Attributes>,
    is_doublestr: bool,
    bytes_count: usize,
    line_count: usize,
    line_end: usize
}

impl State<'_> {
    fn start<W: io::Write>(&mut self, style: Style, term: &mut W) -> anyhow::Result<()> {
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

    fn push<C, W>(&mut self, mut chars: C, term: &mut W) -> anyhow::Result<()>
    where
        C: Iterator<Item = char>,
        W: io::Write
    {
        for c in chars {
            queue!(term, style::Print(c))?;
            self.bytes_count += c.len_utf8();
        }

        Ok(())
    }
}

struct Exe(Span);

struct Empty(Span);

impl Command<'_> {
    fn colour<W: io::Write>(&self, line: &str, state: &mut State<'_>, term: &mut W) -> anyhow::Result<()> {
        Exe(self.exe.0.clone()).colour(line, state, term)?;

        for arg in self.args.iter() {
            arg.colour(line, state, term)?;
        }

        for redirect in self.redirect.iter() {
            redirect.colour(line, state, term)?;
        }

        if let Some(chain) = self.chain.as_ref() {
            chain.colour(line, state, term)?;
        }

        Ok(())
    }
}

impl Exe {
    fn colour<W: io::Write>(&self, line: &str, state: &mut State<'_>, term: &mut W) -> anyhow::Result<()> {
        Empty(state.bytes_count..self.0.start)
            .colour(line, state, term)?;

        let name = &line[self.0.clone()];
        let theme = if state.shell.env.exists(name.as_bytes()) {
            state.shell.theme.exe
        } else {
            state.shell.theme.error
        };

        state.start(theme, term)?;
        state.push(name.chars(), term)?;

        Ok(())
    }
}

impl Empty {
    fn colour<W: io::Write>(&self, line: &str, state: &mut State<'_>, term: &mut W) -> anyhow::Result<()> {
        if self.0.start < self.0.end {
            state.start(Style::default(), term)?;
            state.push(self.0.clone().map(|_| ' '), term)?;
        }

        Ok(())
    }
}

impl Literal {
    fn colour<W: io::Write>(&self, line: &str, state: &mut State<'_>, term: &mut W) -> anyhow::Result<()> {
        Empty(state.bytes_count..self.0.start)
            .colour(line, state, term)?;

        let theme = if !state.is_doublestr {
            state.shell.theme.literal
        } else {
            state.shell.theme.double_str
        };
        state.start(theme, term)?;
        state.push(line[self.0.clone()].chars(), term)?;
        Ok(())
    }
}

impl Env {
    fn colour<W: io::Write>(&self, line: &str, state: &mut State<'_>, term: &mut W) -> anyhow::Result<()> {
        Empty(state.bytes_count..self.0.start)
            .colour(line, state, term)?;

        let name = &line[self.0.clone()];
        let name2 = name.strip_prefix('$').unwrap_or(name);
        let theme = if state.shell.env.get(name2.as_ref()).is_some() {
            state.shell.theme.env
        } else {
            state.shell.theme.error
        };
        state.start(theme, term)?;
        state.push(name.chars(), term)?;
        Ok(())
    }
}

impl Escape {
    fn colour<W: io::Write>(&self, line: &str, state: &mut State<'_>, term: &mut W) -> anyhow::Result<()> {
        Empty(state.bytes_count..self.0.start)
            .colour(line, state, term)?;

        state.start(state.shell.theme.escape, term)?;
        state.push(line[self.0.clone()].chars(), term)?;
        Ok(())
    }
}

impl SingleStr {
    fn colour<W: io::Write>(&self, line: &str, state: &mut State<'_>, term: &mut W) -> anyhow::Result<()> {
        Empty(state.bytes_count..self.0.start)
            .colour(line, state, term)?;

        state.start(state.shell.theme.single_str, term)?;
        state.push(line[self.0.clone()].chars(), term)?;
        Ok(())
    }
}

impl SubShell<'_> {
    fn colour<W: io::Write>(&self, line: &str, state: &mut State<'_>, term: &mut W) -> anyhow::Result<()> {
        Empty(state.bytes_count..self.span.start)
            .colour(line, state, term)?;

        let prev_mode = mem::replace(&mut state.is_doublestr, false);
        let mut state = guard(state, |state| state.is_doublestr = prev_mode);
        let state = &mut *state;

        state.start(state.shell.theme.subshell, term)?;
        state.push("$(".chars(), term)?;

        self.cmd.colour(line, state, term)?;

        if self.is_closed {
            Empty(state.bytes_count..(self.span.end - 1))
                .colour(line, state, term)?;

            state.start(state.shell.theme.subshell, term)?;
            state.push(")".chars(), term)?;
        }

        Ok(())
    }
}

impl DoubleStr<'_> {
    fn colour<W: io::Write>(&self, line: &str, state: &mut State<'_>, term: &mut W) -> anyhow::Result<()> {
        Empty(state.bytes_count..self.span.start)
            .colour(line, state, term)?;

        state.is_doublestr = true;
        let mut state = guard(state, |state| state.is_doublestr = false);
        let state = &mut *state;

        state.start(state.shell.theme.double_str, term)?;
        state.push("\"".chars(), term)?;

        for slice in self.list.iter() {
            match slice {
                StrSlice::Str(val) => val.colour(line, state, term)?,
                StrSlice::Env(val) => val.colour(line, state, term)?,
                StrSlice::Escape(val) => val.colour(line, state, term)?,
                StrSlice::SubShell(val) => val.colour(line, state, term)?,
            }
        }

        if self.is_closed {
            state.start(state.shell.theme.double_str, term)?;
            state.push("\"".chars(), term)?;
        }

        Ok(())
    }
}

impl Argument<'_> {
    fn colour<W: io::Write>(&self, line: &str, state: &mut State<'_>, term: &mut W) -> anyhow::Result<()> {
        for slice in self.0.iter() {
            match slice {
                ArgSlice::Str(val) => val.colour(line, state, term)?,
                ArgSlice::Env(val) => val.colour(line, state, term)?,
                ArgSlice::Escape(val) => val.colour(line, state, term)?,
                ArgSlice::Single(val) => val.colour(line, state, term)?,
                ArgSlice::Double(val) => val.colour(line, state, term)?,
                ArgSlice::SubShell(val) => val.colour(line, state, term)?,
            }
        }

        Ok(())
    }
}

impl Redirect<'_> {
    fn colour<W: io::Write>(&self, line: &str, state: &mut State<'_>, term: &mut W) -> anyhow::Result<()> {
        Empty(state.bytes_count..self.span.start)
            .colour(line, state, term)?;

        state.start(state.shell.theme.redirect, term)?;
        state.push(line[self.span.clone()].chars(), term)?;

        self.value.colour(line, state, term)?;

        Ok(())
    }
}

impl Chain<'_> {
    fn colour<W: io::Write>(&self, line: &str, state: &mut State<'_>, term: &mut W) -> anyhow::Result<()> {
        Empty(state.bytes_count..self.span.start)
            .colour(line, state, term)?;

        state.start(state.shell.theme.chain, term)?;
        state.push(line[self.span.clone()].chars(), term)?;

        self.shell.0.colour(line, state, term)?;

        Ok(())
    }
}
