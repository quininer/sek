use std::io;
use std::rc::Rc;
use std::cell::RefCell;
use std::process::{ ExitStatus, Stdio };
use tokio::process::{ self, Command };
use anyhow::Context;
use bstr::ByteSlice;
use super::Shell;

#[cfg(unix)]
use std::cell::Cell;


#[derive(Debug)]
pub struct ShellCommand {
    cmd: Command,
    redirect_stdout: bool,
}

pub struct Child {
    child: Option<process::Child>,
    morgue: Morgue,
    new_group: bool,
}

#[derive(Clone)]
pub struct Morgue {
    pgid: libc::pid_t,
    queue: Rc<RefCell<Vec<(bool, process::Child)>>>
}

#[derive(Default, Clone, Debug)]
pub struct Leader {
    pgid: Rc<Cell<Option<libc::pid_t>>>
}

impl ShellCommand {
    pub fn new(exe: &[u8]) -> anyhow::Result<ShellCommand> {
        let exe = exe.to_os_str()?;
        Ok(ShellCommand {
            cmd: Command::new(exe),
            redirect_stdout: false,
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

    pub fn spawn(&mut self, shell: &Shell, leader: Option<&Leader>) -> anyhow::Result<Child> {
        let env = shell.env.borrow();

        self.cmd
            .current_dir(env.pwd())
            .envs(env.map.iter().map(|(k, v)| (k.as_os_str(), v.as_os_str())));

        shell.morgue.spawn(&mut self.cmd, leader)
            .with_context(|| format!("spawn failed: {:?}", self.cmd.as_std().get_program()))
    }
}

impl Child {
    pub fn stdout(&mut self) -> &mut Option<process::ChildStdout> {
        &mut self.child.as_mut().unwrap().stdout
    }

    pub fn stderr(&mut self) -> &mut Option<process::ChildStderr> {
        &mut self.child.as_mut().unwrap().stderr
    }    
    
    pub async fn wait(&mut self) -> io::Result<ExitStatus> {
        let mut child = self.child.take().unwrap();
        child.wait().await
    }
}

impl Drop for Child {
    fn drop(&mut self) {
        if let Some(mut child) = self.child.take()
            && let Ok(status) = child.try_wait()
            && status.is_none()
        {
            self.morgue.queue.borrow_mut().push((self.new_group, child));
        }
    }
}

impl Default for Morgue {
    fn default() -> Self {
        Morgue {
            #[cfg(unix)]
            pgid: unsafe {
                libc::getpid()
            },
            queue: Default::default()
        }
    }
}

impl Morgue {
    pub fn spawn(&self, cmd: &mut Command, leader: Option<&Leader>) -> io::Result<Child> {
        #[cfg(unix)] {
            use std::os::unix::process::CommandExt;
            use crate::util::reset_signal_ignore;

            let pgid = leader
                .map(|leader| leader.pgid.get().unwrap_or_default())
                .unwrap_or(self.pgid);

            cmd.as_std_mut().process_group(pgid);
            let new_group = pgid == 0;

            unsafe {
                cmd.as_std_mut().pre_exec(move || {
                    if new_group {
                        libc::tcsetpgrp(libc::STDIN_FILENO, libc::getpid());
                    }

                    reset_signal_ignore();

                    Ok(())
                });
            }
        }

        let child = cmd.spawn();

        #[cfg(unix)] {
            if let Some(leader) = leader
                && leader.pgid.get().is_none()
            {
                let pgid = child
                    .as_ref()
                    .map(|child| child.id().unwrap())
                    .unwrap_or_default();
                leader.pgid.set(Some(pgid as libc::pid_t));
            }
        }

        Ok(Child {
            child: Some(child?),
            morgue: self.clone(),
            new_group: leader.is_some()
        })
    }

    #[allow(clippy::await_holding_refcell_ref)]    
    pub async fn wait(&self, leader: &Leader) -> io::Result<()> {
        let mut queue = self.queue.borrow_mut();

        #[cfg(unix)]
        if !queue.is_empty()
            && let Some(pgid) = leader.pgid.get()
        {
            debug_assert_ne!(self.pgid, pgid);

            unsafe {
                libc::killpg(pgid, libc::SIGKILL);
            }
        }

        for (new_group, ghost) in queue.iter_mut() {
            let skip = *new_group && cfg!(unix);

            if !skip {
                let _ = ghost.start_kill();
            }
        }

        for (_, mut ghost) in queue.drain(..) {
            ghost.wait().await?;
        }

        #[cfg(unix)] {
            use std::os::fd::AsRawFd;

            if leader.pgid.get().is_some() {
                unsafe {
                    if libc::tcsetpgrp(io::stdin().as_raw_fd(), self.pgid) != 0 {
                        return Err(io::Error::last_os_error());
                    }
                }
            }
        }

        Ok(())        
    }
}
