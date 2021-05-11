pub mod env;
pub mod parser;
pub mod config;
pub mod process;
pub mod execute;

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
pub use crate::shell::parser::colour;


pub struct Shell {
    pub bump: Rc<RefCell<Bump>>,
    pub term: io::Stdout,
    pub env: Env,
    pub theme: Theme,
    pub morgue: Morgue,
    pub userdir: UserDirs,
    pub cmdbuf: String,
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
                .context("Unable to retrieve user path from system")?,
            cmdbuf: String::new()
        })
    }

    pub async fn start<'g>(&mut self, global: &'g Global) -> anyhow::Result<()> {
        let mut reader = EventStream::new();

        let mut editor = Editor::new(global)?;

        while let Some(event) = reader.next().await {
            self.bump.borrow_mut().reset();
            self.cmdbuf.clear();

            let action = editor.step(self, event?).await?;

            match action {
                Action::Continue => (),
                Action::NewLine => {
                    let mut term = self.term.lock();
                    queue!(term, style::Print("\n"))?;
                },
                Action::Execute => {
                    // execute
                    // history
                },
                Action::Stop => break
            }

            // render
        }

        Ok(())
    }
}
