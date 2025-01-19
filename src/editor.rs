pub mod line;
pub mod ui;

use std::mem;
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
    Visual
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

            // Noraml && Visual
            (Mode::Normal | Mode::Visual, Event::Key(KeyEvent { modifiers, code, .. }))
                if modifiers.contains(KM::ALT) && code == KeyCode::Char(';')
            => {
                mem::swap(&mut self.insert_cursor.start, &mut self.insert_cursor.end);
            }

            // Noraml && Visual
            (Mode::Normal | Mode::Visual, Event::Key(KeyEvent { modifiers, code, .. }))
                if modifiers.contains(KM::SHIFT & KM::NONE)
            => match (self.mode, self.ready.take(), self.command.first(), code) {
                // mode switch
                (Mode::Normal, None, None, KeyCode::Char(':' | ';'))
                    => self.command.push(&mut self.command_cursor, ':'),
                (Mode::Normal, None, None, KeyCode::Char('/'))
                    => self.command.push(&mut self.command_cursor, '/'),
                (_, None, None, KeyCode::Char('i'))
                    => self.mode = Mode::Insert,
                (Mode::Normal, None, None, KeyCode::Char('v')) =>
                    self.mode = Mode::Visual,
                (Mode::Visual, None, None, KeyCode::Char('v')) =>
                    self.mode = Mode::Normal,
                (_, None, None, KeyCode::Char('a')) => {
                    self.insert.move_right(&mut self.insert_cursor.end);
                    self.mode = Mode::Insert;
                },

                // move
                (_, None, None, KeyCode::Char('h')) => {
                    self.insert.move_left(&mut self.insert_cursor.end);
                    if matches!(self.mode, Mode::Normal) {
                        self.insert_cursor.start = self.insert_cursor.end;
                    }
                },
                (_, None, None, KeyCode::Char('l')) => {
                    self.insert.move_right(&mut self.insert_cursor.end);
                    if matches!(self.mode, Mode::Normal) {
                        self.insert_cursor.start = self.insert_cursor.end;
                    }
                },
                (_, None, None, KeyCode::Char('x')) => {
                    self.insert_cursor.start = 0;
                    self.insert_cursor.end = self.insert.char_len();
                },
                (_, None, None, KeyCode::Char('w')) => {
                    let span = self.insert.move_right_word(self.insert_cursor.end);
                    match self.mode {
                        Mode::Normal => self.insert_cursor = span,
                        Mode::Visual => self.insert_cursor.end = span.end,
                        _ => unreachable!()
                    }
                },
                (_, None, None, KeyCode::Char('b')) => {
                    let span = self.insert.move_left_word(self.insert_cursor.end);
                    match self.mode {
                        Mode::Normal => {
                            self.insert_cursor.start = span.end;
                            self.insert_cursor.end= span.start;
                        },
                        Mode::Visual => self.insert_cursor.end = span.start,
                        _ => unreachable!()
                    }
                },

                // input
                (Mode::Normal, _, _, KeyCode::Char('\r')) => (),
                (Mode::Normal, None, Some(_), KeyCode::Char(c))
                    => self.command.push(&mut self.command_cursor, c),
                (Mode::Normal, None, Some(_), KeyCode::Backspace)
                    => self.command.backspace(&mut self.command_cursor),
                (Mode::Normal, None, Some(_), KeyCode::Delete)
                    => self.command.delete(self.command_cursor),
                (Mode::Normal, None, Some(_), KeyCode::Left)
                    => self.command.move_left(&mut self.command_cursor),
                (Mode::Normal, None, Some(_), KeyCode::Right)
                    => self.command.move_right(&mut self.command_cursor),
                (Mode::Normal, None, Some(_), KeyCode::Esc) => {
                    self.command.clear();
                    self.command_cursor = 0;
                },

                // delete selection
                (Mode::Visual, None, None, KeyCode::Char('d')) => {
                    self.insert.replace_str(&mut self.insert_cursor, "");
                    self.mode = Mode::Normal;
                },

                // ready
                (Mode::Normal, None, None, KeyCode::Char('d')) => self.ready = Some('d'),
                (Mode::Normal, Some('d'), None, KeyCode::Char('d')) => {
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
