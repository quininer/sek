pub mod line;
pub mod ui;
pub mod path_selector;

use std::mem;
use anyhow::Context;
use crossterm::event::{ Event, KeyCode, KeyEvent, KeyModifiers as KM };
use line::EditableLine;
use path_selector::PathSelector;
use crate::ui::layout;
use crate::shell::env::Environment;

pub struct Editor {
    pub ui: ui::Editor,
    pub mode: Mode,
    pub insert: EditableLine,
    pub command: EditableLine,
    pub ready: Option<char>,
    pub path_selector: PathSelector,
}

#[derive(Clone, Copy, Debug)]
pub enum Mode {
    Insert,
    Normal,
    Visual,
    PathSelector,
}

#[derive(Clone, Copy, Debug)]
pub enum Action {
    Continue,
    Completion,
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
        let path_selector = PathSelector::new()?;

        Ok(Editor {
            ui, path_selector,
            mode: Mode::Insert,
            insert: EditableLine::default(),
            command: EditableLine::default(),
            ready: None
        })
    }

    pub fn step(&mut self, env: &Environment, event: Event)
        -> anyhow::Result<Action>
    {
        match (self.mode, event) {
            // Quit
            (Mode::Insert, Event::Key(KeyEvent { modifiers, code, .. }))
                if modifiers == KM::CONTROL && code == KeyCode::Char('d')
                    && self.insert.is_empty()
            => return Ok(Action::Break),

            // Insert to Normal
            (Mode::Insert | Mode::PathSelector, Event::Key(KeyEvent { modifiers, code, .. }))
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
                    KeyCode::Tab => {
                        return Ok(Action::Completion)
                    }
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

            // Normal && Visual && PathSelector
            (Mode::Normal | Mode::Visual | Mode::PathSelector, Event::Key(KeyEvent { modifiers, code, .. }))
                if modifiers.contains(KM::SHIFT & KM::NONE)
            => match (self.mode, self.ready.take(), self.command.first(), code) {
                // command mode
                (Mode::Normal | Mode::PathSelector, None, None, KeyCode::Char(':' | ';'))
                    => self.command.push(':'),
                (Mode::Normal | Mode::PathSelector, None, None, KeyCode::Char('/'))
                    => self.command.push('/'),

                // normal and visual
                (Mode::Normal | Mode::Visual, None, None, KeyCode::Char('i'))
                    => self.mode = Mode::Insert,
                (Mode::Normal | Mode::Visual, None, None, KeyCode::Char('a')) => {
                    self.insert.move_right();
                    self.mode = Mode::Insert;
                },
                (Mode::Normal, None, None, KeyCode::Char('v')) =>
                    self.mode = Mode::Visual,
                (Mode::Visual, None, None, KeyCode::Char('v')) =>
                    self.mode = Mode::Normal,

                // move
                (Mode::Normal | Mode::Visual, None, None, KeyCode::Char('h')) => {
                    self.insert.move_left();
                    if matches!(self.mode, Mode::Normal) {
                        let cursor = self.insert.cursor_mut();
                        cursor.start = cursor.end;
                    }
                },
                (Mode::Normal | Mode::Visual, None, None, KeyCode::Char('l')) => {
                    self.insert.move_right();
                    if matches!(self.mode, Mode::Normal) {
                        let cursor = self.insert.cursor_mut();
                        cursor.start = cursor.end;
                    }
                },
                (Mode::Normal | Mode::Visual, None, None, KeyCode::Char('x')) => {
                    let end = self.insert.char_len();
                    let cursor = self.insert.cursor_mut();
                    cursor.start = 0;
                    cursor.end = end;
                },
                (Mode::Normal | Mode::Visual, None, None, KeyCode::Char('w')) => {
                    let span = self.insert.move_right_word();
                    let cursor = self.insert.cursor_mut();
                    match self.mode {
                        Mode::Normal => *cursor = span,
                        Mode::Visual => cursor.end = span.end,
                        _ => unreachable!()
                    }
                },
                (Mode::Normal | Mode::Visual, None, None, KeyCode::Char('b')) => {
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
                (Mode::Normal | Mode::Visual, None, None, KeyCode::Up | KeyCode::Char('k')) => {
                    self.insert.up();
                },
                (Mode::Normal | Mode::Visual, None, None, KeyCode::Down | KeyCode::Char('j')) => {
                    self.insert.down();
                },

                // execute
                (Mode::Normal | Mode::Visual, None, None, KeyCode::Enter) => {
                    self.insert.submit();
                    self.mode = Mode::Insert;
                    return Ok(Action::Execute)
                }

                // command input
                (Mode::Normal | Mode::PathSelector, _, _, KeyCode::Char('\r')) => (),
                (Mode::Normal | Mode::PathSelector, None, Some(_), KeyCode::Char(c))
                    => self.command.push(c),
                (Mode::Normal | Mode::PathSelector, None, Some(_), KeyCode::Backspace)
                    => self.command.backspace(),
                (Mode::Normal | Mode::PathSelector, None, Some(_), KeyCode::Delete)
                    => self.command.delete(),
                (Mode::Normal | Mode::PathSelector, None, Some(_), KeyCode::Left)
                    => self.command.move_left(),
                (Mode::Normal | Mode::PathSelector, None, Some(_), KeyCode::Right)
                    => self.command.move_right(),
                (Mode::Normal | Mode::PathSelector, None, Some(_), KeyCode::Esc)
                    => self.command.clear(),

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

                (Mode::PathSelector, None, None, KeyCode::Char('j')) =>
                    self.path_selector.down()?,
                (Mode::PathSelector, None, None, KeyCode::Char('k')) =>
                    self.path_selector.up()?,
                (Mode::PathSelector, None, None, KeyCode::Char('h')) =>
                    self.path_selector.left()?,
                (Mode::PathSelector, None, None, KeyCode::Char('l')) =>
                    self.path_selector.right()?,
                (Mode::PathSelector, None, None, KeyCode::Char('.')) =>
                    self.path_selector.toggle_hidden_file(),
                (Mode::PathSelector, None, None, KeyCode::Char(',')) =>
                    self.path_selector.toggle_case_sensitive(),
                (Mode::PathSelector, None, None, KeyCode::Enter) => {
                    let path = self.path_selector.selected();
                    let path = path
                        .strip_prefix(env.pwd())
                        .unwrap_or(&path)
                        .to_str()
                        .context("non-utf8 path are unsupported")?;
                    let path = if !path.is_empty() {
                        path
                    } else {
                        "."
                    };
                    self.insert.push_str(path);
                    self.mode = Mode::Insert;
                },
                    
                _ => ()
            },
            _ => ()
        }

        Ok(Action::Continue)
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
            Some("VIS"),
            // path selector
            None
        ];

        STR[self as usize]
    }
}
