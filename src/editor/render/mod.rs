mod highlight;

use std::io::Write;
use bumpalo::Bump;
use bumpalo::collections::String;
use unicode_width::UnicodeWidthStr;
use crossterm::{ queue, execute, cursor, style, terminal };
use crate::editor::{ Editor, Mode };
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
    };

    match incomplete_parse_in(&bump, &buf) {
        Ok(cmd) => highlight::colour(shell_ref, &buf, &mut term, &cmd)?,
        Err(err) => {
            let color = shell_ref.theme.error.color();
            let attr = shell_ref.theme.error.attr().unwrap_or_default();

            queue!(term, style::Print(&buf[..err.span.start]))?;
            if let Some(color) = color {
                queue!(term, style::SetForegroundColor(color))?;
            }
            queue!(term,
                style::SetAttributes(attr),
                style::Print(&buf[err.span.clone()]),
                style::SetForegroundColor(style::Color::Reset),
                style::SetAttribute(style::Attribute::Reset),
                style::Print(&buf[err.span.end..])
            )?;
        }
    }

    editor.cursor_line.set({
        let total_width = buf.width() + PROMPT.len();
        let total_width = total_width as u16;
        (total_width / editor.columns)
            + (total_width % editor.columns != 0) as u16
            - 1
    });

    let cursor_width = cursor + PROMPT.len() as u16;
    let cursor_line = (cursor_width / editor.columns)
        + (cursor_width % editor.columns != 0) as u16
        - 1;
    let cursor_column = if cursor_width < editor.columns {
        cursor_width
    } else {
        let cursor_column = cursor_width % editor.columns;
        if cursor_column != 0 {
            cursor_column
        } else {
            editor.columns
        }
    };

    match editor.mode {
        Mode::Insert => {
            let last_line = editor.cursor_line.get();
            if let Some(prev_line) = last_line.checked_sub(cursor_line)
                .filter(|&prev_line| prev_line > 0)
            {
                editor.cursor_line.set(cursor_line);
                queue!(term, cursor::MoveToPreviousLine(prev_line))?;
            } else if last_line < cursor_line {
                editor.cursor_line.set(cursor_line);
                queue!(term, style::Print("\n"))?;
            }

            queue!(term, cursor::MoveToColumn(cursor_column))?;
        },
        Mode::Normal => {
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
                let last_line = editor.cursor_line.get();
                if let Some(prev_line) = last_line.checked_sub(cursor_line)
                    .filter(|&line| line > 0)
                {
                    editor.cursor_line.set(cursor_line);
                    queue!(term, cursor::MoveToPreviousLine(prev_line + 1))?;
                } else if last_line < cursor_line {
                    editor.cursor_line.set(cursor_line);
                    queue!(term, style::Print("\n"))?;
                } else {
                    queue!(term, cursor::MoveToPreviousLine(1))?;
                }

                queue!(term, cursor::MoveToColumn(cursor_column))?;
            } else {
                queue!(term, cursor::MoveToColumn(cmdcur))?;
                editor.cursor_line.set(editor.cursor_line.get() + 1);
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
