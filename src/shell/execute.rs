use std::fs;
use std::pin::Pin;
use std::future::Future;
use std::process::{ Stdio, ExitStatus };
use anyhow::Context as AnyhowContext;
use bstr::ByteSlice;
use bumpalo::collections::Vec;
use tokio::io::{ AsyncRead, AsyncReadExt };
use if_chain::if_chain;
use crate::shell::Shell;
use crate::shell::parser::type_::*;
use crate::shell::process::{ ShellCommand, to_stdio };


type Push<'a> = &'a mut dyn FnMut(&[u8]) -> anyhow::Result<()>;

/// TODO replace it with bumpalo::collections::Box
///
/// https://github.com/fitzgen/bumpalo/issues/82
macro_rules! boxed_await {
    ( $fut:expr ) => {{
        let fut = std::boxed::Box::pin($fut) as Pin<std::boxed::Box<dyn Future<Output = _>>>;
        fut.await
    }}
}

impl Literal {
    pub fn eval(&self, shell: &mut Shell, line: &str, push: Push<'_>) -> anyhow::Result<()> {
        let value = &line[self.0.clone()];

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

impl Env {
    pub fn eval(&self, shell: &mut Shell, line: &str, push: Push<'_>) -> anyhow::Result<()> {
        if let Some(val) = line[self.0.clone()]
            .strip_prefix('$')
            .and_then(|name| shell.env.get(name.as_ref()))
        {
            let val = <[u8]>::from_os_str(&val).context("invalid env value")?;
            push(val)?;
        } else {
            push(b"")?;
        }

        Ok(())
    }
}

impl Escape {
    pub fn eval(&self, _shell: &mut Shell, line: &str, push: Push<'_>) -> anyhow::Result<()> {
        let start = self.0.start + 1;
        let end = self.0.end;
        push(line[start..end].as_ref())
    }
}

impl<'c> SubShell<'c> {
    pub async fn eval(&self, shell: &mut Shell, line: &str, push: Push<'_>) -> anyhow::Result<()> {
        let mut shell_cmd = None;
        let mut cmd_new = |osstr: &[u8]| {
            shell_cmd = Some(ShellCommand::new(osstr)?);
            Ok(())
        };
        self.cmd.exe.eval(shell, line, &mut cmd_new)?;

        let mut shell_cmd = shell_cmd.context("the expanded command was empty")?;

        let mut cmd_push = |osstr: &[u8]| shell_cmd.push(osstr);

        for arg in self.cmd.args.iter() {
            boxed_await!(arg.eval(shell, line, &mut cmd_push))?;
        }

        for stdio in self.cmd.redirect.iter() {
            boxed_await!(stdio.redirect(shell, line, &mut shell_cmd))?;
        }

        if let Some(chain) = self.cmd.chain.as_ref() {
            boxed_await!(async {
                chain.exec(shell, line, shell_cmd, Some(push))
                    .await
                    .map(drop)
            })?;
        } else {
            spawn_and_push(shell_cmd, shell, &mut Some(push)).await?;
        }

        Ok(())
    }
}

impl SingleStr {
    pub fn eval(&self, _shell: &mut Shell, line: &str, push: Push<'_>) -> anyhow::Result<()> {
        let start = self.0.start + 1;
        let end = self.0.end - 1;
        push(line[start..end].as_ref())
    }
}

impl<'c> DoubleStr<'c> {
    pub async fn eval(&self, shell: &mut Shell, line: &str, push: Push<'_>) -> anyhow::Result<()> {
        let bump = shell.bump.clone();
        let bump = bump.borrow();

        let mut osbuf = Vec::new_in(&bump);
        let mut push2 = |osstr: &[u8]| {
            osbuf.extend_from_slice(osstr);
            Ok(())
        };

        for item in self.list.iter() {
            match item {
                StrSlice::Str(val) => val.eval(shell, line, &mut push2)?,
                StrSlice::Env(val) => val.eval(shell, line, &mut push2)?,
                StrSlice::Escape(val) => val.eval(shell, line, &mut push2)?,
                StrSlice::SubShell(cmd) => cmd.eval(shell, line, &mut push2).await?
            }
        }

        push(&osbuf)
    }
}

impl<'c> Argument<'c> {
    pub async fn eval(&self, shell: &mut Shell, line: &str, push: Push<'_>) -> anyhow::Result<()> {
        let bump = shell.bump.clone();
        let bump = bump.borrow();

        let mut osbuf = Vec::new_in(&bump);
        let mut push2 = |osstr: &[u8]| {
            osbuf.extend_from_slice(osstr);
            Ok(())
        };

        for item in self.0.iter() {
            match item {
                ArgSlice::Str(v) => v.eval(shell, line, &mut push2)?,
                ArgSlice::Env(v) => v.eval(shell, line, &mut push2)?,
                ArgSlice::Escape(val) => val.eval(shell, line, &mut push2)?,
                ArgSlice::Single(v) => v.eval(shell, line, &mut push2)?,
                ArgSlice::Double(v) => v.eval(shell, line, &mut push2).await?,
                ArgSlice::SubShell(v) => v.eval(shell, line, &mut push2).await?,
            }
        }

        push(&osbuf)
    }
}

impl<'c> Redirect<'c> {
    pub async fn redirect(&self, shell: &mut Shell, line: &str, cmd: &mut ShellCommand)
        -> anyhow::Result<()>
    {
        let mut push = |osstr: &[u8]| {
            let fd = fs::OpenOptions::new()
                .create(true)
                .write(true)
                .append(self.append)
                .open(osstr.to_path()?)?;

            match self.kind {
                StdioKind::Out => cmd.stdout(fd.into()),
                StdioKind::Err => cmd.stderr(fd.into())
            }

            Ok(())
        };

        self.value.eval(shell, line, &mut push).await?;

        Ok(())
    }
}

impl<'c> Chain<'c> {
    pub async fn exec(&self, shell: &mut Shell, line: &str, mut prev_cmd: ShellCommand, mut push: Option<Push<'_>>)
        -> anyhow::Result<ExitStatus>
    {
        let subshell = &self.shell;

        let mut shell_cmd = None;
        let mut cmd_new = |osstr: &[u8]| {
            shell_cmd = Some(ShellCommand::new(osstr)?);
            Ok(())
        };
        subshell.0.exe.eval(shell, line, &mut cmd_new)?;

        let mut shell_cmd = shell_cmd.context("The expanded command was empty")?;

        let mut cmd_push = |osstr: &[u8]| shell_cmd.push(osstr);

        for arg in subshell.0.args.iter() {
            arg.eval(shell, line, &mut cmd_push).await?;
        }

        for stdio in subshell.0.redirect.iter() {
            stdio.redirect(shell, line, &mut shell_cmd).await?;
        }

        match self.kind {
            ChainKind::Pipe => {
                prev_cmd.stdout(Stdio::piped());

                let mut prev_child = prev_cmd.spawn(shell)?;

                if let Some(stdout) = prev_child.take_stdout() {
                    shell_cmd.stdin(to_stdio(stdout));
                }

                let status = if let Some(chain) = subshell.0.chain.as_ref() {
                    boxed_await!(chain.exec(shell, line, shell_cmd, push))?
                } else {
                    spawn_and_push(shell_cmd, shell, &mut push).await?
                };

                prev_child.wait().await?;

                Ok(status)
            },
            ChainKind::Then => {
                spawn_and_push(prev_cmd, shell, &mut push).await?;

                if let Some(chain) = subshell.0.chain.as_ref() {
                    boxed_await!(chain.exec(shell, line, shell_cmd, push))
                } else {
                    spawn_and_push(shell_cmd, shell, &mut push).await
                }
            },
            ChainKind::AndIf => {
                let status = spawn_and_push(prev_cmd, shell, &mut push).await?;

                if status.success() {
                    if let Some(chain) = subshell.0.chain.as_ref() {
                        boxed_await!(chain.exec(shell, line, shell_cmd, push))
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
                    if let Some(chain) = subshell.0.chain.as_ref() {
                        boxed_await!(chain.exec(shell, line, shell_cmd, push))
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

async fn spawn_and_push(mut cmd: ShellCommand, shell: &mut Shell, push: &mut Option<Push<'_>>)
    -> anyhow::Result<ExitStatus>
{
    let mut is_push = false;

    let ret = if_chain! {
        if let Some(push) = push.as_mut();
        if cmd.is_stdout_available();
        then {
            cmd.stdout(Stdio::piped());

            let mut child = cmd.spawn(shell)?;

            if let Some(stdout) = child.take_stdout() {
                let bump = shell.bump.clone();
                let bump = bump.borrow();
                let mut tmpbuf = bumpalo::vec![in &bump; 0; 1024];

                // The size is limited here just to avoid stdout may occupy memory indefinitely.
                read_to_end(
                    stdout.take(shell.arg_max as u64),
                    &mut tmpbuf,
                    push
                ).await?;

                is_push = true;
            }

            child.wait().await
                .map_err(Into::into)
        } else {
            cmd.spawn(shell)?
                .wait().await
                .map_err(Into::into)
        }
    };

    if let Some(push) = push.as_mut().filter(|_| !is_push) {
        push(b"")?;
    }

    ret
}

async fn read_to_end<R: AsyncRead + Unpin>(
    mut reader: R,
    tmpbuf: &mut [u8],
    push: Push<'_>
) -> anyhow::Result<()> {
    loop {
        let n = reader.read(tmpbuf).await?;
        if n == 0 {
            break
        }

        push(&tmpbuf[..n])?;
    }

    Ok(())
}
