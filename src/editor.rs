pub mod line;
pub mod ui;

use std::mem;
use crossterm::event::{ Event, KeyCode, KeyEvent, KeyModifiers as KM };
use line::EditableLine;
use crate::ui::render::{ Renderer, TermTarget };
use crate::ui::layout;

pub struct Editor {
    pub ui: ui::Editor,
    pub mode: Mode,
    pub insert: EditableLine,
    pub command: EditableLine,
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
            command: EditableLine::default(),
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
                    KeyCode::Char(c) => self.insert.push(c),
                    KeyCode::Backspace => self.insert.backspace(),
                    KeyCode::Delete => self.insert.delete(),
                    KeyCode::Left => self.insert.move_left(),
                    KeyCode::Right => self.insert.move_right(),
                    KeyCode::Home => self.insert.move_head(),
                    KeyCode::End => self.insert.move_end(),
                    KeyCode::Up => self.insert.up(),
                    KeyCode::Down => self.insert.down(),
                    KeyCode::Enter => {
                        self.insert.submit();
                        return Ok(Action::Execute)
                    },
                    _ => ()
                }

                let cursor = self.insert.cursor_mut();
                cursor.start = cursor.end;
            },

            // Noraml && Visual
            (Mode::Normal | Mode::Visual, Event::Key(KeyEvent { modifiers, code, .. }))
                if modifiers.contains(KM::ALT) && code == KeyCode::Char(';')
            => {
                let cursor = self.insert.cursor_mut();
                mem::swap(&mut cursor.start, &mut cursor.end);
            }

            // Noraml && Visual
            (Mode::Normal | Mode::Visual, Event::Key(KeyEvent { modifiers, code, .. }))
                if modifiers.contains(KM::SHIFT & KM::NONE)
            => match (self.mode, self.ready.take(), self.command.first(), code) {
                // mode switch
                (Mode::Normal, None, None, KeyCode::Char(':' | ';'))
                    => self.command.push(':'),
                (Mode::Normal, None, None, KeyCode::Char('/'))
                    => self.command.push('/'),
                (_, None, None, KeyCode::Char('i'))
                    => self.mode = Mode::Insert,
                (Mode::Normal, None, None, KeyCode::Char('v')) =>
                    self.mode = Mode::Visual,
                (Mode::Visual, None, None, KeyCode::Char('v')) =>
                    self.mode = Mode::Normal,
                (_, None, None, KeyCode::Char('a')) => {
                    self.insert.move_right();
                    self.mode = Mode::Insert;
                },

                // move
                (_, None, None, KeyCode::Char('h')) => {
                    self.insert.move_left();
                    if matches!(self.mode, Mode::Normal) {
                        let cursor = self.insert.cursor_mut();
                        cursor.start = cursor.end;
                    }
                },
                (_, None, None, KeyCode::Char('l')) => {
                    self.insert.move_right();
                    if matches!(self.mode, Mode::Normal) {
                        let cursor = self.insert.cursor_mut();
                        cursor.start = cursor.end;
                    }
                },
                (_, None, None, KeyCode::Char('x')) => {
                    let end = self.insert.char_len();
                    let cursor = self.insert.cursor_mut();
                    cursor.start = 0;
                    cursor.end = end;
                },
                (_, None, None, KeyCode::Char('w')) => {
                    let span = self.insert.move_right_word();
                    let cursor = self.insert.cursor_mut();
                    match self.mode {
                        Mode::Normal => *cursor = span,
                        Mode::Visual => cursor.end = span.end,
                        _ => unreachable!()
                    }
                },
                (_, None, None, KeyCode::Char('b')) => {
                    let span = self.insert.move_left_word();
                    let cursor = self.insert.cursor_mut();
                    match self.mode {
                        Mode::Normal => {
                            cursor.start = span.end;
                            cursor.end = span.start;
                        },
                        Mode::Visual => cursor.end = span.start,
                        _ => unreachable!()
                    }
                },

                // history
                (_, None, None, KeyCode::Up | KeyCode::Char('k')) => {
                    self.insert.up();
                },
                (_, None, None, KeyCode::Down | KeyCode::Char('j')) => {
                    self.insert.down();
                },

                // execute
                (_, None, None, KeyCode::Enter) => {
                    self.insert.submit();
                    self.mode = Mode::Insert;
                    return Ok(Action::Execute)
                }

                // input
                (Mode::Normal, _, _, KeyCode::Char('\r')) => (),
                (Mode::Normal, None, Some(_), KeyCode::Char(c))
                    => self.command.push(c),
                (Mode::Normal, None, Some(_), KeyCode::Backspace)
                    => self.command.backspace(),
                (Mode::Normal, None, Some(_), KeyCode::Delete)
                    => self.command.delete(),
                (Mode::Normal, None, Some(_), KeyCode::Left)
                    => self.command.move_left(),
                (Mode::Normal, None, Some(_), KeyCode::Right)
                    => self.command.move_right(),
                (Mode::Normal, None, Some(_), KeyCode::Esc) => {
                    self.command.clear();
                },

                // delete selection
                (Mode::Visual, None, None, KeyCode::Char('d')) => {
                    self.insert.replace_str_inclusive("");
                    self.mode = Mode::Normal;
                },

                // ready
                (Mode::Normal, None, None, KeyCode::Char('d')) => self.ready = Some('d'),
                (Mode::Normal, Some('d'), None, KeyCode::Char('d')) => {
                    self.insert.clear();
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
