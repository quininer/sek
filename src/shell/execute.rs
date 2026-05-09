pub mod eval;
pub mod external;
pub mod builtin;

use std::process::{ Stdio, ExitStatus };
use tokio::process;
use bstr::BString;
use super::{ syntax, Shell };


pub async fn execute(shell: &Shell, input: &str, cmd: syntax::Command)
    -> anyhow::Result<Status>
{
    eval::execute(shell, input, cmd).await
}

pub enum Command {
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

pub enum Status {
    BuiltIn(bool),
    Process(ExitStatus),
}

impl Command {
    pub fn new(exe: &[u8]) -> anyhow::Result<Command> {
        if let Some(cmd) = builtin::builtin_command(exe) {
            Ok(Command::Builtin(cmd, Vec::new()))
        } else {
            Ok(Command::External(external::ShellCommand::new(exe)?))
        }
    }

    pub fn push(&mut self, arg: &[u8]) -> anyhow::Result<()> {
        match self {
            Command::Builtin(_, args) => {
                args.push(arg.into());
                Ok(())
            },
            Command::External(cmd) => cmd.push(arg)
        }
    }

    pub fn stdin(&mut self, stdio: Stdio) {
        match self {
            Command::Builtin(..) => (),
            Command::External(cmd) => cmd.stdin(stdio)
        }
    }

    pub fn stdout(&mut self, stdio: Stdio) {
        match self {
            Command::Builtin(..) => (),
            Command::External(cmd) => cmd.stdout(stdio)
        }        
    }

    pub fn stderr(&mut self, stdio: Stdio) {
        match self {
            Command::Builtin(..) => (),
            Command::External(cmd) => cmd.stderr(stdio)
        }
    }

    pub fn is_stdout_available(&self) -> bool {
        match self {
            Command::Builtin(..) => false,
            Command::External(cmd) => cmd.is_stdout_available()
        }
    }    

    pub fn spawn<'a>(&'a mut self, shell: &'a Shell) -> anyhow::Result<Child<'a>> {
        match self {
            Command::Builtin(cmd, args) => Ok(Child::BuiltIn {
                future: cmd(shell, args),
                stdout: None,
                stderr: None,
            }),
            Command::External(cmd) => cmd.spawn(shell).map(Child::External),
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

    #[cfg(unix)]
    pub fn signal(&self) -> Option<libc::c_int> {
        use std::os::unix::process::ExitStatusExt;

        match self {
            Status::BuiltIn(_) => None,
            Status::Process(status) => status.signal()
        }
    }
}
