use std::io;
use std::rc::Rc;
use std::cell::RefCell;
use std::process::{ ExitStatus, Stdio };
use tokio::process::{ self, Command };
use anyhow::Context;
use bstr::ByteSlice;
use super::Shell;


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
    pgid: libc::pid_t,
    queue: Rc<RefCell<Vec<process::Child>>>
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
        #[cfg(unix)] {
            use std::os::unix::process::CommandExt;

            self.cmd.as_std_mut().process_group(shell.morgue.pgid);
        }

        match self.cmd
            .current_dir(shell.env.pwd())
            .envs(&shell.env.map)
            .spawn()
        {
            Ok(child) => Ok(Child {
                child: Some(child),
                morgue: shell.morgue.clone()
            }),
            Err(err) => Err(err).context("spawn failed")
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
        self.morgue.wait_one(&mut child).await
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
            pgid: unsafe {
                libc::getpid()
            },
            queue: Default::default()
        }
    }
}

impl Morgue {
    async fn wait_one(&self, child: &mut process::Child)
        -> io::Result<ExitStatus>
    {
        child.wait().await
    }
    
    pub async fn wait(&mut self) -> io::Result<()> {
        let mut queue = self.queue.borrow_mut();

        for mut ghost in queue.drain(..) {
            self.wait_one(&mut ghost).await?;
        }

        Ok(())        
    }
}
