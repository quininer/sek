mod highlight;

use std::io::Write;
use bumpalo::Bump;
use bumpalo::collections::String;
use unicode_width::UnicodeWidthStr;
use crossterm::{ queue, execute, cursor, style, terminal };
use crate::editor::{ Editor, State };
use crate::shell::Shell;
use crate::shell::parser::{ incomplete_parse_in, ParseFailed };
use crate::util::{ Fill, FmtDebug };


const PROMPT: &str = "~ ";

pub fn render_editor(editor: &Editor, shell: &mut Shell, execute: bool)
    -> anyhow::Result<()>
{
    let bump = shell.bump.clone();
    let bump = bump.borrow();
    let mut buf = String::with_capacity_in(editor.line.len(), &bump);

    let (cursor, _) = editor.line.ready_render(&mut buf, None);

    render_line(editor, shell, &bump, &mut buf, cursor, execute)
}

pub fn render_line(
    editor: &Editor,
    shell: &mut Shell,
    bump: &Bump,
    buf: &mut String<'_>,
    cursor: u16,
    execute: bool
)
    -> anyhow::Result<()>
{
    let mut term = shell.term.lock();

    // TODO slow
    /*
    if execute {
        let (col, _) = cursor::position()?;

        if col != 0 {
            queue!(
                term,
                style::SetAttribute(style::Attribute::Dim),
                style::Print("⏎ "),
                style::SetAttribute(style::Attribute::Reset),
                style::Print("\r\n")
            )?;
        }
    }
    */

    if !execute {
        match editor.cursor_line.get() {
            0 => queue!(term, cursor::MoveToColumn(0))?,
            n => queue!(term, cursor::MoveToPreviousLine(n))?,
        }
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

    let shell_ref = highlight::ShellRef {
        env: &shell.env,
        theme: &shell.theme,
        prompt_len: PROMPT.len(),
        columns: editor.columns as usize
    };

    match incomplete_parse_in(&bump, &buf) {
        Ok(cmd) => highlight::colour(shell_ref, &buf, &mut term, &cmd)?,
        Err(err) => {
            let color = shell_ref.theme.error.color();
            let attr = shell_ref.theme.error.attr().unwrap_or_default();

            queue!(term,
                style::Print(&buf[..err.span.start]),
                style::SetForegroundColor(color),
                style::SetAttributes(attr),
                style::Print(&buf[err.span.clone()]),
                style::SetForegroundColor(style::Color::Reset),
                style::SetAttribute(style::Attribute::Reset),
                style::Print(&buf[err.span.end..])
            )?
        }
    }

    match editor.state {
        State::Edit => {
            queue!(term, cursor::MoveToColumn(cursor + PROMPT.len() as u16))?;
            editor.cursor_line.set(0);
        },
        State::Command => {
            let (cmdcur, fill) = editor.cmd.ready_render(buf, Some(editor.columns));

            queue!(
                term,
                style::Print("\r\n"),
                terminal::DisableLineWrap,
                style::SetColors(style::Colors::new(style::Color::Black, style::Color::White)),
                style::Print(&buf),
                style::Print(fill),
                style::ResetColor,
                terminal::EnableLineWrap
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

pub fn report(_editor: &Editor, shell: &mut Shell, line: &str, err: ParseFailed) -> anyhow::Result<()> {
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
