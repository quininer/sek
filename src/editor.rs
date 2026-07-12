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
    pub path_selector: PathSelector,
    pub complete_selector: CompleteSelector,
    pub suggestion: Suggestion,
    ready: Option<char>,
    clipboard: String,
}

#[derive(PartialEq, Eq, Clone, Copy, Debug)]
pub enum Mode {
    Insert,
    Normal,
    Visual,
    Path,
    Complete,
}

#[derive(Clone, Copy, Debug)]
pub enum Action {
    Continue,
    Completion,
    QueryHistory,
    Reload,
    Execute,
    Break,
}

pub enum StepAction {
    Nop,
    Input(char),
    InputCommand(char),
    Break,
    Cancel,
    Execute,
    ExecuteSpace,
    Completion,
    ApplySuggestion,
    EnterInsert,
    EnterInsertAppend,
    EnterVisual,
    EnterNormal,
    Backspace,
    Delete,
    MoveLeft,
    MoveRight,
    MoveHead,
    MoveEnd,
    HistoryUp,
    HistoryDown,
    SwapCursor,
    SelectAll,
    SelectNextWord,
    SelectBackWord,
    DeleteSelected,
    DeleteAll,
    Copy,
    Paste,
    SelectorUp,
    SelectorDown,
    SelectorLeft,
    SelectorRight,
    SelectorTop,
    SelectorBottom,
    SelectorHiddenToggle,
    SelectorCaseSensitiveToggle,
    SelectorRefresh,
    SearchDown,
    SearchUp,
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

    pub fn step(&mut self, event: Event)
        -> StepAction
    {
        match (self.mode, event) {
            (Mode::Insert, Event::Key(KeyEvent { modifiers, code, .. }))
                if modifiers == KM::CONTROL
                    && code == KeyCode::Char('d')
                    && self.insert.is_empty()
            => StepAction::Break,
            (
                Mode::Insert | Mode::Path | Mode::Complete,
                Event::Key(KeyEvent { modifiers, code, .. })
            )
                if (modifiers == KM::CONTROL && code == KeyCode::Char('c'))
                    || (modifiers == KM::NONE && code == KeyCode::Esc)
                    || (modifiers == KM::ALT && code == KeyCode::Char(' '))
            => StepAction::Cancel,
            (Mode::Insert, Event::Key(KeyEvent { modifiers, code, .. }))
                if modifiers.contains(KM::ALT)
                    && code == KeyCode::Char('l')
                    && self.insert.is_editing()
                    && self.insert.is_point_end()
                    && self.suggestion.has_suggest()
            => StepAction::ApplySuggestion,

            (Mode::Insert, Event::Key(KeyEvent { modifiers, code, .. }))
                if modifiers.contains(KM::SHIFT & KM::NONE)
                    && !modifiers.intersects(KM::CONTROL | KM::ALT)
            => match code {
                KeyCode::Char('\r') => StepAction::Nop,
                KeyCode::Char(c) => StepAction::Input(c),
                KeyCode::Right if 
                    self.insert.is_editing()
                    && self.insert.is_point_end()
                    && self.suggestion.has_suggest()
                => StepAction::ApplySuggestion,
                KeyCode::Backspace => StepAction::Backspace,
                KeyCode::Delete => StepAction::Delete,
                KeyCode::Left => StepAction::MoveLeft,
                KeyCode::Right => StepAction::MoveRight,
                KeyCode::Home => StepAction::MoveHead,
                KeyCode::End => StepAction::MoveEnd,
                KeyCode::Up => StepAction::HistoryUp,
                KeyCode::Down => StepAction::HistoryDown,
                KeyCode::Tab => StepAction::Completion,
                KeyCode::Enter => StepAction::Execute,
                _ => StepAction::Nop,
            },
            (Mode::Normal | Mode::Visual, Event::Key(KeyEvent { modifiers, code, .. }))
                if modifiers.contains(KM::ALT) && code == KeyCode::Char(';')
            => StepAction::SwapCursor,

            (
                Mode::Normal | Mode::Path | Mode::Complete,
                Event::Key(KeyEvent { modifiers, code, .. })
            )
                if modifiers.contains(KM::SHIFT & KM::NONE) && !self.command.is_empty()
            => match (self.command.first(), code) {
                (_, KeyCode::Char('\r')) => StepAction::Nop,
                (Some(_), KeyCode::Char(c)) => StepAction::Input(c),
                (Some(_), KeyCode::Backspace) => StepAction::Backspace,
                (Some(_), KeyCode::Delete) => StepAction::Delete,
                (Some(_), KeyCode::Left) => StepAction::MoveLeft,
                (Some(_), KeyCode::Right) => StepAction::MoveRight,
                (Some(_), KeyCode::Home) => StepAction::MoveHead,
                (Some(_), KeyCode::End) => StepAction::MoveEnd,
                (Some(_), KeyCode::Esc) => StepAction::Cancel,
                (Some(_), KeyCode::Enter) => StepAction::Execute,
                _ => StepAction::Nop,
            }

            (
                Mode::Normal | Mode::Visual | Mode::Path | Mode::Complete,
                Event::Key(KeyEvent { modifiers, code, .. })
            )
                if modifiers.contains(KM::SHIFT & KM::NONE) && self.command.is_empty()
            => match (self.ready.take(), code) {
                (None, KeyCode::Char(':' | ';')) => StepAction::InputCommand(':'),
                (None, KeyCode::Char('/')) => StepAction::InputCommand('/'),
                (None, KeyCode::Char('i')) => StepAction::EnterInsert,
                (None, KeyCode::Char('a')) => StepAction::EnterInsertAppend,
                (None, KeyCode::Char('v')) if matches!(self.mode, Mode::Normal)
                    => StepAction::EnterVisual,
                (None, KeyCode::Char('v')) if matches!(self.mode, Mode::Visual)
                    => StepAction::EnterNormal,

                (None, KeyCode::Char('h')) if matches!(self.mode, Mode::Normal | Mode::Visual)
                    => StepAction::MoveLeft,
                (None, KeyCode::Char('l')) if matches!(self.mode, Mode::Normal | Mode::Visual)
                    => StepAction::MoveRight,
                (None, KeyCode::Char('j')) if matches!(self.mode, Mode::Normal | Mode::Visual)
                    => StepAction::HistoryDown,
                (None, KeyCode::Char('k')) if matches!(self.mode, Mode::Normal | Mode::Visual)
                    => StepAction::HistoryUp,

                (None, KeyCode::Char('h')) if matches!(self.mode, Mode::Path | Mode::Complete)
                    => StepAction::SelectorLeft,
                (None, KeyCode::Char('l')) if matches!(self.mode, Mode::Path | Mode::Complete)
                    => StepAction::SelectorRight,
                (None, KeyCode::Char('j')) if matches!(self.mode, Mode::Path | Mode::Complete)
                    => StepAction::SelectorDown,
                (None, KeyCode::Char('k')) if matches!(self.mode, Mode::Path | Mode::Complete)
                    => StepAction::SelectorUp,

                (None, KeyCode::Char('x')) if matches!(self.mode, Mode::Normal | Mode::Visual)
                    => StepAction::SelectAll,
                (None, KeyCode::Char('w')) if matches!(self.mode, Mode::Normal | Mode::Visual)
                    => StepAction::SelectNextWord,
                (None, KeyCode::Char('b')) if matches!(self.mode, Mode::Normal | Mode::Visual)
                    => StepAction::SelectBackWord,
                (None, KeyCode::Char('d')) if matches!(self.mode, Mode::Visual)
                    => StepAction::DeleteSelected,

                (None, KeyCode::Enter) => StepAction::Execute,

                (None, KeyCode::Char('y')) if matches!(self.mode, Mode::Normal | Mode::Visual | Mode::Path)
                    => StepAction::Copy,
                (None, KeyCode::Char('p')) if matches!(self.mode, Mode::Normal | Mode::Visual)
                    => StepAction::Paste,                

                // dd
                (None, KeyCode::Char('d')) if matches!(self.mode, Mode::Normal)
                    => {
                        self.ready = Some('d');
                        StepAction::Nop
                    },
                (Some('d'), KeyCode::Char('d')) if matches!(self.mode, Mode::Normal)
                    => StepAction::DeleteAll,

                // gh gl gg ge
                (None, KeyCode::Char('g'))
                    => {
                        self.ready = Some('g');
                        StepAction::Nop
                    },
                (Some('g'), KeyCode::Char('h')) if matches!(self.mode, Mode::Normal)
                    => StepAction::MoveHead,
                (Some('g'), KeyCode::Char('l')) if matches!(self.mode, Mode::Normal)
                    => StepAction::MoveEnd,

                (Some('g'), KeyCode::Char('g')) if matches!(self.mode, Mode::Path | Mode::Complete)
                    => StepAction::SelectorTop,
                (Some('g'), KeyCode::Char('e')) if matches!(self.mode, Mode::Path | Mode::Complete)
                    => StepAction::SelectorBottom,

                // ,,
                (None, KeyCode::Char(',')) => {
                    self.ready = Some(',');
                    StepAction::Nop
                },
                (Some(','), KeyCode::Char(',')) => StepAction::Cancel,

                (None, KeyCode::Char('.')) if matches!(self.mode, Mode::Path)
                    => StepAction::SelectorHiddenToggle,
                (None, KeyCode::Char('c')) if matches!(self.mode, Mode::Path)
                    => StepAction::SelectorCaseSensitiveToggle,
                (None, KeyCode::Char('r')) if matches!(self.mode, Mode::Path | Mode::Complete)
                    => StepAction::SelectorRefresh,
                (None, KeyCode::Char('q')) if matches!(self.mode, Mode::Path | Mode::Complete)
                    => StepAction::Cancel,
                (None, KeyCode::Char('n')) if matches!(self.mode, Mode::Path | Mode::Complete)
                    => StepAction::SearchDown,
                (None, KeyCode::Char('N')) if matches!(self.mode, Mode::Path | Mode::Complete)
                    => StepAction::SearchUp,

                (None, KeyCode::Char(' ')) if matches!(self.mode, Mode::Path | Mode::Complete)
                    => StepAction::ExecuteSpace,

                (None, KeyCode::Backspace) if matches!(self.mode, Mode::Path | Mode::Complete)
                    => StepAction::Backspace,
                                    
                _ => StepAction::Nop,
            }
                        
            _ => StepAction::Nop
        }
    }

    pub fn apply(&mut self, env: &RefCell<Environment>, action: StepAction)
        -> anyhow::Result<Action>
    {
        use StepAction::*;

        match action {
            Nop => (),
            Input(c) if self.command.is_empty()
                => self.insert.push(c),
            Input(c) | InputCommand(c)
                => self.command.push(c),
            Break
                => return Ok(Action::Break),
            Cancel if self.command.is_empty()
                => {
                    self.mode = Mode::Normal;
                    self.insert.cursor_mut().start = self.insert.cursor_mut().end;
                },
            Cancel
                => self.command.clear(),
            ApplySuggestion
                => self.suggestion.apply(&mut self.insert),
            Completion
                => return Ok(Action::Completion),
            Execute if matches!(self.mode, Mode::Insert) && self.command.is_empty()
                => return Ok(Action::Execute),
            Execute if matches!(self.mode, Mode::Normal | Mode::Visual) && self.command.is_empty()
                => {
                    self.mode = Mode::Insert;
                    return Ok(Action::Execute)
                },

            Backspace if self.command.is_empty() => {
                self.insert.backspace();
                self.suggestion.clear();

                if matches!(self.mode, Mode::Normal) {
                    self.insert.cursor_mut().start = self.insert.cursor_mut().end;
                }

                if matches!(self.mode, Mode::Path | Mode::Complete) {
                    self.mode = Mode::Insert;
                }
            },
            Delete if self.command.is_empty() => {
                self.insert.delete();
                self.suggestion.clear();

                if matches!(self.mode, Mode::Normal) {
                    self.insert.cursor_mut().start = self.insert.cursor_mut().end;
                }

                if matches!(self.mode, Mode::Path | Mode::Complete) {
                    self.mode = Mode::Insert;
                }                
            },
            MoveLeft if self.command.is_empty() => {
                self.insert.move_left();
                self.suggestion.clear();

                if matches!(self.mode, Mode::Normal) {
                    self.insert.cursor_mut().start = self.insert.cursor_mut().end;
                }
            },
            MoveRight if self.command.is_empty() => {
                self.insert.move_right();
                self.suggestion.clear();

                if matches!(self.mode, Mode::Normal) {
                    self.insert.cursor_mut().start = self.insert.cursor_mut().end;
                }
            },
            MoveHead if self.command.is_empty() => {
                self.insert.move_head();
                self.suggestion.clear();

                if matches!(self.mode, Mode::Normal) {
                    self.insert.cursor_mut().start = self.insert.cursor_mut().end;
                }
            },
            MoveEnd if self.command.is_empty() => {
                self.insert.move_end();
                self.suggestion.clear();

                if matches!(self.mode, Mode::Normal) {
                    self.insert.cursor_mut().start = self.insert.cursor_mut().end;
                }
            },

            Backspace => self.command.backspace(),
            Delete => self.command.delete(),
            MoveLeft => self.command.move_left(),
            MoveRight => self.command.move_right(),
            MoveHead => self.command.move_head(),
            MoveEnd => self.command.move_end(),

            SelectAll => {
                let end = self.insert.char_len();
                let cursor = self.insert.cursor_mut();
                cursor.start = 0;
                cursor.end = end;
            },
            SelectNextWord => {
                let span = self.insert.move_right_word();
                let cursor = self.insert.cursor_mut();
                match self.mode {
                    Mode::Normal => *cursor = span,
                    Mode::Visual => cursor.end = span.end,
                    _ => unreachable!()
                }
            },
            SelectBackWord => {
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

            DeleteSelected => {
                self.insert.replace_str_inclusive("", Some(&mut self.clipboard));
                self.suggestion.clear();

                if matches!(self.mode, Mode::Visual) {
                    self.mode = Mode::Normal;
                }
            },
            DeleteAll => {
                self.clipboard.clear();
                self.clipboard.push_str(self.insert.as_str());
                self.insert.clear();
                self.suggestion.clear();
            },

            HistoryUp if self.insert.is_editing()
                => return Ok(Action::QueryHistory),
            HistoryUp => self.insert.up(),
            HistoryDown => self.insert.down(),

            SwapCursor => {
                let cursor = self.insert.cursor_mut();
                mem::swap(&mut cursor.start, &mut cursor.end);
            },

            Execute if matches!(self.mode, Mode::Normal | Mode::Visual) && !self.command.is_empty()
                => match self.command.as_str() {
                    ":quit" => return Ok(Action::Break),
                    ":reload" => {
                        self.command.clear();
                        return Ok(Action::Reload);
                    },
                    _ => self.command.clear(),
                },

            Execute if matches!(self.mode, Mode::Path) && !self.command.is_empty() => {
                if let Some(cmd) = self.command.as_str().strip_prefix('/') {
                    self.path_selector.search = cmd.into();
                    self.path_selector.cd(Path::new("."))?;
                    self.path_selector.search_down()?;                    
                }

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
            },

            Execute if matches!(self.mode, Mode::Complete) && !self.command.is_empty() => {
                if let Some(cmd) = self.command.as_str().strip_prefix('/') {
                    self.complete_selector.search = cmd.into();
                    self.complete_selector.search_down();
                }

                self.command.clear();
            },

            Execute if matches!(self.mode, Mode::Path) => {
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

            Execute if matches!(self.mode, Mode::Complete) => {
                let s = &self.complete_selector.list[self.complete_selector.cur];
                self.insert.replace_str_inclusive(s, None);
                self.suggestion.clear();
                self.mode = Mode::Insert;
            },
            ExecuteSpace if matches!(self.mode, Mode::Complete) => {
                let s = &self.complete_selector.list[self.complete_selector.cur];
                self.insert.replace_str_inclusive(s, None);
                self.insert.push(' ');
                self.suggestion.clear();
                self.mode = Mode::Insert;
            },

            EnterInsert => self.mode = Mode::Insert,
            EnterInsertAppend => {
                self.insert.move_right();
                self.mode = Mode::Insert;
            },
            EnterNormal => self.mode = Mode::Normal,
            EnterVisual => self.mode = Mode::Visual,
            
            Copy if matches!(self.mode, Mode::Normal | Mode::Visual) => {
                self.clipboard.clear();
                self.clipboard.push_str(self.insert.selected());
            },
            Copy if matches!(self.mode, Mode::Path) => {
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
            Paste if matches!(self.mode, Mode::Normal) => {
                let char_len = self.insert.char_len();
                let cursor = self.insert.cursor_mut();
                cursor.end = char_len.min(cursor.end.saturating_add(1));

                self.insert.push_str(&self.clipboard);
                self.suggestion.clear();

                let cursor = self.insert.cursor_mut();
                cursor.end = cursor.start.max(cursor.end.saturating_sub(1));
            },
            Paste if matches!(self.mode, Mode::Visual) => {
                let cursor = self.insert.cursor();
                let start = cursor.start.min(cursor.end);
                self.insert.replace_str_inclusive(&self.clipboard, None);
                *self.insert.cursor_mut() =
                    start..(start + self.clipboard.chars().count().saturating_sub(1));
                self.suggestion.clear();                
            },

            SelectorUp if matches!(self.mode, Mode::Path)
                => self.path_selector.up()?,
            SelectorDown if matches!(self.mode, Mode::Path)
                => self.path_selector.down()?,
            SelectorLeft if matches!(self.mode, Mode::Path)
                => self.path_selector.left()?,
            SelectorRight if matches!(self.mode, Mode::Path)
                => self.path_selector.right()?,
            SelectorTop if matches!(self.mode, Mode::Path)
                => self.path_selector.move_top()?,
            SelectorBottom if matches!(self.mode, Mode::Path)
                => self.path_selector.move_bottom()?,

            SearchDown if matches!(self.mode, Mode::Path)
                => self.path_selector.search_down()?,
            SearchUp if matches!(self.mode, Mode::Path)
                => self.path_selector.search_up()?,

            SelectorHiddenToggle if matches!(self.mode, Mode::Path)
                => {
                    self.path_selector.toggle_hidden_file();
                    self.path_selector.cd(Path::new("."))?;
                },
            SelectorCaseSensitiveToggle if matches!(self.mode, Mode::Path)
                => {
                    self.path_selector.toggle_case_sensitive();
                    self.path_selector.cd(Path::new("."))?;
                },
            SelectorCaseSensitiveToggle if matches!(self.mode, Mode::Path)
                => self.path_selector.cd(Path::new("."))?,

            SelectorUp if matches!(self.mode, Mode::Complete)
                => {
                    if let Some(cur) = self.complete_selector.cur
                        .checked_sub(self.complete_selector.column)
                    {
                        self.complete_selector.cur = cur;
                        self.complete_selector.update_window();
                    }
                },
            SelectorDown if matches!(self.mode, Mode::Complete)
                => {
                    let cur = self.complete_selector.cur + self.complete_selector.column;
                    if cur < self.complete_selector.list.len() {
                        self.complete_selector.cur = cur;
                        self.complete_selector.update_window();
                    }
                },
            SelectorLeft if matches!(self.mode, Mode::Complete)
                => {
                    if let Some(cur) = self.complete_selector.cur.checked_sub(1) {
                        self.complete_selector.cur = cur;
                        self.complete_selector.update_window();
                    }
                },
            SelectorRight if matches!(self.mode, Mode::Complete)
                => {
                    let cur = self.complete_selector.cur + 1;
                    if cur < self.complete_selector.list.len() {
                        self.complete_selector.cur = cur;
                        self.complete_selector.update_window();
                    }
                },
            SelectorTop if matches!(self.mode, Mode::Complete)
                => {
                    self.complete_selector.cur = 0;
                    self.complete_selector.update_window();
                },
            SelectorBottom if matches!(self.mode, Mode::Complete)
                => {
                    self.complete_selector.cur =
                        self.complete_selector.list.len().saturating_sub(1);
                    self.complete_selector.update_window();
                },
                
            SearchDown if matches!(self.mode, Mode::Complete)
                => self.complete_selector.search_down(),
            SearchUp if matches!(self.mode, Mode::Complete)
                => self.complete_selector.search_up(),

            _ => ()
        }

        if matches!(self.mode, Mode::Insert) {
            let cursor = self.insert.cursor_mut();
            cursor.start = cursor.end;
        }

        Ok(Action::Continue)
    }

    pub fn mode_switch<T: TermTarget>(&mut self, prev_mode: Mode, _renderer: &mut Renderer<T>)
        -> anyhow::Result<()>
    {
        match (prev_mode, self.mode) {
            (x, y) if x == y => (),
            (_, Mode::Path) => {
                if self.ui.layout[self.ui.tail].justify != layout::Justify::Stretch {
                    self.ui.layout[self.ui.tail].justify = layout::Justify::Stretch;
                }                
                
                if self.ui.layout[self.ui.path_selector].hidden {
                    self.ui.layout[self.ui.path_selector].hidden = false;
                }
            },
            (_, Mode::Complete) => {
                debug_assert_ne!(prev_mode, Mode::Path);
                
                if self.ui.layout[self.ui.complete_selector].hidden {
                    self.ui.layout[self.ui.complete_selector].hidden = false;
                }
            },
            (..) => {
                if self.ui.layout[self.ui.tail].justify != layout::Justify::Start {
                    self.ui.layout[self.ui.tail].justify = layout::Justify::Start;
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
            (Mode::Path, _) => {
                self.path_selector.clear();
            },
            (Mode::Complete, _) => {
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
