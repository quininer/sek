pub mod line;
pub mod ui;

use std::ops::Range;
use crossterm::event::{ Event, KeyCode, KeyEvent, KeyModifiers as KM };
use line::EditableLine;
use crate::ui::render::{ Renderer, TermTarget };
use crate::ui::layout;

pub struct Editor {
    pub ui: ui::Editor,
    pub mode: Mode,
    pub insert: EditableLine,
    pub insert_cursor: Range<usize>,
    pub command: EditableLine,
    pub command_cursor: usize,
    pub ready: Option<char>,
}

#[derive(Clone, Copy, Debug)]
pub enum Mode {
    Insert,
    Normal,
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
            insert: EditableLine::default(),
            insert_cursor: 0..0,
            command: EditableLine::default(),
            command_cursor: 0,
            ready: None
        })
    }

    pub fn step(&mut self, event: Event)
        -> anyhow::Result<Action>
    {
        match (self.mode, event) {
            // Quit
            (Mode::Insert, Event::Key(KeyEvent { modifiers, code, .. }))
                if modifiers == KM::CONTROL && code == KeyCode::Char('d')
                    && self.insert.is_empty()
            => return Ok(Action::Break),

            // Insert to Normal
            (Mode::Insert, Event::Key(KeyEvent { modifiers, code, .. }))
                if (modifiers == KM::CONTROL && code == KeyCode::Char('c'))
                    || (modifiers == KM::NONE && code == KeyCode::Esc)
                    || (modifiers == KM::ALT && code == KeyCode::Char(' '))
            => {
                self.mode = Mode::Normal;
            },

            // Insert
            (Mode::Insert, Event::Key(KeyEvent { modifiers, code, .. }))
                if modifiers.contains(KM::SHIFT & KM::NONE)
            => {
                match code {
                    KeyCode::Char('\r') => (),
                    KeyCode::Char(c) => self.insert.push(&mut self.insert_cursor.end, c),
                    KeyCode::Backspace => self.insert.backspace(&mut self.insert_cursor.end),
                    KeyCode::Delete => self.insert.delete(self.insert_cursor.end),
                    KeyCode::Left => self.insert.move_left(&mut self.insert_cursor.end),
                    KeyCode::Right => self.insert.move_right(&mut self.insert_cursor.end),
                    KeyCode::Home => self.insert.move_head(&mut self.insert_cursor.end),
                    KeyCode::End => self.insert.move_end(&mut self.insert_cursor.end),
                    KeyCode::Enter => return Ok(Action::Execute),
                    _ => ()
                }

                self.insert_cursor.start = self.insert_cursor.end;
            },

            // Noraml
            (Mode::Normal, Event::Key(KeyEvent { modifiers, code, .. }))
                if modifiers.contains(KM::SHIFT & KM::NONE)
            => match (self.ready.take(), self.command.first(), code) {
                // mode switch
                (None, None, KeyCode::Char(':' | ';')) => self.command.push(&mut self.command_cursor, ':'),
                (None, None, KeyCode::Char('/')) => self.command.push(&mut self.command_cursor, '/'),
                (None, None, KeyCode::Char('i')) => self.mode = Mode::Insert,
                (None, None, KeyCode::Char('a')) => {
                    self.insert.move_right(&mut self.insert_cursor.end);
                    self.mode = Mode::Insert;
                },

                // move
                (None, None, KeyCode::Char('h')) => self.insert.move_left(&mut self.insert_cursor.end),
                (None, None, KeyCode::Char('l')) => self.insert.move_right(&mut self.insert_cursor.end),

                // input
                (_, _, KeyCode::Char('\r')) => (),
                (None, Some(_), KeyCode::Char(c)) => self.command.push(&mut self.command_cursor, c),
                (None, Some(_), KeyCode::Backspace) => self.command.backspace(&mut self.command_cursor),
                (None, Some(_), KeyCode::Delete) => self.command.delete(self.command_cursor),
                (None, Some(_), KeyCode::Left) => self.command.move_left(&mut self.command_cursor),
                (None, Some(_), KeyCode::Right) => self.command.move_right(&mut self.command_cursor),
                (None, Some(_), KeyCode::Esc) => {
                    self.command.clear();
                    self.command_cursor = 0;
                },                

                // ready
                (None, None, KeyCode::Char('d')) => self.ready = Some('d'),
                (Some('d'), None, KeyCode::Char('d')) => {
                    self.insert.clear();
                    self.insert_cursor = 0..0;
                }
                _ => ()
            },
            _ => ()
        }

        Ok(Action::Continue)
    }

    pub fn render<T: TermTarget>(
        &self,
        renderer: &mut Renderer<Self, T, anyhow::Error>,
    ) -> anyhow::Result<()> {
        renderer.render(self)
    }
}

impl Mode {
    fn str(self) -> Option<&'static str> {
        const STR: &[Option<&str>] = &[
            // insert
            None,
            // normal
            None,
            // visual
            Some("VIS")
        ];

        STR[self as usize]
    }
}
