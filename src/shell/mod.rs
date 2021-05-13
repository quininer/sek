pub mod env;
pub mod parser;
pub mod config;
pub mod process;
pub mod execute;
pub mod builtin;

use std::io;
use std::rc::Rc;
use std::cell::RefCell;
use anyhow::Context;
use bumpalo::Bump;
use bumpalo::collections::String;
use tokio::signal;
use tokio_stream::StreamExt;
use scopeguard::defer;
use crossterm::{ execute, queue, style, terminal };
use crossterm::event::EventStream;
use crate::Global;
use crate::shell::env::Env;
use crate::shell::config::Theme;
use crate::shell::parser::type_::Command;
use crate::shell::process::{ ShellCommand, Morgue };
use crate::editor::Editor;
use crate::editor::render::{ render, report };
use crate::util::FmtDebug;
pub use crate::shell::parser::colour;


pub struct Shell {
    pub bump: Rc<RefCell<Bump>>,
    pub term: io::Stdout,
    pub env: Env,
    pub theme: Theme,
    pub morgue: Morgue,
    pub last_status: bool
}

pub enum Action {
    Continue,
    Execute,
    Stop
}

impl Shell {
    pub fn new(_global: &Global) -> anyhow::Result<Self> {
        Ok(Shell {
            bump: Rc::new(RefCell::new(Bump::new())),
            term: io::stdout(),
            env: Env::new()?,
            theme: Theme::default(),
            morgue: Morgue::default(),
            last_status: true
        })
    }

    pub async fn start(&mut self, global: &Global) -> anyhow::Result<()> {
        let mut reader = EventStream::new();

        let mut editor = Editor::new(global)?;

        render(&editor, self, false)?;

        while let Some(event) = reader.next().await {
            self.bump.borrow_mut().reset();

            let action = editor.step(self, event?).await?;

            let execute = match action {
                Action::Continue => false,
                Action::Execute => {
                    let bump = self.bump.clone();
                    let bump = bump.borrow();

                    let mut line = String::with_capacity_in(editor.line.len(), &bump);
                    editor.line.read_into(&mut line);
                    let line = line.trim_end().trim_end_matches(';');

                    execute!(&self.term, style::Print("\r\n"))?;

                    if !line.is_empty() {
                        match parser::parse_in(&bump, line) {
                            Ok(cmd) => self.execute(line, cmd).await?,
                            Err(err) => report(&editor, self, line, err)?
                        }

                        editor.line.history.push(line);
                    }

                    true
                },
                Action::Stop => break
            };

            if execute {
                editor.line.clear();
            }

            render(&editor, self, execute)?;
        }

        Ok(())
    }

    pub async fn execute<'g>(&mut self, line: &str, cmd: Command<'_>) -> anyhow::Result<()> {
        terminal::disable_raw_mode()?;

        defer!{
            let _ = terminal::enable_raw_mode();
        };

        tokio::select!{
            ret = shell_execute(self, line, &cmd) => match ret {
                Ok(status) => self.last_status = status,
                Err(err) => {
                    queue!(
                        &self.term,
                        style::Print(concat!(env!("CARGO_PKG_NAME"), ": ")),
                        style::Print(FmtDebug(&err)),
                        style::Print("\r\n")
                    )?;
                    self.last_status = false;
                }
            },
            ret = signal::ctrl_c() => {
                ret?;
                self.last_status = false;
            }
        };

        self.morgue.wait().await?;

        Ok(())
    }
}

async fn shell_execute(shell: &mut Shell, line: &str, cmd: &Command<'_>)
    -> anyhow::Result<bool>
{
    if builtin::try_command(shell, line, cmd).await? {
        return Ok(true);
    }

    let mut shell_cmd = None;
    let mut cmd_new = |osstr: &[u8]| {
        shell_cmd = Some(ShellCommand::new(osstr)?);
        Ok(())
    };
    cmd.exe.eval(shell, line, &mut cmd_new)?;

    let mut shell_cmd = shell_cmd.context("the expanded command was empty")?;

    let mut push = |osstr: &[u8]| shell_cmd.push(osstr);

    for arg in cmd.args.iter() {
        arg.eval(shell, line, &mut push).await?;
    }

    for stdio in cmd.redirect.iter() {
        stdio.redirect(shell, line, &mut shell_cmd).await?;
    }

    let status = if let Some(chain) = cmd.chain.as_ref() {
        chain.exec(shell, line, shell_cmd, None).await?
    } else {
        shell_cmd.spawn(shell)?.wait().await?
    };

    Ok(status.success())
}
