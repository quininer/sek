use std::io::Write;
use bumpalo::Bump;
use bumpalo::collections::String;
use unicode_width::UnicodeWidthStr;
use crossterm::{ queue, cursor, style, terminal };
use crate::editor::{ Editor, Mode };
use crate::shell::Shell;
use crate::shell::parser::incomplete_parse_in;
use crate::editor::render::{ highlight, PROMPT };
use crate::util::Fill;


pub fn render(
    bump: &Bump,
    editor: &mut Editor,
    shell: &mut Shell,
    buf: &mut String<'_>,
    cursor: u16
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

    if !shell.is_execute {
        match editor.ui.cursor_row {
            0 => queue!(term, cursor::MoveToColumn(0))?,
            n => queue!(term, cursor::MoveToPreviousLine(n))?,
        }
    }

    // TODO custom prompt
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
            queue!(term, style::Print(&buf[..err.span.start]))?;
            if let Some(color) = shell_ref.theme.error.color() {
                queue!(term, style::SetForegroundColor(color))?;
            }
            if let Some(attr) = shell_ref.theme.error.attr() {
                queue!(term, style::SetAttributes(attr))?;
            }
            queue!(term,
                style::Print(&buf[err.span.clone()]),
                style::SetForegroundColor(style::Color::Reset),
                style::SetAttribute(style::Attribute::Reset),
                style::Print(&buf[err.span.end..])
            )?;
        }
    }

    let cursor_width = cursor + PROMPT.len() as u16;
    editor.ui.bottom = {
        let total_width = buf.width() + PROMPT.len();
        let total_width = total_width as u16;
        (total_width / editor.ui.columns)
            + (total_width % editor.ui.columns != 0) as u16
            - 1
    };
    editor.ui.cursor_row = (cursor_width / editor.ui.columns)
        + (cursor_width % editor.ui.columns != 0) as u16
        - 1;
    editor.ui.cursor_column = if cursor_width < editor.ui.columns {
        cursor_width
    } else {
        let cursor_column = cursor_width % editor.ui.columns;
        if cursor_column != 0 {
            cursor_column
        } else {
            editor.ui.columns
        }
    };

    match editor.mode {
        Mode::Insert => {
            if let Some(prev_line) = editor.ui.bottom
                .checked_sub(editor.ui.cursor_row)
                .filter(|&prev_line| prev_line > 0)
            {
                queue!(term, cursor::MoveToPreviousLine(prev_line))?;
            } else if editor.ui.bottom < editor.ui.cursor_row {
                queue!(term, style::Print("\n"))?;
            }

            queue!(term, cursor::MoveToColumn(editor.ui.cursor_column))?;
        },
        Mode::Normal => {
            let (cmdcur, cmdwidth) = editor.cmd.read_into_and_width(buf);
            let fill = Fill::empty(editor.ui.columns.checked_sub(cmdwidth).unwrap_or(0));

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
                if let Some(prev_line) = editor.ui.bottom
                    .checked_sub(editor.ui.cursor_row)
                    .filter(|&line| line > 0)
                {
                    queue!(term, cursor::MoveToPreviousLine(prev_line + 1))?;
                } else if editor.ui.bottom < editor.ui.cursor_row {
                    queue!(term, style::Print("\n"))?;
                } else {
                    queue!(term, cursor::MoveToPreviousLine(1))?;
                }

                queue!(term, cursor::MoveToColumn(editor.ui.cursor_column))?;
            } else {
                editor.ui.cursor_row = editor.ui.bottom + 1;
                queue!(term, cursor::MoveToColumn(cmdcur))?;
            }
        },
        _ => ()
    }

    term.flush()?;

    Ok(())
}
