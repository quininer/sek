use std::{ io, mem };
use bumpalo::Bump;
use crossterm::{ queue, style };
use crossterm::style::Color;
use scopeguard::guard;
use if_chain::if_chain;
use crate::util::{ Fill, DynWriter };
use crate::shell::Shell;
use crate::shell::parser::Token;
use crate::shell::parser::type_::*;


pub fn colour(shell: &Shell, input: &str, term: &mut dyn io::Write, cmd: &Command<'_>) -> anyhow::Result<()> {
    let mut term = DynWriter(term);
    let mut term = guard(term, |mut term| {
        let _ = queue!(term, style::SetForegroundColor(Color::Reset));
    });
    let mut cursor = Cursor(0);

    cmd.push(shell, input, &mut *term, &mut cursor)?;

    todo!()
}

struct Cursor(usize);

trait Colour {
    fn push<W: io::Write>(&self, shell: &Shell, input: &str, term: &mut W, cursor: &mut Cursor)
        -> anyhow::Result<()>;
}

impl Cursor {
    fn fill<W: io::Write>(&mut self, to: usize, term: &mut W) -> anyhow::Result<()> {
        if self.0 < to {
            let len = to - mem::replace(&mut self.0, to);
            queue!(term, style::Print(Fill::empty(len as _)))?;
        }

        Ok(())
    }
}

impl Colour for Command<'_> {
    fn push<W: io::Write>(&self, shell: &Shell, input: &str, mut term: &mut W, cursor: &mut Cursor)
        -> anyhow::Result<()>
    {
        cursor.fill(self.exe.0.start, &mut *term)?;
        shell.theme.exe.print(&input[self.exe.0.clone()], &mut *term)?;

        for arg in self.args.iter() {
            arg.push(shell, input, &mut *term, cursor)?;
        }

        todo!()
    }
}

impl Colour for Argument<'_> {
    fn push<W: io::Write>(&self, shell: &Shell, input: &str, term: &mut W, cursor: &mut Cursor) -> anyhow::Result<()> {
        for slice in self.0.iter() {
            /*
            match slice {
                ArgSlice::Str(val) => val.push(shell, input, term, cursor)?,
                ArgSlice::Env(val) => val.push(shell, input, term, cursor)?,
                ArgSlice::Escape(val) => val.push(shell, input, term, cursor)?,
                ArgSlice::SingleStr(val) => val.push(shell, input, term, cursor)?,
                ArgSlice::DoubleStr(val) => val.push(shell, input, term, cursor)?,
                ArgSlice::SubShell(val) => val.push(shell, input, term, cursor)?,
            }
            */
        }

        Ok(())
    }
}

impl Colour for Literal {
    fn push<W: io::Write>(&self, shell: &Shell, input: &str, term: &mut W, cursor: &mut Cursor) -> anyhow::Result<()> {
        cursor.fill(self.0.start, term)?;

        todo!()
    }
}
