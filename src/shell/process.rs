use std::{ io, mem };
use std::rc::Rc;
use std::ffi::OsStr;
use std::cell::RefCell;
use std::process::{ Stdio, ExitStatus };
use anyhow::Context;
use bstr::ByteSlice;
use tokio::process::{ Command, Child as TokioChild, ChildStdout };
use crate::shell::Shell;


pub struct ShellCommand {
    cmd: Command,
    redirect_stdout: bool,
}

pub struct Child {
    inner: Option<TokioChild>,
    morgue: Morgue
}

#[derive(Clone)]
pub struct Morgue {
    queue: Rc<RefCell<Vec<TokioChild>>>
}

impl ShellCommand {
    pub fn new(exe: &[u8]) -> anyhow::Result<ShellCommand> {
        Ok(ShellCommand {
            cmd: Command::new(exe.to_os_str()?),
            redirect_stdout: false
        })
    }

    pub fn push(&mut self, arg: &[u8]) -> anyhow::Result<()> {
        self.cmd.arg(arg.to_os_str()?);
        Ok(())
    }

    pub fn stdin(&mut self, stdio: Stdio) {
        self.cmd.stdin(stdio);
    }

    pub fn stdout(&mut self, stdio: Stdio) {
        self.cmd.stdout(stdio);
        self.redirect_stdout = true;
    }

    pub fn stderr(&mut self, stdio: Stdio) {
        self.cmd.stderr(stdio);
    }

    pub fn is_stdout_available(&self) -> bool {
        !self.redirect_stdout
    }

    pub fn spawn(&mut self, shell: &mut Shell) -> anyhow::Result<Child> {
        self.cmd.envs(&shell.env.0);

        let child = self.cmd.spawn()
            .context("Command execute failed")?;

        Ok(Child {
            inner: Some(child),
            morgue: shell.morgue.clone()
        })
    }
}

impl Child {
    #[cfg(unix)]
    pub fn start_kill(&mut self) -> io::Result<()> {
        let inner = self.inner.as_ref().unwrap();

        if let Some(pid) = inner.id() {
            let ret = unsafe {
                libc::kill(pid as _, libc::SIGTERM)
            };

            if ret == 0 {
                Ok(())
            } else {
                Err(io::Error::last_os_error())
            }
        } else {
            Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "invalid argument: can't kill an exited process",
            ))
        }
    }

    #[cfg(windows)]
    pub fn start_kill(&mut self) -> io::Result<()> {
        let inner = self.inner.as_mut().unwrap();

        // TODO send ctrl-c event
        inner.start_kill()
    }

    pub fn take_stdout(&mut self) -> Option<ChildStdout> {
        self.inner.as_mut()
            .and_then(|inner| inner.stdout.take())
    }

    pub async fn wait(&mut self) -> io::Result<ExitStatus> {
        let inner = self.inner.as_mut().unwrap();
        inner.wait().await
    }
}

impl Drop for Child {
    fn drop(&mut self) {
        let _ = self.start_kill();

        if let Some(child) = self.inner.take() {
            self.morgue.queue.borrow_mut().push(child);
        }
    }
}

impl Default for Morgue {
    fn default() -> Self {
        Morgue {
            queue: Rc::new(RefCell::new(Vec::new()))
        }
    }
}

impl Morgue {
    pub async fn wait(&mut self) -> anyhow::Result<()> {
        let mut queue = self.queue.borrow_mut();
        let queue = queue.drain(..);

        for mut ghost in queue {
            ghost.wait().await?;
        }

        Ok(())
    }
}

#[cfg(unix)]
pub fn to_stdio(stdout: ChildStdout) -> Stdio {
    use std::os::unix::io::{ AsRawFd, FromRawFd };

    let fd = stdout.as_raw_fd();
    mem::forget(stdout);

    unsafe {
        Stdio::from_raw_fd(fd)
    }
}

#[cfg(windows)]
pub fn to_stdio(stdout: ChildStdout) -> Stdio {
    use std::os::windows::io::{ AsRawHandle, FromRawHandle };

    let fd = stdout.as_raw_handle();
    mem::forget(stdout);

    unsafe {
        Stdio::from_raw_handle(fd)
    }
}
