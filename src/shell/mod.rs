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
use unicode_width::UnicodeWidthStr;
use crossterm::{ execute, queue, style, terminal };
use crossterm::event::EventStream;
use crate::shell::parser::type_::Command;
use crate::shell::process::{ ShellCommand, Morgue };
use crate::editor::Editor;
use crate::editor::render::{ render, report, render_line };
use crate::util::FmtDebug;
pub use crate::shell::env::Env;
pub use crate::shell::config::{ Theme, AliasMap };


pub struct Shell {
    pub bump: Rc<RefCell<Bump>>,
    pub term: io::Stdout,
    pub env: Env,
    pub theme: Theme,
    pub alias: AliasMap,
    pub morgue: Morgue,
    pub arg_max: usize,
    pub last_status: bool,
    pub is_execute: bool
}

pub enum Action {
    Continue,
    Execute,
    Stop
}

impl Shell {
    pub async fn start(&mut self) -> anyhow::Result<()> {
        let mut reader = EventStream::new();
        let mut editor = Editor::new()?;

        render(&mut editor, self)?;

        while let Some(event) = reader.next().await {
            self.bump.borrow_mut().reset();

            let action = match editor.step(self, event?).await {
                Ok(action) => action,
                Err(err) => {
                    // TODO better error handle

                    queue!(&self.term,
                        style::Print("\r\n"),
                        style::Print(format_args!("{:?}", err)),
                        style::Print("\r\n")
                    )?;

                    Action::Continue
                }
            };
            self.is_execute = false;

            match action {
                Action::Continue => (),
                Action::Execute => {
                    let bump = self.bump.clone();
                    let bump = bump.borrow();

                    let mut line = String::with_capacity_in(editor.line.len() + 16, &bump);
                    editor.line.read_into(&mut line);
                    self.alias.replace(&mut line);
                    let line2 = line.trim_end();

                    if !line2.is_empty() {
                        match parser::parse_in(&bump, line2) {
                            Ok(cmd) => {
                                execute!(&self.term, style::Print("\r\n"))?;
                                self.is_execute = true;
                                self.execute(line2, cmd).await?;
                                editor.line.history.push(line2);
                                editor.line.clear();
                            },
                            Err(err) => {
                                let width = line.width() as u16;
                                render_line(&bump, &mut editor, self, &mut line, width)?;
                                queue!(&self.term, style::Print("\r\n"))?;
                                report(&mut editor, self, &line, err)?;
                                self.last_status = false;
                            }
                        }
                    } else {
                        queue!(&self.term, style::Print("\r\n"))?;
                        editor.line.clear();
                    }
                },
                Action::Stop => break
            };

            render(&mut editor, self)?;
        }

        Ok(())
    }

    pub async fn execute(&mut self, line: &str, cmd: Command<'_>) -> anyhow::Result<()> {
        terminal::disable_raw_mode()?;

        defer!{
            let _ = terminal::enable_raw_mode();
        };

        let join = tokio::spawn(async {
            loop {
                if let Err(err) = signal::ctrl_c().await {
                    return err;
                }
            }
        });
        let mut join = scopeguard::guard(join, |join| join.abort());
        let join = &mut *join;

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
            err = join => return Err(err?.into())
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
