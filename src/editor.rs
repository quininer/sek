pub mod line;
pub mod ui;
pub mod path_selector;

use std::mem;
use std::path::Path;
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
    clipboard: String,
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
            ready: None,
            clipboard: String::new(),
        })
    }

    pub fn step(&mut self, env: &Environment, event: Event)
        -> anyhow::Result<Action>
    {
        match (self.mode, event) {
            // Quit
            (Mode::Insert, Event::Key(KeyEvent { modifiers, code, .. }))
                if modifiers == KM::CONTROL
                    && code == KeyCode::Char('d')
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

            // Noraml && Visual: swap cursor
            (Mode::Normal | Mode::Visual, Event::Key(KeyEvent { modifiers, code, .. }))
                if modifiers.contains(KM::ALT) && code == KeyCode::Char(';')
            => {
                let cursor = self.insert.cursor_mut();
                mem::swap(&mut cursor.start, &mut cursor.end);
            }

            // Normal && PathSelector with command
            (Mode::Normal | Mode::PathSelector, Event::Key(KeyEvent { modifiers, code, .. }))
                if modifiers.contains(KM::SHIFT & KM::NONE) && !self.command.is_empty()
            => {
                match (self.mode, self.command.first(), code) {
                    // command input
                    (Mode::Normal | Mode::PathSelector, _, KeyCode::Char('\r')) => (),
                    (Mode::Normal | Mode::PathSelector, Some(_), KeyCode::Char(c))
                        => self.command.push(c),
                    (Mode::Normal | Mode::PathSelector, Some(_), KeyCode::Backspace)
                        => self.command.backspace(),
                    (Mode::Normal | Mode::PathSelector, Some(_), KeyCode::Delete)
                        => self.command.delete(),
                    (Mode::Normal | Mode::PathSelector, Some(_), KeyCode::Left)
                        => self.command.move_left(),
                    (Mode::Normal | Mode::PathSelector, Some(_), KeyCode::Right)
                        => self.command.move_right(),
                    (Mode::Normal | Mode::PathSelector, Some(_), KeyCode::Esc)
                        => self.command.clear(),
                    (Mode::PathSelector, Some('/'), KeyCode::Enter)
                        => {
                            if let Some(cmd) = self.command.as_str().strip_prefix('/')
                                .filter(|cmd| !cmd.is_empty())
                            {
                                self.path_selector.set_glob(Some(glob::Pattern::new(cmd)?));
                            } else {
                                self.path_selector.set_glob(None);
                            }

                            self.command.clear();
                            self.path_selector.cd(Path::new("."))?;                            
                        }
                    _ => (),      
                }
            },

            // Normal && Visual && PathSelector
            (Mode::Normal | Mode::Visual | Mode::PathSelector, Event::Key(KeyEvent { modifiers, code, .. }))
                if modifiers.contains(KM::SHIFT & KM::NONE) && self.command.is_empty()
            => match (self.mode, self.ready.take(), code) {
                // command mode
                (Mode::Normal | Mode::PathSelector, None, KeyCode::Char(':' | ';'))
                    => self.command.push(':'),
                (Mode::Normal | Mode::PathSelector, None, KeyCode::Char('/'))
                    => self.command.push('/'),

                // normal and visual
                (Mode::Normal | Mode::Visual, None, KeyCode::Char('i'))
                    => self.mode = Mode::Insert,
                (Mode::Normal | Mode::Visual, None, KeyCode::Char('a')) => {
                    self.insert.move_right();
                    self.mode = Mode::Insert;
                },
                (Mode::Normal, None, KeyCode::Char('v')) =>
                    self.mode = Mode::Visual,
                (Mode::Visual, None, KeyCode::Char('v')) =>
                    self.mode = Mode::Normal,

                // move
                (Mode::Normal | Mode::Visual, None, KeyCode::Char('h')) => {
                    self.insert.move_left();

                    if matches!(self.mode, Mode::Normal) {
                        self.insert.cursor_mut().start = self.insert.cursor_mut().end;
                    }
                },
                (Mode::Normal | Mode::Visual, None, KeyCode::Char('l')) => {
                    self.insert.move_right();

                    if matches!(self.mode, Mode::Normal) {
                        self.insert.cursor_mut().start = self.insert.cursor_mut().end;
                    }
                },
                (Mode::Normal | Mode::Visual, None, KeyCode::Char('x')) => {
                    let end = self.insert.char_len();
                    let cursor = self.insert.cursor_mut();
                    cursor.start = 0;
                    cursor.end = end;
                },
                (Mode::Normal | Mode::Visual, None, KeyCode::Char('w')) => {
                    let span = self.insert.move_right_word();
                    let cursor = self.insert.cursor_mut();
                    match self.mode {
                        Mode::Normal => *cursor = span,
                        Mode::Visual => cursor.end = span.end,
                        _ => unreachable!()
                    }
                },
                (Mode::Normal | Mode::Visual, None, KeyCode::Char('b')) => {
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
                (Mode::Normal | Mode::Visual, None, KeyCode::Up | KeyCode::Char('k')) => {
                    self.insert.up();
                },
                (Mode::Normal | Mode::Visual, None, KeyCode::Down | KeyCode::Char('j')) => {
                    self.insert.down();
                },

                // execute
                (Mode::Normal | Mode::Visual, None, KeyCode::Enter) => {
                    self.insert.submit();
                    self.mode = Mode::Insert;
                    return Ok(Action::Execute)
                }

                // delete selection
                (Mode::Visual, None, KeyCode::Char('d')) => {
                    self.insert.replace_str_inclusive("", Some(&mut self.clipboard));
                    self.mode = Mode::Normal;
                },

                // ready
                (Mode::Normal, None, KeyCode::Char('d')) => self.ready = Some('d'),
                (Mode::Normal, Some('d'), KeyCode::Char('d')) => {
                    self.clipboard.clear();
                    self.clipboard.push_str(self.insert.as_str());
                    self.insert.clear();
                },

                (Mode::Normal | Mode::Visual, None, KeyCode::Char('g')) => self.ready = Some('g'),
                (Mode::Normal | Mode::Visual, Some('g'), KeyCode::Char('h')) => {
                    self.insert.move_head();

                    if matches!(self.mode, Mode::Normal) {
                        self.insert.cursor_mut().start = self.insert.cursor_mut().end;
                    }
                },
                (Mode::Normal | Mode::Visual, Some('g'), KeyCode::Char('l')) => {
                    self.insert.move_end();

                    if matches!(self.mode, Mode::Normal) {
                        self.insert.cursor_mut().start = self.insert.cursor_mut().end;
                    }
                },

                // clipboard
                (Mode::Normal | Mode::Visual, None, KeyCode::Char('y')) => {
                    self.clipboard.clear();
                    self.clipboard.push_str(self.insert.selected());
                },
                (Mode::Normal, None, KeyCode::Char('p')) => {
                    let char_len = self.insert.char_len();
                    let cursor = self.insert.cursor_mut();
                    cursor.end = char_len.min(cursor.end.saturating_add(1));

                    self.insert.push_str(&self.clipboard);

                    let cursor = self.insert.cursor_mut();
                    cursor.end = cursor.start.max(cursor.end.saturating_sub(1));
                }
                (Mode::Visual, None, KeyCode::Char('p')) => {
                    let cursor = self.insert.cursor();
                    let start = cursor.start.min(cursor.end);
                    self.insert.replace_str_inclusive(&self.clipboard, None);
                    *self.insert.cursor_mut() =
                        start..(start + self.clipboard.chars().count().saturating_sub(1));
                },

                // visual cancel
                (Mode::Normal | Mode::Visual, None, KeyCode::Char(',')) => self.ready = Some(','),
                (Mode::Normal | Mode::Visual, Some(','), KeyCode::Char(',')) => {
                    self.mode = Mode::Normal;
                    self.insert.cursor_mut().start = self.insert.cursor_mut().end;
                },

                // path selector
                (Mode::PathSelector, None,KeyCode::Char('j')) =>
                    self.path_selector.down()?,
                (Mode::PathSelector, None, KeyCode::Char('k')) =>
                    self.path_selector.up()?,
                (Mode::PathSelector, None, KeyCode::Char('h')) =>
                    self.path_selector.left()?,
                (Mode::PathSelector, None, KeyCode::Char('l')) =>
                    self.path_selector.right()?,
                (Mode::PathSelector, None, KeyCode::Char('.')) => {
                    self.path_selector.toggle_hidden_file();
                    self.path_selector.cd(Path::new("."))?;
                },
                (Mode::PathSelector, None, KeyCode::Char(',')) => {
                    self.path_selector.toggle_case_sensitive();
                    self.path_selector.cd(Path::new("."))?;
                },
                (Mode::PathSelector, None, KeyCode::Char('r')) =>
                    self.path_selector.cd(Path::new("."))?,
                (Mode::PathSelector, None, KeyCode::Char('q')) => {
                    self.mode = Mode::Insert;
                },
                (Mode::PathSelector, None, KeyCode::Char('y')) => {
                    if let Some(path) = self.path_selector.selected() {
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
                        self.clipboard.clear();
                        self.clipboard.push_str(path);
                    }
                },
                (Mode::PathSelector, None, KeyCode::Enter) => {
                    if let Some(path) = self.path_selector.selected() {
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
                    }
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
