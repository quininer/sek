pub mod env;
pub mod parser;
pub mod config;
pub mod process;

use std::io;
use std::rc::Rc;
use std::cell::RefCell;
use anyhow::Context;
use bumpalo::Bump;
use tokio_stream::StreamExt;
use directories::UserDirs;
use crossterm::{ execute, queue, style, terminal };
use crossterm::event::EventStream;
use crate::Global;
use crate::shell::env::Env;
use crate::shell::config::Theme;
use crate::shell::process::Morgue;
use crate::editor::Editor;


pub struct Shell {
    pub bump: Rc<RefCell<Bump>>,
    pub term: io::Stdout,
    pub env: Env,
    pub theme: Theme,
    pub morgue: Morgue,
    pub userdir: UserDirs,
}

pub enum Action {
    Continue,
    NewLine,
    Execute,
    Stop
}

impl Shell {
    pub fn new(_global: &Global) -> anyhow::Result<Self> {
        Ok(Shell {
            bump: Rc::new(RefCell::new(Bump::new())),
            term: io::stdout(),
            env: Env::default(),
            theme: Theme::default(),
            morgue: Morgue::default(),
            userdir: UserDirs::new()
                .context("Unable to retrieve user path from system")?
        })
    }

    pub async fn start<'g>(&mut self, global: &'g Global) -> anyhow::Result<()> {
        let mut reader = EventStream::new();

        let mut editor = Editor::new(global)?;

        loop {
            self.bump.borrow_mut().reset();

            if let Some(event) = reader.next().await {
                let action = editor.step(self, event?).await?;

                match action {
                    Action::Continue => {
                        let mut term = self.term.lock();
                        // render
                    },
                    Action::NewLine => {
                        let mut term = self.term.lock();
                        queue!(term, style::Print("\n"))?;
                        // render
                    },
                    Action::Execute => {
                        // execute
                        // history
                        // render
                    },
                    Action::Stop => break
                }
            }
        }

        Ok(())
    }
}
