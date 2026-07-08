pub mod line;
pub mod ui;
pub mod path_selector;
pub mod complete;

use std::mem;
use std::path::Path;
use std::cell::RefCell;
use anyhow::Context;
use crossterm::event::{ Event, KeyCode, KeyEvent, KeyModifiers as KM };
use line::EditableLine;
use line::Suggestion;
use path_selector::PathSelector;
use complete::CompleteSelector;
use crate::ui::layout;
use crate::shell::env::Environment;
use crate::ui::render::{ Renderer, TermTarget };

pub struct Editor {
    pub ui: ui::Editor,
    pub mode: Mode,
    pub insert: EditableLine,
    pub command: EditableLine,
    pub ready: Option<char>,
    pub path_selector: PathSelector,
    pub complete_selector: CompleteSelector,
    pub suggestion: Suggestion,
    clipboard: String,
}

#[derive(PartialEq, Eq, Clone, Copy, Debug)]
pub enum Mode {
    Insert,
    Normal,
    Visual,
    PathSelector,
    CompleteSelector,
}

#[derive(Clone, Copy, Debug)]
pub enum Action {
    Continue,
    Completion,
    Reload,
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
        let ui = ui::Editor::default();
        let path_selector = PathSelector::new()?;
        let complete_selector = CompleteSelector::default();

        Ok(Editor {
            ui, path_selector, complete_selector,
            mode: Mode::Insert,
            insert: EditableLine::default(),
            command: EditableLine::default(),
            ready: None,
            suggestion: Suggestion::default(),
            clipboard: String::new(),
        })
    }

    pub fn step(&mut self, env: &RefCell<Environment>, event: Event)
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
            (
                Mode::Insert | Mode::PathSelector | Mode::CompleteSelector,
                Event::Key(KeyEvent { modifiers, code, .. })
            )
                if (modifiers == KM::CONTROL && code == KeyCode::Char('c'))
                    || (modifiers == KM::NONE && code == KeyCode::Esc)
                    || (modifiers == KM::ALT && code == KeyCode::Char(' '))
            => {
                if self.command.is_empty() {
                    self.mode = Mode::Normal;
                } else {
                   self.command.clear(); 
                }
            },

            // Insert: apply suggestion
            (Mode::Insert, Event::Key(KeyEvent { modifiers, code, .. }))
                if modifiers.contains(KM::ALT)
                    && code == KeyCode::Char('l')
                    && self.insert.is_editing()
                    && self.insert.is_point_end()
                    && self.suggestion.has_suggest()
            => {
                self.suggestion.apply(&mut self.insert);
            },            

            // Insert
            (Mode::Insert, Event::Key(KeyEvent { modifiers, code, .. }))
                if modifiers.contains(KM::SHIFT & KM::NONE)
                    && !modifiers.intersects(KM::CONTROL | KM::ALT)
            => {
                match code {
                    KeyCode::Char('\r') => (),
                    KeyCode::Char(c) => self.insert.push(c),
                    KeyCode::Right if
                        self.insert.is_editing()
                        && self.insert.is_point_end()
                        && self.suggestion.has_suggest()
                    => {
                        self.suggestion.apply(&mut self.insert);
                    },
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

                self.suggestion.clear();

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

            // Normal && PathSelector && CompleteSelector with command
            (
                Mode::Normal | Mode::PathSelector | Mode::CompleteSelector,
                Event::Key(KeyEvent { modifiers, code, .. })
            )
                if modifiers.contains(KM::SHIFT & KM::NONE) && !self.command.is_empty()
            => {
                match (self.mode, self.command.first(), code) {
                    // command input
                    (_, _, KeyCode::Char('\r')) => (),
                    (_, Some(_), KeyCode::Char(c))
                        => self.command.push(c),
                    (_, Some(_), KeyCode::Backspace)
                        => self.command.backspace(),
                    (_, Some(_), KeyCode::Delete)
                        => self.command.delete(),
                    (_, Some(_), KeyCode::Left)
                        => self.command.move_left(),
                    (_, Some(_), KeyCode::Right)
                        => self.command.move_right(),
                    (_, Some(_), KeyCode::Esc)
                        => self.command.clear(),
                    (Mode::Normal, Some(':'), KeyCode::Enter)
                        => match self.command.as_str() {
                            ":quit" => return Ok(Action::Break),
                            ":reload" => {
                                self.command.clear();
                                return Ok(Action::Reload);
                            },
                            _ => self.command.clear(),
                        }
                    (Mode::PathSelector, Some('/'), KeyCode::Enter)
                        => {
                            if let Some(cmd) = self.command.as_str().strip_prefix('/') {
                                self.path_selector.search = cmd.into();
                            }

                            self.command.clear();
                            self.path_selector.cd(Path::new("."))?;
                            self.path_selector.search_down()?;
                        }
                    (Mode::PathSelector, Some(':'), KeyCode::Enter)
                        => {
                            if let Some(cmd) = self.command.as_str().strip_prefix(":glob") {
                                let cmd = cmd.trim_start();

                                if cmd.is_empty() {
                                    self.path_selector.set_glob(None);
                                } else {
                                    self.path_selector.set_glob(Some(glob::Pattern::new(cmd)?));
                                }

                                self.path_selector.cd(Path::new("."))?;
                            }

                            self.command.clear();
                        }
                    (Mode::CompleteSelector, Some('/'), KeyCode::Enter)
                        => {
                            if let Some(cmd) = self.command.as_str().strip_prefix('/') {
                                self.complete_selector.search = cmd.into();
                            }

                            self.command.clear();
                            self.complete_selector.search_down();
                        },
                    (_, Some(_), KeyCode::Enter) => {
                        self.command.clear();
                    }
                    _ => (),      
                }
            },

            // Normal && Visual && PathSelector && CompleteSelector
            (
                Mode::Normal | Mode::Visual | Mode::PathSelector | Mode::CompleteSelector,
                Event::Key(KeyEvent { modifiers, code, .. })
            )
                if modifiers.contains(KM::SHIFT & KM::NONE) && self.command.is_empty()
            => match (self.mode, self.ready.take(), code) {
                // command mode
                (Mode::Normal | Mode::PathSelector, None, KeyCode::Char(':' | ';'))
                    => self.command.push(':'),
                (
                    Mode::Normal | Mode::PathSelector | Mode::CompleteSelector,
                    None,
                    KeyCode::Char('/')
                )
                    => self.command.push('/'),

                // normal and visual
                (_, None, KeyCode::Char('i'))
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
                (Mode::Normal | Mode::Visual, None, KeyCode::Char('0')) => {
                    self.insert.move_head();

                    if matches!(self.mode, Mode::Normal) {
                        self.insert.cursor_mut().start = self.insert.cursor_mut().end;
                    }
                },
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
                    self.suggestion.clear();
                },
                (Mode::Normal | Mode::Visual, None, KeyCode::Down | KeyCode::Char('j')) => {
                    self.insert.down();
                    self.suggestion.clear();
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
                    self.suggestion.clear();
                    self.mode = Mode::Normal;
                },

                // ready
                (Mode::Normal, None, KeyCode::Char('d')) => self.ready = Some('d'),
                (Mode::Normal, Some('d'), KeyCode::Char('d')) => {
                    self.clipboard.clear();
                    self.clipboard.push_str(self.insert.as_str());
                    self.insert.clear();
                    self.suggestion.clear();
                },

                // gh
                (Mode::Normal | Mode::Visual, None, KeyCode::Char('g')) => self.ready = Some('g'),
                (Mode::Normal | Mode::Visual, Some('g'), KeyCode::Char('h')) => {
                    self.insert.move_head();

                    if matches!(self.mode, Mode::Normal) {
                        self.insert.cursor_mut().start = self.insert.cursor_mut().end;
                    }
                },
                // gl
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
                    self.suggestion.clear();

                    let cursor = self.insert.cursor_mut();
                    cursor.end = cursor.start.max(cursor.end.saturating_sub(1));
                }
                (Mode::Visual, None, KeyCode::Char('p')) => {
                    let cursor = self.insert.cursor();
                    let start = cursor.start.min(cursor.end);
                    self.insert.replace_str_inclusive(&self.clipboard, None);
                    *self.insert.cursor_mut() =
                        start..(start + self.clipboard.chars().count().saturating_sub(1));
                    self.suggestion.clear();
                },

                // visual cancel
                (_, None, KeyCode::Char(',')) => self.ready = Some(','),
                (_, Some(','), KeyCode::Char(',')) => {
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
                (Mode::PathSelector, None, KeyCode::Char('c')) => {
                    self.path_selector.toggle_case_sensitive();
                    self.path_selector.cd(Path::new("."))?;
                },
                (Mode::PathSelector, None, KeyCode::Char('r')) =>
                    self.path_selector.cd(Path::new("."))?,
                (Mode::PathSelector, None, KeyCode::Char('q')) => {
                    self.mode = Mode::Insert;
                },
                (Mode::PathSelector, None, KeyCode::Char('n')) =>
                    self.path_selector.search_down()?,
                (Mode::PathSelector, None, KeyCode::Char('N')) =>
                    self.path_selector.search_up()?,

                // gg
                (Mode::PathSelector, None, KeyCode::Char('g')) => self.ready = Some('g'),
                (Mode::PathSelector, Some('g'), KeyCode::Char('g')) =>
                    self.path_selector.move_top()?,
                // ge
                (Mode::PathSelector, Some('g'), KeyCode::Char('e')) =>
                    self.path_selector.move_bottom()?,
                                
                (Mode::PathSelector, None, KeyCode::Char('y')) => {
                    if let Some(path) = self.path_selector.selected() {
                        let env = env.borrow();
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
                    use crate::util::path::EscapePath;
                    
                    if let Some(path) = self.path_selector.selected() {
                        let env = env.borrow();
                        let path = path
                            .strip_prefix(env.pwd())
                            .unwrap_or(&path)
                            .to_str()
                            .context("non-utf8 path are unsupported")?;
                        let path = if !path.is_empty() {
                            EscapePath(path).to_string()
                        } else {
                            ".".into()
                        };
                        if self.insert.cursor().is_empty() {
                            self.insert.push_str(&path);
                        } else {
                            self.insert.replace_str_inclusive(&path, None);
                        }
                        self.suggestion.clear();
                        self.mode = Mode::Insert;
                    }
                },

                (Mode::CompleteSelector, None, KeyCode::Char('h')) => {
                    if let Some(cur) = self.complete_selector.cur.checked_sub(1) {
                        self.complete_selector.cur = cur;
                        self.complete_selector.update_window();
                    }
                },
                (Mode::CompleteSelector, None, KeyCode::Char('j')) => {
                    let cur = self.complete_selector.cur + self.complete_selector.column;
                    if cur < self.complete_selector.list.len() {
                        self.complete_selector.cur = cur;
                        self.complete_selector.update_window();
                    }
                },
                (Mode::CompleteSelector, None, KeyCode::Char('k')) => {
                    if let Some(cur) = self.complete_selector.cur
                        .checked_sub(self.complete_selector.column)
                    {
                        self.complete_selector.cur = cur;
                        self.complete_selector.update_window();
                    }
                },
                (Mode::CompleteSelector, None, KeyCode::Char('l')) => {
                    let cur = self.complete_selector.cur + 1;
                    if cur < self.complete_selector.list.len() {
                        self.complete_selector.cur = cur;
                        self.complete_selector.update_window();
                    }
                },
                (Mode::CompleteSelector, None, KeyCode::Char('n')) =>
                    self.complete_selector.search_down(),
                (Mode::CompleteSelector, None, KeyCode::Char('N')) =>
                    self.complete_selector.search_up(),
                (Mode::CompleteSelector, None, KeyCode::Enter) => {
                    let s = &self.complete_selector.list[self.complete_selector.cur];
                    self.insert.replace_str_inclusive(s, None);
                    self.suggestion.clear();
                    self.mode = Mode::Insert;
                },
                (Mode::CompleteSelector, None, KeyCode::Char(' ')) => {
                    let s = &self.complete_selector.list[self.complete_selector.cur];
                    self.insert.replace_str_inclusive(s, None);
                    self.insert.push(' ');
                    self.suggestion.clear();
                    self.mode = Mode::Insert;
                },

                (Mode::PathSelector | Mode::CompleteSelector, None, KeyCode::Backspace) => {
                    self.insert.backspace();
                    self.mode = Mode::Insert;
                },                
                _ => ()
            },
            _ => ()
        }

        Ok(Action::Continue)
    }

    pub fn mode_switch<T: TermTarget>(&mut self, prev_mode: Mode, renderer: &mut Renderer<T>) -> anyhow::Result<()> {
        match (prev_mode, self.mode) {
            (x, y) if x == y => (),
            (_, Mode::PathSelector) => {
                if self.ui.layout[self.ui.command].justify != layout::Justify::End {
                    self.ui.layout[self.ui.command].justify = layout::Justify::End;
                }

                if self.ui.layout[self.ui.error].justify != layout::Justify::End {
                    self.ui.layout[self.ui.error].justify = layout::Justify::End;
                }                
                
                if self.ui.layout[self.ui.path_selector].hidden {
                    self.ui.layout[self.ui.path_selector].hidden = false;
                }

                renderer.enter_alternate()?;
            },
            (_, Mode::CompleteSelector) => {
                debug_assert_ne!(prev_mode, Mode::PathSelector);
                
                if self.ui.layout[self.ui.complete_selector].hidden {
                    self.ui.layout[self.ui.complete_selector].hidden = false;
                }
            },
            (..) => {
                if self.ui.layout[self.ui.command].justify != layout::Justify::Start {
                    self.ui.layout[self.ui.command].justify = layout::Justify::Start;
                }

                if self.ui.layout[self.ui.error].justify != layout::Justify::Start {
                    self.ui.layout[self.ui.error].justify = layout::Justify::Start;
                }

                if !self.ui.layout[self.ui.path_selector].hidden {
                    self.ui.layout[self.ui.path_selector].hidden = true;
                }

                if !self.ui.layout[self.ui.complete_selector].hidden {
                    self.ui.layout[self.ui.complete_selector].hidden = true;
                }
            }
        }

        match (prev_mode, self.mode) {
            (x, y) if x == y => (),
            (Mode::PathSelector, _) => {
                self.path_selector.clear();
                renderer.leave_alternate()?;
            },
            (Mode::CompleteSelector, _) => {
                self.complete_selector.clear();
            },
            (..) => (),
        }

        Ok(())
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
            None,
            // compele selector
            None,
        ];

        STR[self as usize]
    }
}
