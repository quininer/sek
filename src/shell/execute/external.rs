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
    morgue: Morgue
}

#[derive(Clone)]
pub struct Morgue {
    #[cfg(unix)]
    jobs_pgid: Cell<Option<libc::pid_t>>,
    queue: Rc<RefCell<Vec<process::Child>>>
}

#[derive(Clone, Copy, Debug)]
pub enum Cause {
    Wait,
    CtrcC,
    Error,
}

impl ShellCommand {
    pub fn new(exe: &[u8]) -> anyhow::Result<ShellCommand> {
        let exe = exe.to_os_str()?;
        Ok(ShellCommand {
            cmd: Command::new(exe),
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

    pub fn spawn(&mut self, shell: &Shell) -> anyhow::Result<Child> {
        let env = shell.env.borrow();

        self.cmd
            .current_dir(env.pwd())
            .envs(env.map.iter().map(|(k, v)| (k.as_os_str(), v.as_os_str())));

        match shell.morgue.spawn(&mut self.cmd) {
            Ok(child) => Ok(Child {
                child: Some(child),
                morgue: shell.morgue.clone()
            }),
            Err(err) => Err(err)
                .with_context(|| format!("spawn failed: {:?}", self.cmd.as_std().get_program()))
        }
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
        if let Some(child) = self.child.take() {
            self.morgue.queue.borrow_mut().push(child);
        }
    }
}

impl Default for Morgue {
    fn default() -> Self {
        Morgue {
            #[cfg(unix)]
            jobs_pgid: Cell::new(None),
            queue: Default::default()
        }
    }
}

impl Morgue {
    pub fn spawn(&self, cmd: &mut Command) -> io::Result<process::Child> {
        #[cfg(unix)] {
            use std::os::unix::process::CommandExt;

            let pgid = self.jobs_pgid.get().unwrap_or_default();
            cmd.as_std_mut().process_group(pgid);
        }

        let child = cmd.spawn()?;

        #[cfg(unix)] {
            use std::os::fd::AsRawFd;

            if self.jobs_pgid.get().is_none() {
                let pgid = child.id().unwrap() as libc::pid_t;

                unsafe {
                    if libc::tcsetpgrp(io::stdin().as_raw_fd(), pgid) != 0 {
                        return Err(io::Error::last_os_error());
                    }
                }
                
                self.jobs_pgid.set(Some(pgid));
            }
        }

        Ok(child)
    }

    #[allow(clippy::await_holding_refcell_ref, unused_variables)]    
    pub async fn wait(&self, cause: Cause) -> io::Result<()> {
        let mut queue = self.queue.borrow_mut();

        queue.retain_mut(|child| !matches!(child.try_wait(), Ok(Some(_))));

        #[cfg(unix)] {
            let maybe_signal = match cause {
                Cause::Wait => None,
                Cause::CtrcC => Some(libc::SIGINT),
                Cause::Error => Some(libc::SIGKILL)
            };

            if let Some(signal) = maybe_signal {
                if let Some(pgid) = self.jobs_pgid.get() {
                    unsafe {
                        libc::killpg(pgid, signal);
                    }
                } else {
                    for ghost in queue.as_mut_slice() {
                        if signal == libc::SIGKILL {
                            // Use pidfd kill
                            let _ = ghost.start_kill();
                        } else {
                            let pid = ghost.id().unwrap() as libc::pid_t;

                            // TODO replace it with pidfd
                            unsafe {
                                libc::kill(pid, signal);
                            }                        
                        }
                    }
                }
            }
        }

        for mut ghost in queue.drain(..) {
            ghost.wait().await?;
        }

        #[cfg(unix)] {
            use std::os::fd::AsRawFd;

            if self.jobs_pgid.take().is_some() {
                unsafe {
                    if libc::tcsetpgrp(io::stdin().as_raw_fd(), libc::getpid()) != 0 {
                        return Err(io::Error::last_os_error());
                    }
                }
            }
        }

        Ok(())        
    }
}
