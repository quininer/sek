pub mod line;
pub mod ui;

use std::ops::Range;
use crossterm::event::{ Event, KeyCode, KeyEvent, KeyModifiers as KM };
use line::EditableLine;
use crate::ui::render::{ Renderer, Target };
use crate::ui::layout;

pub struct Editor {
    ui: ui::Editor,
    pub mode: Mode,
    pub line: EditableLine,
    pub line_cursor: Range<usize>,
    pub command: EditableLine,
    pub command_cursor: Range<usize>,
}

#[derive(Clone, Copy, Debug)]
pub enum Mode {
    Insert,
    Normal,
    Visual,
}

#[derive(Clone, Copy, Debug)]
pub enum Action {
    Continue,
    Execute,
    Break,
}

impl AsRef<layout::Tree> for Editor {
    fn as_ref(&self) -> &layout::Tree {
        &self.ui.layout
    }
}

impl Editor {
    pub fn new() -> anyhow::Result<Self> {
        let ui = ui::Editor::new()?;

        Ok(Editor {
            ui,
            mode: Mode::Insert,
            line: EditableLine::default(),
            line_cursor: 0..0,
            command: EditableLine::default(),
            command_cursor: 0..0
        })
    }

    pub fn init_to<W>(&self, renderer: &mut Renderer<Self, W, anyhow::Error>) {
        renderer.insert::<ui::Prompt>(self.ui.prompt);
        renderer.insert::<ui::InsertLine>(self.ui.insert_line);
        renderer.insert::<ui::CommandLine>(self.ui.command_line);
        renderer.insert::<ui::Tips>(self.ui.tips);
    }

    pub fn step(&mut self, event: Event)
        -> anyhow::Result<Action>
    {
        match (self.mode, event) {
            // Quit
            (Mode::Insert, Event::Key(KeyEvent { modifiers, code, .. }))
                if modifiers == KM::CONTROL && code == KeyCode::Char('d')
                    && self.line.is_empty()
            => return Ok(Action::Break),
            // Insert
            (Mode::Insert | Mode::Normal, Event::Key(KeyEvent { modifiers, code, .. }))
                if modifiers.contains(KM::SHIFT & KM::NONE)
            => {
                let end = match self.mode {
                    Mode::Insert => &mut self.line_cursor.end,
                    Mode::Normal => &mut self.command_cursor.end,
                    _ => unreachable!()
                };

                match code {
                    KeyCode::Char('\r') => (),
                    KeyCode::Char(c) => self.line.push(end, c),
                    KeyCode::Backspace => self.line.backspace(end),
                    KeyCode::Delete => self.line.delete(*end),
                    KeyCode::Left => self.line.move_left(end),
                    KeyCode::Right => self.line.move_right(end),
                    KeyCode::Home => self.line.move_head(end),
                    KeyCode::End => self.line.move_end(end),
                    KeyCode::Enter => return Ok(Action::Execute),
                    _ => ()
                }
            },
            _ => ()
        }

        if matches!(self.mode, Mode::Insert) {
            self.line_cursor.start = self.line_cursor.end;
        }

        Ok(Action::Continue)
    }

    pub fn render<T: Target>(
        &self,
        renderer: &mut Renderer<Self, T, anyhow::Error>,
    ) -> anyhow::Result<()> {
        renderer.render(self)
    }
}
