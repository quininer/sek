use std::{ io, mem };
use bumpalo::Bump;
use scopeguard::guard;
use crossterm::{ queue, style, cursor, terminal };
use crossterm::style::{ Color, Attributes };
use crate::shell::Shell;
use crate::editor::Editor;
use crate::editor::path_selector::Entry;


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
        style::Print("\n"),
    )?;

    let space = editor.ui.available_space().saturating_sub(4);
    let mut parent = editor.path_selector.parent
        .take(space)
        .map(with(Level::Parent, editor.ui.columns));
    let mut current = editor.path_selector.current
        .take(space)
        .map(with(Level::Current, editor.ui.columns));
    let mut sub = editor.path_selector.sub
        .take(space)
        .map(with(Level::Sub, editor.ui.columns));

    for i in 0..space {
        if let Some(entry) = parent.next() {
            entry.render(term)?;
        }

        if let Some(entry) = current.next() {
            entry.render(term)?;
        }

        if let Some(entry) = sub.next() {
            entry.render(term)?;
        }

        queue!(term, style::Print("\n"))?;
    }

    // TODO vi buffer
    queue!(term, style::Print("\r\n"))?;

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
    -> impl Fn(&Entry) -> Item<'_>
{
    move |entry| Item {
        entry,
        level, width,
        selected: false
    }
}

impl Item<'_> {
    fn render<W: io::Write>(&self, term: &mut W) -> anyhow::Result<()> {
        let (start, len) = match self.level {
            Level::Parent => (0, 50),
            Level::Current => (50, 100),
            Level::Sub => (100, 30)
        };
        let file_name = self.entry.name();
        let file_name = file_name.to_string_lossy();

        queue!(term, cursor::MoveToColumn(start))?;

        if self.selected {
            // TODO color
        }

        queue!(term, style::Print(file_name))?;

        if self.selected {
            queue!(term, style::SetBackgroundColor(style::Color::Reset))?;
        }

        Ok(())
    }
}
