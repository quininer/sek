mod highlight;
mod lines;
mod path_selector;

use bumpalo::collections::String;
use unicode_width::UnicodeWidthStr;
use crossterm::{ queue, execute, style, terminal };
use crate::editor::{ Editor, Mode };
use crate::shell::Shell;
use crate::shell::parser::ParseFailed;
use crate::util::{ Fill, FmtDebug };
pub use lines::render as render_line;
pub use path_selector::RESERVE_SPACE as PATH_SELECTOR_RESERVE_SPACE;


const PROMPT: &str = "~ ";

pub fn render(editor: &mut Editor, shell: &mut Shell)
    -> anyhow::Result<()>
{
    let bump = shell.bump.clone();
    let bump = bump.borrow();

    let title = bumpalo::format!(in &bump,
        "sek {}",
        shell.env.pwd().display()
    );
    queue!(&mut shell.term, terminal::SetTitle(&title))?;

    if let Mode::PathSelector = editor.mode {
        path_selector::render(&bump, editor, shell)
    } else {
        let mut buf = String::with_capacity_in(editor.line.len(), &bump);

        let (cursor_width, _) = editor.line.read_into_and_width(&mut buf);
        lines::render(&bump, editor, shell, &mut buf, cursor_width)
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
