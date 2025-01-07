pub mod line;
pub mod ui;

use std::io;
use std::ops::{ Range, ControlFlow };
use crossterm::event::{ Event, KeyCode, KeyEvent, KeyModifiers as KM };
use line::EditableLine;
use crate::ui::render::Renderer;
use crate::ui::layout;

pub struct Editor {
    ui: ui::Editor,
    mode: Mode,
    line: EditableLine,
    line_cursor: Range<usize>,
    command: EditableLine,
    command_cursor: Range<usize>,
}

#[derive(Clone, Copy, Debug)]
pub enum Mode {
    Insert,
    Normal,
    Visual,
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
            line: EditableLine::default(),
            line_cursor: 0..0,
            command: EditableLine::default(),
            command_cursor: 0..0
        })
    }

    pub fn init_to<W>(&self, renderer: &mut Renderer<Self, W, anyhow::Error>) {
        renderer.insert::<ui::Prompt>(self.ui.prompt);
        renderer.insert::<ui::InsertLine>(self.ui.insert_line);
        renderer.insert::<ui::CommandLine>(self.ui.command_line);
        renderer.insert::<ui::Tips>(self.ui.tips);
    }

    pub fn step(&mut self, event: Event)
        -> anyhow::Result<ControlFlow<()>>
    {
        match (self.mode, event) {
            // Quit
            (Mode::Insert, Event::Key(KeyEvent { modifiers, code, .. }))
                if modifiers == KM::CONTROL && code == KeyCode::Char('d')
                    && self.line.is_empty()
            => return Ok(ControlFlow::Break(())),
            // Insert
            (Mode::Insert | Mode::Normal, Event::Key(KeyEvent { modifiers, code, .. }))
                if modifiers.contains(KM::SHIFT & KM::NONE)
            => {
                let end = match self.mode {
                    Mode::Insert => &mut self.line_cursor.end,
                    Mode::Normal => &mut self.command_cursor.end,
                    _ => unreachable!()
                };

                match code {
                    KeyCode::Char('\r') => (),
                    KeyCode::Char(c) => self.line.push(end, c),
                    KeyCode::Backspace => self.line.backspace(end),
                    KeyCode::Delete => self.line.delete(*end),
                    KeyCode::Left => self.line.move_left(end),
                    KeyCode::Right => self.line.move_right(end),
                    KeyCode::Home => self.line.move_head(end),
                    KeyCode::End => self.line.move_end(end),
                    _ => ()
                }
            },
            _ => ()
        }

        if matches!(self.mode, Mode::Insert) {
            self.line_cursor.start = self.line_cursor.end;
        }

        Ok(ControlFlow::Continue(()))
    }

    pub fn render<GetWriter, Writer>(
        &self,
        renderer: &mut Renderer<Self, GetWriter, anyhow::Error>,
    ) -> anyhow::Result<()>
    where
        GetWriter: Fn() -> Writer,
        Writer: io::Write
    {
        renderer.render(self)
    }
}
