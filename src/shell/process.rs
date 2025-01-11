use std::process::{ Command, Stdio, Child };
use bstr::ByteSlice;


pub struct ShellCommand {
    cmd: Command,
    redirect_stdout: bool,
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

    pub fn spawn(&mut self) -> anyhow::Result<Child> {
        #[cfg(unix)] {
            use std::process;
            use std::os::unix::process::CommandExt;

            self.cmd.process_group(process::id() as _);
        }
        
        self.cmd.spawn().map_err(Into::into)
    }
}
