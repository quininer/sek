use std::io;
use bumpalo::Bump;
use crossterm::{ queue, style };
use crossterm::style::Color;
use scopeguard::guard;
use if_chain::if_chain;
use crate::util::DynWriter;
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
    fn push<W: io::Write>(&self, shell: &Shell, input: &str, term: &mut W, cursor: &mut Cursor) -> anyhow::Result<()>;
}

impl Colour for Command<'_> {
    fn push<W: io::Write>(&self, shell: &Shell, input: &str, term: &mut W, cursor: &mut Cursor) -> anyhow::Result<()> {
        shell.theme.exe.print(&input[self.exe.0.clone()], term)?;

        todo!()
    }
}
