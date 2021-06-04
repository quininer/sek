use std::mem;
use std::io::{ self, Write };
use bumpalo::Bump;
use bumpalo::collections::String;
use bstr::{ ByteVec, ByteSlice };
use scopeguard::guard;
use crossterm::{ queue, style, cursor, terminal };
use crate::shell::Shell;
use crate::editor::Editor;
use crate::editor::path_selector::{ Entry, EntryType };
use crate::util::Fill;


pub const RESERVE_SPACE: usize = 3;

pub fn render(
    bump: &Bump,
    editor: &mut Editor,
    shell: &mut Shell,
) -> anyhow::Result<()> {
    let term = shell.term.lock();
    let mut term = guard(term, |mut term| {
        let _ = queue!(term,
            style::ResetColor,
            style::SetAttribute(style::Attribute::Reset),
            terminal::EnableLineWrap
        );
    });
    let term = &mut *term;

    if mem::take(&mut editor.path_selector.need_init) {
        queue!(term, style::Print("\r\n"))?;
    } else {
        queue!(term, cursor::MoveTo(0, editor.ui.bottom + 1))?;
    }

    queue!(term,
        terminal::Clear(terminal::ClearType::FromCursorDown),
        terminal::DisableLineWrap,
        style::Print(editor.path_selector.path().display()),
        style::Print('/'),
    )?;

    if let Some(entry) = editor.path_selector.current.get() {
        let name = entry.name();
        let name = Vec::from_os_str_lossy(&name);
        queue!(term, style::Print(name.as_bstr()))?;
    }

    queue!(term, style::Print('\n'))?;

    let space = editor.ui.available_space().saturating_sub(RESERVE_SPACE);
    let mut parent = editor.path_selector.parent
        .take(space)
        .map(with(Level::Parent, editor.ui.columns));
    let mut current = editor.path_selector.current
        .take(space)
        .map(with(Level::Current, editor.ui.columns));
    let mut sub = editor.path_selector.sub
        .take(space)
        .map(with(Level::Sub, editor.ui.columns));

    for _ in 0..space {
        if let Some(entry) = parent.next() {
            entry.render(term)?;
        }

        if let Some(entry) = current.next() {
            entry.render(term)?;
        }

        if let Some(entry) = sub.next() {
            entry.render(term)?;
        }

        queue!(term, style::Print('\n'))?;
    }

    if let Some(err) = editor.error.take() {
        queue!(term,
            cursor::MoveToColumn(0),
            style::SetColors(style::Colors::new(style::Color::Black, style::Color::Red)),
            style::Print(err),
            style::ResetColor,
        )?;
    } else if !editor.cmd.is_empty() {
        let mut buf = String::with_capacity_in(editor.cmd.len(), bump);
        let (cmdcur, cmdwidth) = editor.cmd.read_into_and_width(&mut buf);
        let fill = Fill::empty(editor.ui.columns.saturating_sub(cmdwidth));

        queue!(term,
            cursor::MoveToColumn(0),
            style::SetColors(style::Colors::new(style::Color::Black, style::Color::White)),
            style::Print(&buf),
            style::Print(fill),
            style::ResetColor,
            cursor::MoveToColumn(cmdcur)
        )?;
    } else {
        queue!(term, cursor::MoveTo(
            editor.ui.cursor_column.saturating_sub(1),
            editor.ui.cursor_row
        ))?;
    }

    term.flush()?;

    Ok(())
}

#[derive(Clone, Copy)]
enum Level {
    Parent,
    Current,
    Sub
}

struct Item<'a> {
    entry: &'a Entry,
    level: Level,
    width: u16,
    selected: bool
}

fn with(level: Level, width: u16)
    -> impl Fn((bool, &Entry)) -> Item<'_>
{
    move |(selected, entry)| Item {
        entry, selected,
        level, width,
    }
}

impl Item<'_> {
    fn render<W: io::Write>(&self, term: &mut W) -> anyhow::Result<()> {
        fn cal(width: u16, scale: f32) -> u16 {
            (width as f32 * scale) as u16
        }

        let (start, len) = match self.level {
            Level::Parent => (0, cal(self.width, 0.2)),
            Level::Current => (cal(self.width, 0.2) + 1, cal(self.width, 0.4)),
            Level::Sub => (cal(self.width, 0.6) + 1, self.width - cal(self.width, 0.6))
        };
        let ty = self.entry.type_();
        let file_name = self.entry.name();
        let file_name = Vec::from_os_str_lossy(&file_name);
        let (file_name, fill) = name_width_limit(&file_name, len as usize);

        queue!(term, cursor::MoveToColumn(start))?;

        if self.selected {
            queue!(term,
                style::SetAttribute(style::Attribute::Bold),
                style::SetBackgroundColor(style::Color::Blue),
                style::SetForegroundColor(style::Color::Black)
            )?;
        } else if let EntryType::Dir = ty {
            queue!(term,
                style::SetAttribute(style::Attribute::Bold),
                style::SetForegroundColor(style::Color::DarkBlue)
            )?;
        }

        queue!(term,
            style::Print(' '),
            style::Print(file_name.as_bstr()),
            style::Print(' '),
        )?;

        if self.selected {
            queue!(term, style::Print(fill))?;
        }

        if self.selected || ty == EntryType::Dir {
            queue!(term,
                style::SetAttribute(style::Attribute::Reset),
                style::ResetColor
            )?;
        }

        Ok(())
    }
}

fn name_width_limit(name: &[u8], width: usize) -> (&[u8], Fill) {
    use unicode_width::UnicodeWidthChar;

    let mut width = width.saturating_sub(3);
    let mut end = 0;

    for (_, char_end, c) in name.char_indices() {
        if let Some(result) = width.checked_sub(c.width().unwrap_or(0)) {
            width = result;
            end = char_end;
        } else {
            break
        }
    }

    (&name[..end], Fill::empty(width as u16))
}
