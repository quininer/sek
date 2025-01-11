use std::fs;
use std::io;
use std::process::{ Stdio, ExitStatus };
use anyhow::Context;
use bstr::ByteSlice;
use crate::shell::Shell;
use crate::shell::syntax::{ self, ArgSlice, StrSlice, StdioKind, ChainKind };
use crate::shell::process::ShellCommand;


pub fn execute(shell: &Shell, input: &str, cmd: syntax::Command) -> anyhow::Result<ExitStatus> {
    let mut shell_cmd = None;
    let mut cmd_new = |osstr: &[u8]| {
        shell_cmd = Some(ShellCommand::new(osstr)?);
        Ok(())
    };
    cmd.exe(&shell.parser).eval(shell, input, &mut cmd_new)?;
    
    let mut shell_cmd = shell_cmd.context("the expanded command was empty")?;
    let mut push = |osstr: &[u8]| shell_cmd.push(osstr);

    for arg in cmd.args(&shell.parser) {
        arg.eval(shell, input, &mut push)?;
    }

    for redirect in cmd.redirect(&shell.parser) {
        redirect.eval(shell, input, &mut shell_cmd)?;
    }

    let status = if let Some(chain) = cmd.chain(&shell.parser) {
        chain.eval(shell, input, shell_cmd, None)?
    } else {
        shell_cmd.spawn()?.wait()?
    };

    Ok(status)    
}

type Push<'a> = &'a mut dyn FnMut(&[u8]) -> anyhow::Result<()>;

impl syntax::Literal {
    fn eval(self, shell: &Shell, input: &str, push: Push<'_>) -> anyhow::Result<()> {
        let span = self.span(&shell.parser);
        let value = &input[span];

        if let Some(path) = value.strip_prefix('~') {
            if path.is_empty() {
                let home = shell.env.home();
                let home = <[u8]>::from_path(&home).context("invalid home path")?;
                push(home)
            } else if path.starts_with('/') {
                let home = shell.env.home();
                let newpath = home.join(path.trim_start_matches('/'));
                let newpath = <[u8]>::from_path(&newpath).context("invalid path")?;
                push(newpath)
            } else {
                push(value.as_ref())
            }
        } else {
            push(value.as_ref())
        }
    }   
}

impl syntax::Variable {
    fn eval(self, shell: &Shell, input: &str, push: Push<'_>) -> anyhow::Result<()> {
        let span = self.span(&shell.parser);

        if let Some(val) = input[span]
            .strip_prefix('$')
            .and_then(|name| shell.env.get(name.as_ref()))
        {
            let val = <[u8]>::from_os_str(&val).context("invalid env value")?;
            push(val)?;
        }

        Ok(())
    }
}

impl syntax::Escape {
    fn eval(self, shell: &Shell, input: &str, push: Push<'_>) -> anyhow::Result<()> {
        let span = self.value(&shell.parser);
        push(input[span].as_ref())
    }
}

impl syntax::SubShell {
    fn eval(self, shell: &Shell, input: &str, push: Push<'_>) -> anyhow::Result<()> {
        let mut shell_cmd = None;
        let mut cmd_new = |osstr: &[u8]| {
            shell_cmd = Some(ShellCommand::new(osstr)?);
            Ok(())
        };

        let cmd = self.command(&shell.parser);
        cmd.exe(&shell.parser).eval(shell, input, &mut cmd_new)?;

        let mut shell_cmd = shell_cmd.context("the expanded command was empty")?;
        let mut cmd_push = |osstr: &[u8]| shell_cmd.push(osstr);

        for arg in cmd.args(&shell.parser) {
            arg.eval(shell, input, &mut cmd_push)?;
        }

        for redirect in cmd.redirect(&shell.parser) {
            redirect.eval(shell, input, &mut shell_cmd)?;
        }

        if let Some(chain) = cmd.chain(&shell.parser) {
            chain.eval(shell, input, shell_cmd, Some(push))?;
        } else {
            spawn_and_push(shell_cmd, shell, &mut Some(push))?;
        }

        Ok(())
    }
}

impl syntax::SingleStr {
    fn eval(self, shell: &Shell, input: &str, push: Push<'_>) -> anyhow::Result<()> {
        let start = self.start(&shell.parser);
        let end = self.end(&shell.parser);
        let value = end
            .map(|end| &input[start.end..end.start])
            .unwrap_or_else(|| &input[start.end..]);
        push(value.as_ref())
    }
}

impl syntax::DoubleStr {
    fn eval(self, shell: &Shell, input: &str, push: Push<'_>) -> anyhow::Result<()> {
        // TODO arg max
        
        let arg_max = 1024 * 4;
        let mut osbuf = Vec::new();
        let mut push2 = |osstr: &[u8]| {
            // The size is limited here just to avoid stdout may occupy memory indefinitely.
            if osbuf.len() + osstr.len() > arg_max {
                return Err(anyhow::format_err!("Command argument is too long"));
            }

            osbuf.extend_from_slice(osstr);
            Ok(())
        };

        for s in self.slice(&shell.parser) {
            match s {
                StrSlice::Literal(val) => val.eval(shell, input, &mut push2)?,
                StrSlice::Variable(val) => val.eval(shell, input, &mut push2)?,
                StrSlice::Escape(val) => val.eval(shell, input, &mut push2)?,
                StrSlice::SubShell(cmd) => cmd.eval(shell, input, &mut push2)?,
            }
        }

        push(&osbuf)
    }
}

impl syntax::Argument {
    fn eval(self, shell: &Shell, input: &str, push: Push<'_>) -> anyhow::Result<()> {
        // TODO arg max

        let arg_max = 1024 * 4;
        let mut osbuf = Vec::new();
        let mut push2 = |osstr: &[u8]| {
            // The size is limited here just to avoid stdout may occupy memory indefinitely.
            if osbuf.len() + osstr.len() > arg_max {
                return Err(anyhow::format_err!("Command argument is too long"));
            }

            osbuf.extend_from_slice(osstr);
            Ok(())
        };

        for arg in self.slice(&shell.parser) {
            match arg {
                ArgSlice::Literal(v) => v.eval(shell, input, &mut push2)?,
                ArgSlice::Variable(v) => v.eval(shell, input, &mut push2)?,
                ArgSlice::Escape(v) => v.eval(shell, input, &mut push2)?,
                ArgSlice::SingleStr(v) => v.eval(shell, input, &mut push2)?,
                ArgSlice::DoubleStr(v) => v.eval(shell, input, &mut push2)?,
                ArgSlice::SubShell(v) => v.eval(shell, input, &mut push2)?,
            }
        }

        push(&osbuf)
    }
}

impl syntax::Redirect {
    fn eval(self, shell: &Shell, input: &str, cmd: &mut ShellCommand) -> anyhow::Result<()> {
        let kind = self.kind(&shell.parser);
        let append = self.append(&shell.parser);
        
        let mut push = |osstr: &[u8]| {
            let fd = fs::OpenOptions::new()
                .create(true)
                .write(true)
                .append(append)
                .open(osstr.to_path()?)?;

            match kind {
                StdioKind::Out => cmd.stdout(fd.into()),
                StdioKind::Err => cmd.stderr(fd.into()),
                StdioKind::All => {
                    let fd2 = fd.try_clone()?;
                    cmd.stdout(fd.into());
                    cmd.stderr(fd2.into());
                }
            }

            Ok(())
        };

        self.value(&shell.parser)
            .eval(shell, input, &mut push)
    }
}

impl syntax::Chain {
    fn eval(self, shell: &Shell, input: &str, mut prev_cmd: ShellCommand, mut push: Option<Push<'_>>)
        -> anyhow::Result<ExitStatus>
    {
        let kind = self.kind(&shell.parser);
        let subshell = self.command(&shell.parser);

        let mut shell_cmd = None;
        let mut cmd_new = |osstr: &[u8]| {
            shell_cmd = Some(ShellCommand::new(osstr)?);
            Ok(())
        };
        subshell.exe(&shell.parser)
            .eval(shell, input, &mut cmd_new)?;

        let mut shell_cmd = shell_cmd.context("The expanded command was empty")?;
        let mut cmd_push = |osstr: &[u8]| shell_cmd.push(osstr);

        for arg in subshell.args(&shell.parser) {
            arg.eval(shell, input, &mut cmd_push)?;
        }

        for redirect in subshell.redirect(&shell.parser) {
            redirect.eval(shell, input, &mut shell_cmd)?;
        }

        let chain = subshell.chain(&shell.parser);

        match kind {
            ChainKind::Pipe(stdio_kind) => {
                prev_cmd.stdout(Stdio::piped());

                let mut prev_child = prev_cmd.spawn()?;

                match stdio_kind {
                    StdioKind::Out => if let Some(stdout) = prev_child.stdout.take() {
                        shell_cmd.stdin(stdout.into());
                    },
                    StdioKind::Err => if let Some(stderr) = prev_child.stderr.take() {
                        shell_cmd.stdin(stderr.into());
                    },
                    StdioKind::All => todo!()
                }

                let status = if let Some(chain) = chain.as_ref() {
                    chain.eval(shell, input, shell_cmd, push)?
                } else {
                    spawn_and_push(shell_cmd, shell, &mut push)?
                };

                // TODO wait by pgid
                prev_child.wait()?;

                Ok(status)
            },
            ChainKind::Then => {
                spawn_and_push(prev_cmd, shell, &mut push)?;

                if let Some(chain) = chain.as_ref() {
                    chain.eval(shell, input, shell_cmd, push)
                } else {
                    spawn_and_push(shell_cmd, shell, &mut push)
                }
            },
            ChainKind::AndIf => {
                let status = spawn_and_push(prev_cmd, shell, &mut push)?;

                if status.success() {
                    if let Some(chain) = chain.as_ref() {
                        chain.eval(shell, input, shell_cmd, push)
                    } else {
                        spawn_and_push(shell_cmd, shell, &mut push)
                    }
                } else {
                    Ok(status)
                }
            },
            ChainKind::OrIf => {
                let status = spawn_and_push(prev_cmd, shell, &mut push)?;

                if !status.success() {
                    if let Some(chain) = chain.as_ref() {
                        chain.eval(shell, input, shell_cmd, push)
                    } else {
                        spawn_and_push(shell_cmd, shell, &mut push)
                    }
                } else {
                    Ok(status)
                }
            }
        }
    }
}

fn spawn_and_push(mut cmd: ShellCommand, _shell: &Shell, push: &mut Option<Push<'_>>)
    -> anyhow::Result<ExitStatus>
{
    let ret = if let Some(push) = push.as_mut()
        .filter(|_| cmd.is_stdout_available())
    {
        cmd.stdout(Stdio::piped());

        let mut child = cmd.spawn()?;

        if let Some(stdout) = child.stdout.take() {
            let mut tmpbuf = vec![0; 1024];
            read_to_end(stdout, &mut tmpbuf, push)?;
        }

        child.wait()
            .map_err(Into::into)
    } else {
        cmd.spawn()?
            .wait()
            .map_err(Into::into)
    };

    ret
}

fn read_to_end<R: io::Read>(
    mut reader: R,
    tmpbuf: &mut [u8],
    push: Push<'_>
) -> anyhow::Result<()> {
    loop {
        let n = reader.read(tmpbuf)?;
        if n == 0 {
            break
        }

        push(&tmpbuf[..n])?;
    }

    Ok(())
}
