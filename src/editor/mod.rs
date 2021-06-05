pub mod buffer;
pub mod history;
pub mod render;
pub mod command;
pub mod path_selector;

use std::fmt::Write;
use std::path::Path;
use bumpalo::collections::String;
use crossterm::event::{ Event, KeyEvent, KeyCode, KeyModifiers as KM };
use crate::shell::{ Shell, Action };
use crate::editor::buffer::Buffer;
use crate::editor::path_selector::PathSelector;
use crate::editor::command::execute_command;
use crate::util::EscapePath;

pub struct Editor {
    pub line: Buffer,
    pub cmd: Buffer,
    pub error: Option<anyhow::Error>,
    pub mode: Mode,
    path_selector: PathSelector,
    ready: Option<char>,
    ui: Ui
}

#[derive(Default)]
struct Ui {
    columns: u16,
    rows: u16,
    cursor_column: u16,
    cursor_row: u16,
    bottom: u16
}

#[allow(dead_code)]
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Mode {
    Insert,
    Normal,
    Visual,
    PathSelector,
    Completion
}

impl Editor {
    pub fn new() -> anyhow::Result<Self> {
        let (columns, rows) = crossterm::terminal::size()?;
        Ok(Editor {
            line: Buffer::default(),
            cmd: Buffer::default(),
            error: None,
            mode: Mode::Insert,
            path_selector: PathSelector::new(std::cmp::max(rows / 2, 1) as usize),
            ready: None,
            ui: Ui {
                columns, rows,
                ..Default::default()
            }
        })
    }

    pub async fn step(&mut self, shell: &mut Shell, event: Event) -> anyhow::Result<Action> {
        if self.mode != Mode::PathSelector {
            self.path_selector.clear();
        }

        match (self.mode, event) {
            // resize
            (_, Event::Resize(columns, rows)) => {
                self.ui.columns = columns;
                self.ui.rows = rows;
                self.path_selector.set_space(self.ui.available_space());
            },
            // Insert to Normal
            (Mode::Insert, Event::Key(KeyEvent { modifiers, code }))
                if (modifiers == KM::CONTROL && code == KeyCode::Char('c'))
                    || (modifiers == KM::NONE && code == KeyCode::Esc)
                    || (modifiers == KM::ALT && code == KeyCode::Char(' '))
            => {
                self.mode = Mode::Normal;
            },
            // Quit
            (Mode::Insert, Event::Key(KeyEvent { modifiers, code }))
                if modifiers == KM::CONTROL && code == KeyCode::Char('d')
                    && self.line.is_empty()
            => return Ok(Action::Stop),
            // Insert
            (Mode::Insert, Event::Key(KeyEvent { modifiers, code }))
                if modifiers.contains(KM::SHIFT & KM::NONE)
            => match code {
                KeyCode::Char('\r') => (),
                KeyCode::Char(c) => self.line.push(c),
                KeyCode::Backspace => self.line.backspace(),
                KeyCode::Delete => self.line.delete(),
                KeyCode::Left => self.line.move_left(),
                KeyCode::Right => self.line.move_right(),
                KeyCode::Home => self.line.move_head(),
                KeyCode::End => self.line.move_end(),
                KeyCode::Up => self.line.history.up(),
                KeyCode::Down => self.line.history.down(),
                KeyCode::Tab => return Ok(Action::Completion),
                KeyCode::Enter => return Ok(Action::Execute),
                _ => ()
            },
            // Normal Command
            (Mode::Normal | Mode::PathSelector, Event::Key(KeyEvent { modifiers, code }))
                if modifiers.contains(KM::SHIFT & KM::NONE)
                    && (code == KeyCode::Char(':') || code == KeyCode::Char(';'))
            => {
                self.cmd.push(':');
            },
            // Noraml Filter
            (Mode::Normal | Mode::PathSelector, Event::Key(KeyEvent { modifiers, code }))
                if modifiers == KM::NONE && code == KeyCode::Char('/')
            => {
                self.cmd.push('/');
            },
            // Normal Command input
            (Mode::Normal | Mode::PathSelector, Event::Key(KeyEvent { modifiers, code }))
                if modifiers.contains(KM::SHIFT & KM::NONE) && !self.cmd.is_empty()
            => match code {
                KeyCode::Char('\r') => (),
                KeyCode::Char(c) => self.cmd.push(c),
                KeyCode::Backspace => self.cmd.backspace(),
                KeyCode::Delete => self.cmd.delete(),
                KeyCode::Left => self.cmd.move_left(),
                KeyCode::Right => self.cmd.move_right(),
                KeyCode::Up => self.cmd.history.up(),
                KeyCode::Down => self.cmd.history.down(),
                KeyCode::Esc => self.cmd.clear(),
                KeyCode::Enter if self.mode == Mode::Normal
                    => return execute_command(self, shell),
                KeyCode::Enter
                    if self.mode == Mode::PathSelector && self.cmd.first() == Some(':')
                    => return execute_command(self, shell),
                KeyCode::Enter
                    if self.mode == Mode::PathSelector && self.cmd.first() == Some('/')
                => {
                    let bump = shell.bump.clone();
                    let bump = bump.borrow();

                    let mut buf = String::with_capacity_in(self.cmd.len(), &bump);
                    self.cmd.read_into(&mut buf);
                    self.cmd.clear();

                    if let Some(needle) = buf
                        .strip_prefix('/')
                        .map(|buf| buf.trim())
                        .filter(|buf| !buf.is_empty())
                    {
                        self.path_selector.search.clear();
                        self.path_selector.search.push_str(needle);
                        if self.path_selector.current.search_down(needle) {
                            self.path_selector.refresh_current()?;
                        }
                    }
                },
                _ => ()
            },
            // Noraml
            (Mode::Normal, Event::Key(KeyEvent { modifiers, code }))
                if modifiers.contains(KM::SHIFT & KM::NONE) && self.cmd.is_empty()
            => match (self.ready.take(), code) {
                // Normal to Insert
                (None, KeyCode::Char('i')) => self.mode = Mode::Insert,
                (None, KeyCode::Char('a')) => {
                    self.line.move_right();
                    self.mode = Mode::Insert;
                },
                (None, KeyCode::Char('h')) => self.line.move_left(),
                (None, KeyCode::Char('l')) => self.line.move_right(),
                (None, KeyCode::Char('j')) => self.line.history.down(),
                (None, KeyCode::Char('k')) => self.line.history.up(),
                (None, KeyCode::Char('0')) => self.line.move_head(),
                (None, KeyCode::Char('$')) => self.line.move_end(),
                (None, KeyCode::Char('x')) => self.line.delete(),
                (None, KeyCode::Char('D')) => self.line.delete_to_end(),
                (None, KeyCode::Char('s')) => {
                    self.line.delete();
                    self.mode = Mode::Insert;
                },
                (None, KeyCode::Char('C')) => {
                    self.line.delete_to_end();
                    self.line.move_right();
                    self.mode = Mode::Insert;
                },
                (None, KeyCode::Char('d')) => self.ready = Some('d'),
                (None, KeyCode::Char('z')) => self.ready = Some('z'),
                (None, KeyCode::Char('g')) => self.ready = Some('g'),
                (None, KeyCode::Char('r')) => self.ready = Some('r'),
                (None, KeyCode::Char('c')) => self.ready = Some('c'),
                (None, KeyCode::Backspace) => self.line.move_left(),
                (None, KeyCode::Left) => self.line.move_left(),
                (None, KeyCode::Right) => self.line.move_right(),
                (None, KeyCode::Esc) => self.ready = None,
                (None, KeyCode::Enter) if !self.line.is_empty() => {
                    self.mode = Mode::Insert;
                    return Ok(Action::Execute)
                },
                (Some('d'), KeyCode::Char('d')) => self.line.clear(),
                (Some('d'), KeyCode::Char('h')) => self.line.backspace(),
                (Some('d'), KeyCode::Char('l')) => self.line.delete(),
                (Some('c'), KeyCode::Char('h')) => {
                    self.line.backspace();
                    self.mode = Mode::Insert;
                },
                (Some('c'), KeyCode::Char('l')) => {
                    self.line.delete();
                    self.mode = Mode::Insert;
                },
                (Some('r'), KeyCode::Char(c)) => self.line.replace(c),
                (Some('z'), KeyCode::Char('c')) => shell.env.cd("..".as_ref())?,
                (Some('z'), KeyCode::Char('j')) => shell.env.go_back()?,
                (Some('z'), KeyCode::Char('h')) => shell.env.go_home()?,
                (Some('g'), KeyCode::Char('i')) => {
                    self.line.move_end();
                    self.mode = Mode::Insert;
                },
                _ => ()
            },
            (Mode::PathSelector, Event::Key(KeyEvent { modifiers, code }))
                if (modifiers == KM::CONTROL && code == KeyCode::Char('c'))
                    || (modifiers == KM::NONE && code == KeyCode::Esc)
                    || (modifiers == KM::NONE && code == KeyCode::Char('q'))
            => {
                self.mode = Mode::Insert;
            },
            (Mode::PathSelector, Event::Key(KeyEvent { modifiers, code }))
                if modifiers.contains(KM::SHIFT & KM::NONE)
            => match (self.ready.take(), code) {
                (None, KeyCode::Char('h')) => self.path_selector.left()?,
                (None, KeyCode::Char('l')) => self.path_selector.right()?,
                (None, KeyCode::Char('j')) => self.path_selector.down()?,
                (None, KeyCode::Char('k')) => self.path_selector.up()?,
                (None, KeyCode::Char('a')) => {
                    self.path_selector.toggle_hidden_file();
                    self.path_selector.cd(".".as_ref())?;
                },
                (None, KeyCode::Char('c')) => self.path_selector.toggle_case_sensitive(),
                (None, KeyCode::Char('g')) => self.ready = Some('g'),
                (None, KeyCode::Char('G')) => {
                    self.path_selector.current.move_to_bottom();
                    self.path_selector.cd(".".as_ref())?;
                },
                (None, KeyCode::Char('n')) => if self.path_selector.current.search_down(&self.path_selector.search) {
                    self.path_selector.refresh_current()?;
                },
                (None, KeyCode::Char('N')) => if self.path_selector.current.search_up(&self.path_selector.search) {
                    self.path_selector.refresh_current()?;
                },
                (None, KeyCode::Enter) => if let Some(entry) = self.path_selector.current.get() {
                    let bump = shell.bump.clone();
                    let bump = bump.borrow();

                    let path = entry.path();
                    let mut path = path.strip_prefix(shell.env.pwd()).unwrap_or(&path);
                    if path == Path::new("") {
                        path = Path::new(".");
                    }

                    let path = path.to_string_lossy();
                    let mut buf = String::with_capacity_in(self.cmd.len(), &bump);
                    write!(&mut buf, "{}", EscapePath(&path))?;

                    self.line.push_str(&buf);
                    self.mode = Mode::Insert;
                },
                (Some('g'), KeyCode::Char('g')) => {
                    self.path_selector.current.move_to_top();
                    self.path_selector.cd(".".as_ref())?;
                },
                _ => ()
            },
            _ => ()
        }

        Ok(Action::Continue)
    }

    pub fn switch_path_selector(&mut self, shell: &Shell) -> anyhow::Result<()> {
        self.path_selector.set_space(self.ui.available_space());
        self.path_selector.cd(shell.env.pwd())?;
        self.path_selector.need_init = true;
        self.cmd.clear();
        self.mode = Mode::PathSelector;
        Ok(())
    }
}

impl Ui {
    fn available_space(&self) -> usize {
        self.rows.checked_sub(self.bottom)
            .unwrap_or_default()
            .into()
    }
}
