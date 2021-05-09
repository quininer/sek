use std::io;
use bumpalo::Bump;
use crossterm::{ queue, style };
use crossterm::style::Color;
use scopeguard::guard;
use if_chain::if_chain;
use crate::util::DynWriter;
use crate::shell::parser::Token;


pub fn colour(bump: &Bump, term: &mut dyn io::Write, input: &str) -> anyhow::Result<()> {
    let mut term = DynWriter(term);
    let mut term = guard(term, |mut term| {
        let _ = queue!(term, style::SetForegroundColor(Color::Reset));
    });

    todo!()
}
