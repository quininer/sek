use std::{ fs, io };
use std::marker::Unpin;
use std::process::Stdio;
use anyhow::Context;
use bstr::ByteSlice;
use tokio::io::{ AsyncRead, AsyncReadExt };
use smallvec::SmallVec;
use crate::shell::Shell;
use crate::shell::syntax::{ self, ArgSlice, StrSlice, StdioKind, ChainKind };
use super::external::Leader;
use super::Command as ShellCommand;
use super::Status as ExitStatus;


pub async fn execute(
    shell: &Shell,
    input: &str,
    cmd: syntax::Command,
    leader: &Leader,
) -> anyhow::Result<ExitStatus> {
    let mut osbuf = <SmallVec<[u8; 32]>>::new();
    let mut push = |osstr: &[u8]| {
        osbuf.extend_from_slice(osstr);
        Ok(())
    };
    cmd.exe(&shell.parser).eval(shell, input, &mut push).await?;

    if osbuf.is_empty() {
        anyhow::bail!("the expanded command was empty");
    }

    let mut shell_cmd = ShellCommand::new(shell, &osbuf, Some(leader.clone()))?;
    let mut push = |osstr: &[u8]| shell_cmd.push(osstr);

    for arg in cmd.args(&shell.parser) {
        arg.eval(shell, input, &mut push).await?;
    }

    for redirect in cmd.redirect(&shell.parser) {
        redirect.eval(shell, input, &mut shell_cmd).await?;
    }

    let status = if let Some(chain) = cmd.chain(&shell.parser) {
        chain.eval(shell, input, shell_cmd, None).await?
    } else {
        shell_cmd.spawn(shell)?.wait().await?
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
                let env = shell.env.borrow();
                let home = env.home();
                let home = <[u8]>::from_path(home).context("invalid home path")?;
                push(home)
            } else if path.starts_with('/') {
                let env = shell.env.borrow();
                let home = env.home();
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
        let env = shell.env.borrow();

        if let Some(val) = input[span]
            .strip_prefix('$')
            .and_then(|name| env.get(name.as_ref()))
        {
            let val = <[u8]>::from_os_str(val).context("invalid env value")?;
            push(val)?;
        } else {
            push(b"")?;
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
    async fn eval(self, shell: &Shell, input: &str, push: Push<'_>) -> anyhow::Result<()> {
        let cmd = self.command(&shell.parser);

        let mut osbuf = <SmallVec<[u8; 32]>>::new();
        let mut push_cmd = |osstr: &[u8]| {
            osbuf.extend_from_slice(osstr);
            Ok(())
        };
        cmd.exe(&shell.parser).eval(shell, input, &mut push_cmd).await?;

        if osbuf.is_empty() {
            anyhow::bail!("the expanded command was empty");
        }        

        let mut shell_cmd = ShellCommand::new(shell, &osbuf, None)?;
        let mut cmd_push = |osstr: &[u8]| shell_cmd.push(osstr);

        for arg in cmd.args(&shell.parser) {
            Box::pin(arg.eval(shell, input, &mut cmd_push)).await?;
        }

        for redirect in cmd.redirect(&shell.parser) {
            Box::pin(redirect.eval(shell, input, &mut shell_cmd)).await?;
        }

        if let Some(chain) = cmd.chain(&shell.parser) {
            Box::pin(chain.eval(shell, input, shell_cmd, Some(push))).await?;
        } else {
            spawn_and_push(shell_cmd, shell, &mut Some(push)).await?;
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
    async fn eval(self, shell: &Shell, input: &str, push: Push<'_>) -> anyhow::Result<()> {
        let mut osbuf = <SmallVec<[u8; 32]>>::new();
        let mut push2 = |osstr: &[u8]| {
            // The size is limited here just to avoid stdout may occupy memory indefinitely.
            if osbuf.len() + osstr.len() > shell.env.borrow().max_args_len {
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
                StrSlice::SubShell(cmd) => Box::pin(cmd.eval(shell, input, &mut push2)).await?,
            }
        }

        push(&osbuf)
    }
}

impl syntax::Argument {
    pub async fn eval(self, shell: &Shell, input: &str, push: Push<'_>) -> anyhow::Result<()> {
        let mut osbuf = <SmallVec<[u8; 32]>>::new();
        let mut push2 = |osstr: &[u8]| {
            // The size is limited here just to avoid stdout may occupy memory indefinitely.
            if osbuf.len() + osstr.len() > shell.env.borrow().max_args_len {
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
                ArgSlice::DoubleStr(v) => v.eval(shell, input, &mut push2).await?,
                ArgSlice::SubShell(v) => Box::pin(v.eval(shell, input, &mut push2)).await?,
            }
        }

        push(&osbuf)
    }
}

impl syntax::Redirect {
    async fn eval(self, shell: &Shell, input: &str, cmd: &mut ShellCommand) -> anyhow::Result<()> {
        let kind = self.kind(&shell.parser);
        let append = self.append(&shell.parser);
        
        let mut push = |osstr: &[u8]| {
            let path = osstr.to_path()?;
            let fd = fs::OpenOptions::new()
                .create(true)
                .write(true)
                .append(append)
                .truncate(!append)
                .open(path)
                .with_context(|| format!("failed to open redirect target: {:?}", path))?;

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
            .eval(shell, input, &mut push).await
    }
}

impl syntax::Chain {
    async fn eval(self, shell: &Shell, input: &str, mut prev_cmd: ShellCommand, mut push: Option<Push<'_>>)
        -> anyhow::Result<ExitStatus>
    {
        let kind = self.kind(&shell.parser);
        let subshell = self.command(&shell.parser);

        let mut osbuf = <SmallVec<[u8; 32]>>::new();
        let mut push_cmd = |osstr: &[u8]| {
            osbuf.extend_from_slice(osstr);
            Ok(())
        };
        subshell.exe(&shell.parser)
            .eval(shell, input, &mut push_cmd).await?;

        if osbuf.is_empty() {
            anyhow::bail!("the expanded command was empty");
        }        

        let mut shell_cmd = ShellCommand::new(shell, &osbuf, prev_cmd.leader())?;
        let mut cmd_push = |osstr: &[u8]| shell_cmd.push(osstr);

        for arg in subshell.args(&shell.parser) {
            Box::pin(arg.eval(shell, input, &mut cmd_push)).await?;
        }

        for redirect in subshell.redirect(&shell.parser) {
            Box::pin(redirect.eval(shell, input, &mut shell_cmd)).await?;
        }

        let chain = subshell.chain(&shell.parser);

        match kind {
            ChainKind::Pipe(stdio_kind) => {
                let mut prev_child;

                match stdio_kind {
                    StdioKind::Out => {
                        prev_cmd.stdout(Stdio::piped());
                        prev_child = prev_cmd.spawn(shell)?;

                        if let Some(stdout) = prev_child.stdout().take() {
                            shell_cmd.stdin(stdout.try_into()?);
                        }                        
                    },
                    StdioKind::Err => {
                        prev_cmd.stderr(Stdio::piped());
                        prev_child = prev_cmd.spawn(shell)?;
                        
                        if let Some(stderr) = prev_child.stderr().take() {
                            shell_cmd.stdin(stderr.try_into()?);
                        }
                    },
                    StdioKind::All => {
                        let (reader, writer) = io::pipe()?;
                        prev_cmd.stdout(writer.try_clone()?.into());
                        prev_cmd.stderr(writer.into());
                        shell_cmd.stdin(reader.into());

                        prev_child = prev_cmd.spawn(shell)?;
                    },
                }

                let status = if let Some(chain) = chain.as_ref() {
                    Box::pin(chain.eval(shell, input, shell_cmd, push)).await?
                } else {
                    spawn_and_push(shell_cmd, shell, &mut push).await?
                };

                prev_child.wait().await?;

                Ok(status)
            },
            ChainKind::Then => {
                spawn_and_push(prev_cmd, shell, &mut push).await?;

                if let Some(chain) = chain.as_ref() {
                    Box::pin(chain.eval(shell, input, shell_cmd, push)).await
                } else {
                    spawn_and_push(shell_cmd, shell, &mut push).await
                }
            },
            ChainKind::AndIf => {
                let status = spawn_and_push(prev_cmd, shell, &mut push).await?;

                if status.success() {
                    if let Some(chain) = chain.as_ref() {
                        Box::pin(chain.eval(shell, input, shell_cmd, push)).await
                    } else {
                        spawn_and_push(shell_cmd, shell, &mut push).await
                    }
                } else {
                    Ok(status)
                }
            },
            ChainKind::OrIf => {
                let status = spawn_and_push(prev_cmd, shell, &mut push).await?;

                if !status.success() {
                    if let Some(chain) = chain.as_ref() {
                        Box::pin(chain.eval(shell, input, shell_cmd, push)).await
                    } else {
                        spawn_and_push(shell_cmd, shell, &mut push).await
                    }
                } else {
                    Ok(status)
                }
            }
        }
    }
}

async fn spawn_and_push(mut cmd: ShellCommand, shell: &Shell, push: &mut Option<Push<'_>>)
    -> anyhow::Result<ExitStatus>
{
    let status = if let Some(push) = push.as_mut()
        .filter(|_| cmd.is_stdout_available())
    {
        cmd.stdout(Stdio::piped());

        let mut child = cmd.spawn(shell)?;

        if let Some(stdout) = child.stdout().take() {
            read_to_end(stdout, push).await?;
        }

        child.wait().await?
    } else {
        cmd.spawn(shell)?.wait().await?
    };

    #[cfg(unix)] {
        if let Some(sig @ (libc::SIGINT | libc::SIGQUIT)) = status.signal() {
            anyhow::bail!("cancel command by signal: {}", sig);
        }
    }

    Ok(status)
}

async fn read_to_end<R: AsyncRead + Unpin>(
    mut reader: R,
    push: Push<'_>
) -> anyhow::Result<()> {
    let mut buf = [0; 1024];
    
    loop {
        match reader.read(&mut buf).await {
            Ok(0) => break,
            Ok(n) => push(&buf[..n])?,
            Err(ref err) if err.kind() == io::ErrorKind::Interrupted => (),
            Err(err) => return Err(err.into())
        }
    }

    Ok(())
}
