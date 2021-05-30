pub mod buffer;
pub mod history;
pub mod render;
pub mod command;
pub mod path_selector;


use crossterm::event::{ Event, KeyEvent, KeyCode, KeyModifiers as KM };
use crate::shell::{ Shell, Action };
use crate::editor::buffer::Buffer;
use crate::editor::path_selector::PathSelector;
use crate::editor::command::execute_command;

pub struct Editor {
    pub line: Buffer,
    pub cmd: Buffer,
    path_selector: PathSelector,
    ready: Option<char>,
    mode: Mode,
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

#[derive(Clone, Copy, Debug)]
enum Mode {
    Insert,
    Normal,
    Visual,
    PathSelector
}

impl Editor {
    pub fn new() -> anyhow::Result<Self> {
        let (columns, rows) = crossterm::terminal::size()?;
        Ok(Editor {
            line: Buffer::default(),
            cmd: Buffer::default(),
            path_selector: PathSelector::new(std::cmp::max(rows / 2, 1) as usize),
            ready: None,
            mode: Mode::Insert,
            ui: Ui {
                columns, rows,
                ..Default::default()
            }
        })
    }

    pub async fn step(&mut self, shell: &mut Shell, event: Event) -> anyhow::Result<Action> {
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
                KeyCode::Tab => {
                    // TODO
                    // completion daemon

                    self.path_selector.set_space(self.ui.available_space());
                    self.path_selector.cd(shell.env.pwd())?;
                    self.path_selector.need_init = true;
                    self.mode = Mode::PathSelector;
                },
                KeyCode::Enter => return Ok(Action::Execute),
                _ => ()
            },
            // Normal Command
            (Mode::Normal, Event::Key(KeyEvent { modifiers, code }))
                if modifiers.contains(KM::SHIFT & KM::NONE)
                    && (code == KeyCode::Char(':') || code == KeyCode::Char(';'))
            => {
                self.cmd.push(':');
            },
            // Noraml Filter
            (Mode::Normal, Event::Key(KeyEvent { modifiers, code }))
                if modifiers == KM::NONE && code == KeyCode::Char('/')
            => {
                self.cmd.push('/');
            },
            // Normal Command input
            (Mode::Normal, Event::Key(KeyEvent { modifiers, code }))
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
                KeyCode::Enter => return execute_command(self, shell),
                _ => ()
            },
            // Noraml
            (Mode::Normal, Event::Key(KeyEvent { modifiers, code }))
                if modifiers == KM::NONE && self.cmd.is_empty()
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
                (None, KeyCode::Char('d')) => self.ready = Some('d'),
                (None, KeyCode::Char('z')) => self.ready = Some('z'),
                (None, KeyCode::Char('0')) => self.line.move_head(),
                (None, KeyCode::Char('$')) => self.line.move_end(),
                (None, KeyCode::Backspace) => self.line.move_left(),
                (None, KeyCode::Left) => self.line.move_left(),
                (None, KeyCode::Right) => self.line.move_right(),
                (None, KeyCode::Esc) => self.ready = None,
                (Some('d'), KeyCode::Char('d')) => self.line.clear(),
                (Some('z'), KeyCode::Char('c')) => shell.env.cd("..".as_ref())?,
                (Some('z'), KeyCode::Char('j')) => shell.env.go_back()?,
                (Some('z'), KeyCode::Char('h')) => shell.env.go_home()?,
                _ => ()
            },
            (Mode::PathSelector, Event::Key(KeyEvent { modifiers, code }))
                if (modifiers == KM::CONTROL && code == KeyCode::Char('c'))
                    || (modifiers == KM::NONE && code == KeyCode::Esc)
            => {
                self.mode = Mode::Insert;
            },
            (Mode::PathSelector, Event::Key(KeyEvent { modifiers, code }))
                if modifiers.contains(KM::SHIFT & KM::NONE)
            => match code {
                KeyCode::Char('h') => self.path_selector.left()?,
                KeyCode::Char('l') => self.path_selector.right()?,
                KeyCode::Char('j') => self.path_selector.down()?,
                KeyCode::Char('k') => self.path_selector.up()?,
                KeyCode::Enter => {
                    let path = self.path_selector.path();
                    let path = path.strip_prefix(shell.env.pwd()).unwrap_or(path);
                    self.line.insert_path(path);
                    self.mode = Mode::Insert;
                },
                _ => ()
            },
            _ => ()
        }

        Ok(Action::Continue)
    }
}

impl Ui {
    fn available_space(&self) -> usize {
        self.rows.checked_sub(self.bottom)
            .unwrap_or_default()
            .into()
    }
}
