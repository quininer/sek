use std::{ io, mem };
use std::rc::Rc;
use std::ffi::OsStr;
use std::cell::RefCell;
use std::process::{ Stdio, ExitStatus };
use tokio::process::{ Command, Child as TokioChild, ChildStdout };
use anyhow::Context;
use crate::shell::Shell;


pub struct ShellCommand {
    cmd: Command,
    stdin: Option<Stdio>,
    stdout: Option<Stdio>,
    stderr: Option<Stdio>
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
    pub fn new(exe: &OsStr) -> ShellCommand {
        ShellCommand {
            cmd: Command::new(exe),
            stdin: None,
            stdout: None,
            stderr: None
        }
    }

    pub fn push(&mut self, arg: &OsStr) {
        self.cmd.arg(arg);
    }

    pub fn stdin(&mut self, stdio: Stdio) {
        self.stdin = Some(stdio);
    }

    pub fn stdout(&mut self, stdio: Stdio) {
        self.stdout = Some(stdio);
    }

    pub fn stderr(&mut self, stdio: Stdio) {
        self.stderr = Some(stdio);
    }

    pub fn is_stdout_available(&self) -> bool {
        self.stdout.is_none()
    }

    pub fn spawn(&mut self, shell: &mut Shell) -> anyhow::Result<Child> {
        self.cmd.envs(&shell.env.0);

        if let Some(stdio) = self.stdin.take() {
            self.cmd.stdin(stdio);
        }

        if let Some(stdio) = self.stdout.take() {
            self.cmd.stdout(stdio);
        }

        if let Some(stdio) = self.stderr.take() {
            self.cmd.stderr(stdio);
        }

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
