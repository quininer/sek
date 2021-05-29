mod highlight;
mod lines;
mod path_selector;

use bumpalo::collections::String;
use unicode_width::UnicodeWidthStr;
use crossterm::{ execute, style };
use crate::editor::{ Editor, Mode };
use crate::shell::Shell;
use crate::shell::parser::ParseFailed;
use crate::util::{ Fill, FmtDebug };
pub use lines::render as render_line;


const PROMPT: &str = "~ ";

pub fn render(editor: &mut Editor, shell: &mut Shell)
    -> anyhow::Result<()>
{
    let bump = shell.bump.clone();
    let bump = bump.borrow();

    if let Mode::PathSelector = editor.mode {
        path_selector::render(&bump, editor, shell)
    } else {
        let mut buf = String::with_capacity_in(editor.line.len(), &bump);

        let (cursor, _) = editor.line.read_into_and_width(&mut buf);
        lines::render(&bump, editor, shell, &mut buf, cursor)
    }
}

pub fn report(_editor: &mut Editor, shell: &mut Shell, line: &str, err: ParseFailed) -> anyhow::Result<()> {
    let fill = line[..err.span.start].width() as u16;
    let flag = line[err.span.clone()].width() as u16;

    let mut term = shell.term.lock();

    execute!(
        term,
        style::Print(Fill::empty(PROMPT.len() as u16 + fill)),
        style::SetForegroundColor(style::Color::Red),
        style::Print(Fill::flag(flag)),
        style::SetForegroundColor(style::Color::Reset),
        style::Print("\r\nSyntax error: "),
        style::Print(FmtDebug(&err.token)),
        style::Print(" - "),
        style::Print(err.kind.as_str()),
        style::Print("\r\n"),
    )?;

    Ok(())
}
