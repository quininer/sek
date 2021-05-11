use std::io::Write;
use if_chain::if_chain;
use bumpalo::collections::String;
use unicode_width::UnicodeWidthStr;
use crossterm::{ queue, execute, cursor, style, terminal };
use crate::editor::{ Editor, State };
use crate::shell::Shell;
use crate::shell::parser::ParseFailed;
use crate::util::{ Fill, FmtDebug };


const PROMPT: &str = "~ ";

pub fn render(editor: &Editor, shell: &mut Shell, execute: bool)
    -> anyhow::Result<()>
{
    let bump = shell.bump.clone();
    let bump = bump.borrow();
    let mut buf = String::with_capacity_in(editor.line.len(), &bump);

    let (cursor, _) = editor.line.ready_render(&mut buf, None);

    let mut term = shell.term.lock();

    match editor.cursor_line.get() {
        0 => queue!(term, cursor::MoveToColumn(0))?,
        n => queue!(term, cursor::MoveToPreviousLine(n))?,
    }

    queue!(
        term,
        terminal::Clear(terminal::ClearType::FromCursorDown),
        style::SetForegroundColor(if shell.last_status {
            style::Color::Grey
        } else {
            style::Color::DarkRed
        }),
        style::Print(PROMPT),
        style::ResetColor,
    )?;

    // TODO colour
    queue!(term, style::Print(&buf))?;

    match editor.state {
        State::Edit => {
            queue!(term, cursor::MoveToColumn(cursor + PROMPT.len() as u16))?;
            editor.cursor_line.set(0);
        },
        State::Command => {
            let size = editor.global.columns.get();
            let (cmdcur, fill) = editor.cmd.ready_render(&mut buf, Some(size));

            queue!(
                term,
                style::Print("\r\n"),
                style::SetColors(style::Colors::new(style::Color::Black, style::Color::White)),
                style::Print(&buf),
                style::Print(fill),
                style::ResetColor,
            )?;

            if editor.cmd.is_empty() {
                queue!(
                    term,
                    cursor::MoveToPreviousLine(1),
                    cursor::MoveToColumn(cursor + PROMPT.len() as u16)
                )?;
                editor.cursor_line.set(0);
            } else {
                queue!(term, cursor::MoveToColumn(cmdcur))?;
                editor.cursor_line.set(1);
            }
        },
        _ => ()
    }

    term.flush()?;

    Ok(())
}

pub fn report(editor: &Editor, shell: &mut Shell, line: &str, err: ParseFailed) -> anyhow::Result<()> {
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
