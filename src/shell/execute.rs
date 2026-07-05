pub mod eval;
pub mod external;
pub mod builtin;

use std::process::{ Stdio, ExitStatus };
use tokio::process;
use bstr::BString;
use super::{ syntax, Shell };
use external::Leader;


pub async fn execute(shell: &Shell, input: &str, cmd: syntax::Command)
    -> anyhow::Result<Status>
{
    let leader = Leader::default();
    let result = eval::execute(shell, input, cmd, &leader).await;
    shell.morgue.wait(&leader).await?;
    result
}

pub struct Command {
    kind: CommandKind,
    leader: Option<Leader>,
}

pub enum CommandKind {
    Builtin(builtin::BuiltinCommand, Vec<BString>),
    External(external::ShellCommand),
}

pub enum Child<'a> {
    BuiltIn {
        future: builtin::BuiltinFuture<'a>,
        stdout: Option<process::ChildStdout>,
        stderr: Option<process::ChildStderr>,
    },
    External(external::Child),
}

#[derive(Debug)]
pub enum Status {
    BuiltIn(bool),
    Process(ExitStatus),
}

impl Command {
    pub fn new(shell: &Shell, exe: &[u8], leader: Option<Leader>)
        -> anyhow::Result<Command>
    {
        let config = shell.config.borrow();

        let kind = if let Some(cmd) = builtin::builtin_command(exe) {
            CommandKind::Builtin(cmd, Vec::new())
        } else if let Some((exe, args)) = config.alias.get(exe) {
            let mut cmd = external::ShellCommand::new(exe.as_bytes())?;
            for arg in args {
                cmd.push(arg.as_bytes())?;
            }
            CommandKind::External(cmd)
        } else {
            CommandKind::External(external::ShellCommand::new(exe)?)
        };

        Ok(Command { kind, leader })
    }

    pub fn push(&mut self, arg: &[u8]) -> anyhow::Result<()> {
        match &mut self.kind {
            CommandKind::Builtin(_, args) => {
                args.push(arg.into());
                Ok(())
            },
            CommandKind::External(cmd) => cmd.push(arg)
        }
    }

    pub fn stdin(&mut self, stdio: Stdio) {
        match &mut self.kind {
            CommandKind::Builtin(..) => (),
            CommandKind::External(cmd) => cmd.stdin(stdio)
        }
    }

    pub fn stdout(&mut self, stdio: Stdio) {
        match &mut self.kind {
            CommandKind::Builtin(..) => (),
            CommandKind::External(cmd) => cmd.stdout(stdio)
        }        
    }

    pub fn stderr(&mut self, stdio: Stdio) {
        match &mut self.kind {
            CommandKind::Builtin(..) => (),
            CommandKind::External(cmd) => cmd.stderr(stdio)
        }
    }

    pub fn is_stdout_available(&self) -> bool {
        match &self.kind {
            CommandKind::Builtin(..) => false,
            CommandKind::External(cmd) => cmd.is_stdout_available()
        }
    }

    pub fn leader(&self) -> Option<Leader> {
        self.leader.clone()
    }

    pub fn spawn<'a>(&'a mut self, shell: &'a Shell)
        -> anyhow::Result<Child<'a>>
    {
        match &mut self.kind {
            CommandKind::Builtin(cmd, args) => Ok(Child::BuiltIn {
                future: cmd(shell, args),
                stdout: None,
                stderr: None,
            }),
            CommandKind::External(cmd) =>
                cmd.spawn(shell, self.leader.as_ref()).map(Child::External),
        }
    }
}

impl Child<'_> {
    pub fn stdout(&mut self) -> &mut Option<process::ChildStdout> {
        match self {
            Child::BuiltIn { stdout, .. } => stdout,
            Child::External(child) => child.stdout()
        }
    }

    pub fn stderr(&mut self) -> &mut Option<process::ChildStderr> {
        match self {
            Child::BuiltIn { stderr, .. } => stderr,
            Child::External(child) => child.stderr()
        }        
    }

    pub async fn wait(&mut self) -> anyhow::Result<Status> {
        use tokio::signal::ctrl_c;
        use crate::util::{ Select, Either };

        match self {
            Child::BuiltIn { future, .. } => {
                match Select::new(future.as_mut(), ctrl_c()).await {
                    Either::Left(result) => Ok(Status::BuiltIn(result?)),
                    Either::Right(result) => {
                        result?;
                        anyhow::bail!("built-in command cancel by ctrl-c")
                    }
                }
            },
            Child::External(child) => Ok(Status::Process(child.wait().await?))
        }
    }
}

impl Status {
    pub fn success(&self) -> bool {
        match self {
            Status::BuiltIn(v) => *v,
            Status::Process(status) => status.success()
        }
    }

    pub fn code(&self) -> i32 {
        match self {
            Status::BuiltIn(v) => !v as i32,
            Status::Process(status) => status.code().unwrap_or_default()
        }
    }

    #[cfg(unix)]
    pub fn signal(&self) -> Option<libc::c_int> {
        use std::os::unix::process::ExitStatusExt;

        match self {
            Status::BuiltIn(_) => None,
            Status::Process(status) => status.signal()
        }
    }
}
