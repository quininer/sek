use std::fs;
use std::pin::Pin;
use std::future::Future;
use std::ffi::{ OsStr, OsString };
use std::process::{ Stdio, ExitStatus };
use anyhow::Context as AnyhowContext;
use tokio::io::AsyncReadExt;
use if_chain::if_chain;
use crate::shell::Shell;
use crate::shell::parser::type_::*;
use crate::shell::process::{ ShellCommand, to_stdio };
use crate::util::arg_max;


type Push<'a> = &'a mut dyn FnMut(&OsStr) -> anyhow::Result<()>;

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
            let home = shell.userdir.home_dir();

            if path.is_empty() {
                push(home.as_ref())
            } else if path.starts_with('/') {
                push(home.join(path.trim_start_matches('/')).as_ref())
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
            push(val)?;
        }

        Ok(())
    }
}

impl<'c> SubShell<'c> {
    pub async fn eval(&self, shell: &mut Shell, line: &str, push: Push<'_>) -> anyhow::Result<()> {
        let mut shell_cmd = None;
        let mut cmd_new = |osstr: &OsStr| {
            shell_cmd = Some(ShellCommand::new(osstr));
            Ok(())
        };
        self.0.exe.eval(shell, line, &mut cmd_new)?;

        let mut shell_cmd = shell_cmd.context("The expanded command was empty")?;

        let mut cmd_push = |osstr: &OsStr| {
            shell_cmd.push(osstr);
            Ok(())
        };

        for arg in self.0.args.iter() {
            boxed_await!(arg.eval(shell, line, &mut cmd_push))?;
        }

        for stdio in self.0.redirect.iter() {
            boxed_await!(stdio.redirect(shell, line, &mut shell_cmd))?;
        }

        if let Some(chain) = self.0.chain.as_ref() {
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
    pub fn eval(&self, shell: &mut Shell, line: &str, push: Push<'_>) -> anyhow::Result<()> {
        let value = &line[self.0.clone()];

        if !value.contains('\\') {
            push(value.as_ref())
        } else {
            shell.strbuf.clear();

            let mut backslash: Option<()> = None;
            for val in value.split_terminator('\\') {
                if backslash.take().is_some() {
                    if val.is_empty() {
                        shell.strbuf.push('\\');
                    } else if val.starts_with('\'') {
                        shell.strbuf.push_str(val);
                    } else {
                        shell.strbuf.push('\\');
                        shell.strbuf.push_str(val);
                    }
                } else if val.is_empty() {
                    backslash = Some(());
                } else {
                    shell.strbuf.push_str(val);
                }
            }

            push(shell.strbuf.as_ref())
        }
    }
}

impl<'c> DoubleStr<'c> {
    pub async fn eval(&self, shell: &mut Shell, line: &str, push: Push<'_>) -> anyhow::Result<()> {
        let mut osbuf = OsString::new();
        let mut push2 = |osstr: &OsStr| {
            osbuf.push(osstr);
            Ok(())
        };

        for item in self.0.iter() {
            match item {
                StrSlice::Str(val) => val.eval(shell, line, &mut push2)?,
                StrSlice::Env(val) => val.eval(shell, line, &mut push2)?,
                StrSlice::SubShell(cmd) => cmd.eval(shell, line, &mut push2).await?
            }
        }

        push(&osbuf)
    }
}

impl<'c> Argument<'c> {
    pub async fn eval(&self, shell: &mut Shell, line: &str, push: Push<'_>) -> anyhow::Result<()> {
        let mut osbuf = OsString::new();
        let mut push2 = |osstr: &OsStr| {
            osbuf.push(osstr);
            Ok(())
        };

        for item in self.0.iter() {
            match item {
                ArgSlice::Str(v) => v.eval(shell, line, &mut push2)?,
                ArgSlice::Env(v) => v.eval(shell, line, &mut push2)?,
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
        let mut stdio_want: Option<()> = Some(());
        let mut push = |osstr: &OsStr| {
            if stdio_want.take().is_some() {
                let fd = fs::OpenOptions::new()
                    .create(true)
                    .write(true)
                    .append(self.append)
                    .open(osstr)?;

                match self.ty {
                    StdioType::Out => cmd.stdout(fd.into()),
                    StdioType::Err => cmd.stderr(fd.into())
                }
            } else {
                cmd.push(osstr);
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
        let subshell = match self {
            Chain::Pipe(subshell) => subshell,
            Chain::Then(subshell) => subshell,
            Chain::AndIf(subshell) => subshell,
            Chain::OrIf(subshell) => subshell,
        };

        let mut shell_cmd = None;
        let mut cmd_new = |osstr: &OsStr| {
            shell_cmd = Some(ShellCommand::new(osstr));
            Ok(())
        };
        subshell.0.exe.eval(shell, line, &mut cmd_new)?;

        let mut shell_cmd = shell_cmd.context("The expanded command was empty")?;

        let mut cmd_push = |osstr: &OsStr| {
            shell_cmd.push(osstr);
            Ok(())
        };

        for arg in subshell.0.args.iter() {
            arg.eval(shell, line, &mut cmd_push).await?;
        }

        for stdio in subshell.0.redirect.iter() {
            stdio.redirect(shell, line, &mut shell_cmd).await?;
        }

        match self {
            Chain::Pipe(_) => {
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
            Chain::Then(_) => {
                spawn_and_push(prev_cmd, shell, &mut push).await?;

                if let Some(chain) = subshell.0.chain.as_ref() {
                    boxed_await!(chain.exec(shell, line, shell_cmd, push))
                } else {
                    spawn_and_push(shell_cmd, shell, &mut push).await
                }
            },
            Chain::AndIf(_) => {
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
            Chain::OrIf(_) => {
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
    if_chain! {
        if let Some(push) = push;
        if cmd.is_stdout_available();
        then {
            cmd.stdout(Stdio::piped());

            let mut child = cmd.spawn(shell)?;

            if let Some(stdout) = child.take_stdout() {
                shell.strbuf.clear();

                // The size is limited here just to avoid stdout may occupy memory indefinitely.
                stdout
                    .take(arg_max() as u64)
                    .read_to_string(&mut shell.strbuf).await?;

                push(shell.strbuf.as_ref())?;
            }

            child.wait().await
                .map_err(Into::into)
        } else {
            cmd.spawn(shell)?
                .wait().await
                .map_err(Into::into)
        }
    }
}
