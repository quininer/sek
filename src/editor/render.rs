use std::io::Write;
use if_chain::if_chain;
use crossterm::{ queue, execute, cursor, style, terminal };
use crate::editor::Editor;
use crate::shell::Shell;


pub fn render(editor: &Editor, shell: &mut Shell, execute: bool)
    -> anyhow::Result<()>
{
    const PROMPT: &str = "~ ";

    let mut buf = editor.global.strbuf.borrow_mut();
    let (linecur, linebuf, _) = editor.line.ready_render(&mut buf, None);

    if execute {
        let (col, _) = cursor::position()?;

        if col != 0 {
            queue!(
                shell.term,
                style::SetAttribute(style::Attribute::Dim),
                style::Print("⏎ "),
                style::SetAttribute(style::Attribute::Reset),
                style::Print("\n")
            )?;
        }
    }

    match editor.cursor_line.get() {
        0 => queue!(shell.term, cursor::MoveToColumn(0))?,
        n => queue!(shell.term, cursor::MoveToPreviousLine(n))?,
    }

    queue!(
        shell.term,
        terminal::Clear(terminal::ClearType::FromCursorDown),
        style::SetForegroundColor(if shell.last_status {
            style::Color::Grey
        } else {
            style::Color::DarkRed
        }),
        style::Print(PROMPT),
    )?;

    let bump = shell.bump.clone();
    let bump = bump.borrow();

//    shell::colour(&bump, &mut shell.term, linebuf)?;

    if_chain!{
        if let Some(suggest_line) = cursor.as_line();
        if let Some(suggest) = strip_prefix(&suggest_line, linebuf.as_bytes());
        then {
            queue!(
                shell.term,
                style::SetForegroundColor(style::Color::Magenta),
                style::Print(suggest.as_bstr())
            )?;
        }
    };

    match editor.state {
        State::Edit => {
            queue!(shell.term, cursor::MoveToColumn(linecur + PROMPT.len() as u16))?;
            editor.cursor_line.set(0);
        },
        State::Command => {
            let size = editor.global.columns.get();
            let (cmdcur, cmdbuf, fill) = editor.cmd.ready_render(&mut buf, Some(size));

            queue!(
                shell.term,
                style::Print("\r\n"),
                style::SetColors(style::Colors::new(style::Color::Black, style::Color::White)),
                style::Print(cmdbuf),
                style::Print(fill),
                style::ResetColor,
            )?;

            if editor.cmd.is_empty() {
                queue!(
                    shell.term,
                    cursor::MoveToPreviousLine(1),
                    cursor::MoveToColumn(linecur + PROMPT.len() as u16)
                )?;
                editor.cursor_line.set(0);
            } else {
                queue!(shell.term, cursor::MoveToColumn(cmdcur))?;
                editor.cursor_line.set(1);
            }
        },
        _ => ()
    }

    shell.term.flush()?;

    Ok(())
}
