pub mod buffer;
pub mod history;
pub mod render;


use std::cell::Cell;
use crossterm::event::{ Event, KeyEvent, KeyCode, KeyModifiers as KM };
use crate::Global;
use crate::shell::{ Shell, Action };
use crate::editor::buffer::Buffer;

pub struct Editor<'g> {
    pub global: &'g Global,
    pub line: Buffer,
    pub cmd: Buffer,
    ready: Option<char>,
    cursor_line: Cell<u16>,
    state: State
}

#[derive(Clone, Copy, Debug)]
enum State {
    Edit,
    Command,
    Selection,
    Completion
}

impl<'g> Editor<'g> {
    pub fn new(global: &'g Global) -> anyhow::Result<Self> {
        Ok(Editor {
            global,
            line: Buffer::default(),
            cmd: Buffer::default(),
            ready: None,
            cursor_line: Cell::new(0),
            state: State::Edit
        })
    }

    pub async fn step(&mut self, shell: &mut Shell, event: Event) -> anyhow::Result<Action> {
        match (self.state, event) {
            // resize
            (_, Event::Resize(size, _)) => self.global.columns.set(size),
            // edit to command
            (State::Edit, Event::Key(KeyEvent { modifiers, code }))
                if (modifiers == KM::CONTROL && code == KeyCode::Char('c'))
                    || (modifiers == KM::NONE && code == KeyCode::Esc)
                    || (modifiers == KM::ALT && code == KeyCode::Char(' '))
            => {
                self.state = State::Command;
            },
            // edit quit
            (State::Edit, Event::Key(KeyEvent { modifiers, code }))
                if modifiers == KM::CONTROL && code == KeyCode::Char('d')
                    && self.line.is_empty()
            => return Ok(Action::Stop),
            // edit input
            (State::Edit, Event::Key(KeyEvent { modifiers, code }))
                if modifiers.contains(KM::SHIFT & KM::NONE)
            => match code {
                KeyCode::Char('\r') => (),
                KeyCode::Char(c) => {
                    self.line.push(c);

                    // TODO cursor
                },
                KeyCode::Backspace => self.line.backspace(),
                KeyCode::Delete => self.line.delete(),
                KeyCode::Left => self.line.move_left(),
                KeyCode::Right => self.line.move_right(),
                KeyCode::Enter => return Ok(Action::Execute),
                _ => ()
            },
            // command to command input
            (State::Command, Event::Key(KeyEvent { modifiers, code }))
                if modifiers.contains(KM::SHIFT & KM::NONE)
                    && (code == KeyCode::Char(':') || code == KeyCode::Char(';'))
            => {
                self.cmd.push(':');
            },
            // command to command filter
            (State::Command, Event::Key(KeyEvent { modifiers, code }))
                if modifiers == KM::NONE && code == KeyCode::Char('/')
            => {
                self.cmd.push('/');
            },
            // command input
            (State::Command, Event::Key(KeyEvent { modifiers, code }))
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
                KeyCode::Enter => {
                    // TODO
                },
                _ => ()
            },
            // command selection
            (State::Command, Event::Key(KeyEvent { modifiers, code }))
                if modifiers == KM::NONE && self.cmd.is_empty()
            => match (self.ready.take(), code) {
                // command to edit
                (None, KeyCode::Char('i')) => self.state = State::Edit,
                (None, KeyCode::Char('a')) => {
                    self.line.move_right();
                    self.state = State::Edit;
                },
                (None, KeyCode::Char('h')) => self.line.move_left(),
                (None, KeyCode::Char('l')) => self.line.move_right(),
                (None, KeyCode::Char('j')) => self.line.history.down(),
                (None, KeyCode::Char('k')) => self.line.history.up(),
                (None, KeyCode::Char('d')) => self.ready = Some('d'),
                (None, KeyCode::Char('z')) => self.ready = Some('z'),
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
            _ => ()
        }

        Ok(Action::Continue)
    }

}
