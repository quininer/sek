pub mod line;
pub mod ui;

use std::ops::Range;
use crossterm::event::{ Event, KeyCode, KeyEvent, KeyModifiers as KM };
use line::EditableLine;

pub struct Editor {
    line: EditableLine,
    line_cursor: Range<usize>,
    cmd: EditableLine,
    cmd_cursor: Range<usize>,
    ui: ui::Editor,
    mode: Mode
}

#[derive(Clone, Copy, Debug)]
enum Mode {
    Insert,
    Normal,
    Visual,
}

impl Editor {
    pub fn new() -> anyhow::Result<Self> {
        let line = EditableLine::default();
        let cmd = EditableLine::default();
        let editor = ui::Editor::new()?;

        Ok(Editor {
            line, line_cursor: 0..0,
            cmd, cmd_cursor: 0..0,
            ui: editor,
            mode: Mode::Insert
        })
    }

    pub fn step(&mut self, event: Event)
        -> anyhow::Result<()>
    {
        match (self.mode, event) {
            // Insert
            (Mode::Insert, Event::Key(KeyEvent { modifiers, code, .. }))
                if modifiers.contains(KM::SHIFT & KM::NONE)
            => match code {
                KeyCode::Char('\r') => (),
                KeyCode::Char(c) => self.line.push(&mut self.line_cursor.end, c),
                KeyCode::Backspace => self.line.backspace(&mut self.line_cursor.end),
                KeyCode::Delete => self.line.delete(self.line_cursor.end),
                KeyCode::Left => self.line.move_left(&mut self.line_cursor.end),
                KeyCode::Right => self.line.move_right(&mut self.line_cursor.end),
                KeyCode::Home => self.line.move_head(&mut self.line_cursor.end),
                KeyCode::End => self.line.move_end(&mut self.line_cursor.end),
                _ => ()
            },
            _ => ()
        }

        if matches!(self.mode, Mode::Insert) {
            self.line_cursor.start = self.line_cursor.end;
        }

        Ok(())
    }
}
