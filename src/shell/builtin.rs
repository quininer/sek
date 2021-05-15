use std::rc::Rc;
use std::pin::Pin;
use std::collections::HashMap;
use std::future::Future;
use anyhow::Context;
use bstr::{ ByteSlice, ByteVec };
use bumpalo::collections::Vec as BumpVec;
use if_chain::if_chain;
use crate::shell::Shell;
use crate::shell::parser::type_::{ Command, Argument, ArgSlice, Literal };


pub const BUILTIN_COMMANDS: &[(&str, CommandFn)] = &[
    ("cd", cd),
    ("set-env", set_env),
    ("unset-env", unset_env),
    ("push-path", push_path),
];

type CommandFn = for<'a> fn(&'a mut Shell, &'a str, &'a Command<'_>)
    -> Pin<Box<dyn Future<Output = anyhow::Result<()>> + 'a>>;

pub async fn try_command(shell: &mut Shell, line: &str, cmd: &Command<'_>) -> anyhow::Result<bool> {
    let exe = &line[cmd.exe.0.clone()];

    for (name, cmdfn) in BUILTIN_COMMANDS {
        if exe.eq_ignore_ascii_case(name) {
            cmdfn(shell, line, cmd).await?;
            return Ok(true);
        }
    }

    Ok(false)
}

macro_rules! async_fn {
    (
        async fn $name:ident ( $shell:ident, $line:ident, $cmd:ident )
        $block:block
    ) => {
        fn $name<'a>($shell: &'a mut Shell, $line: &'a str, $cmd: &'a Command<'_>)
            -> Pin<Box<dyn Future<Output = anyhow::Result<()>> + 'a>>
        {
            Box::pin(async move { $block })
        }
    }
}

async_fn!{
    async fn cd(shell, line, cmd) {
        if cmd.args.len() != 1
            || !cmd.redirect.is_empty()
            || cmd.chain.is_some()
        {
            return Err(anyhow::format_err!("bad argument"));
        }

        let bump = shell.bump.clone();
        let bump = bump.borrow();

        let mut path = BumpVec::with_capacity_in(8, &bump);
        let mut push = |osstr: &[u8]| {
            path.extend_from_slice(osstr);
            Ok(())
        };
        cmd.args[0].eval(shell, line, &mut push).await?;

        let path = path.to_path().context("invalid path")?;

        shell.env.cd(path)
            .with_context(|| format!("{:?}", path))?;

        Ok(())
    }
}

async_fn!{
    async fn set_env(shell, line, cmd) {
        if cmd.args.len() != 2
            || !cmd.redirect.is_empty()
            || cmd.chain.is_some()
        {
            return Err(anyhow::format_err!("bad argument"));
        }

        let name = take_name(line, cmd)?;

        let mut value = Vec::with_capacity(8);
        let mut push = |osstr: &[u8]| {
            value.extend_from_slice(osstr);
            Ok(())
        };
        cmd.args[1].eval(shell, line, &mut push).await?;

        let value = value.into_os_string()
            .ok()
            .context("invalid value name")?;

        shell.env.set(name.as_ref(), value);

        Ok(())
    }
}

async_fn!{
    async fn unset_env(shell, line, cmd) {
        if cmd.args.len() != 1
            || !cmd.redirect.is_empty()
            || cmd.chain.is_some()
        {
            return Err(anyhow::format_err!("bad argument"));
        }

        let name = take_name(line, cmd)?;
        shell.env.remove(name.as_ref());

        Ok(())
    }
}

async_fn!{
    async fn push_path(shell, line, cmd) {
        if cmd.args.len() != 1
            || !cmd.redirect.is_empty()
            || cmd.chain.is_some()
        {
            return Err(anyhow::format_err!("bad argument"));
        }

        let bump = shell.bump.clone();
        let bump = bump.borrow();

        let mut path = BumpVec::with_capacity_in(16, &bump);
        let mut push = |osstr: &[u8]| {
            path.extend_from_slice(osstr);
            Ok(())
        };
        cmd.args[0].eval(shell, line, &mut push).await?;

        let path = path.to_path().context("invalid path")?;
        let path = path.canonicalize()
            .with_context(|| format!("{:?}", path))?;
        shell.env.push_path(path)?;

        Ok(())
    }
}

#[inline]
fn take_name<'a>(line: &'a str, cmd: &'a Command<'_>) -> anyhow::Result<&'a str> {
    if_chain!{
        if let Argument(arg) = &cmd.args[0];
        if arg.len() == 1;
        if let ArgSlice::Str(Literal(span)) = &arg[0];
        then {
            Ok(&line[span.clone()])
        } else {
            Err(anyhow::format_err!("variable name must be literal"))
        }
    }
}
