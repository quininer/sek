use std::rc::Rc;
use std::pin::Pin;
use std::collections::HashMap;
use std::future::Future;
use anyhow::Context;
use bstr::{ ByteSlice, ByteVec };
use bumpalo::collections::Vec as BumpVec;
use crate::shell::Shell;
use crate::shell::parser::type_::Command;


pub const BUILTIN_COMMANDS: &[(&str, CommandFn)] = &[
    ("cd", cd),
    ("set-env", set_env),
    ("unset-env", unset_env),
    ("push-path", push_path),
    ("alias", alias)
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

        shell.env.cd(path)?;

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

        let bump = shell.bump.clone();
        let bump = bump.borrow();

        let mut name = BumpVec::with_capacity_in(8, &bump);
        let mut push = |osstr: &[u8]| {
            name.extend_from_slice(osstr);
            Ok(())
        };
        cmd.args[0].eval(shell, line, &mut push).await?;

        let mut value = Vec::with_capacity(8);
        let mut push = |osstr: &[u8]| {
            value.extend_from_slice(osstr);
            Ok(())
        };
        cmd.args[1].eval(shell, line, &mut push).await?;

        let name = name.to_os_str().context("invalid env name")?;
        let value = value.into_os_string()
            .ok()
            .context("invalid value name")?;

        shell.env.set(name, value);

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

        let bump = shell.bump.clone();
        let bump = bump.borrow();

        let mut name = BumpVec::with_capacity_in(8, &bump);
        let mut push = |osstr: &[u8]| {
            name.extend_from_slice(osstr);
            Ok(())
        };
        cmd.args[0].eval(shell, line, &mut push).await?;

        let name = name.to_os_str().context("invalid env name")?;

        shell.env.remove(name);

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

        let mut path = Vec::with_capacity(8);
        let mut push = |osstr: &[u8]| {
            path.extend_from_slice(osstr);
            Ok(())
        };
        cmd.args[0].eval(shell, line, &mut push).await?;

        let path = path.into_path_buf()
            .ok()
            .context("invalid env name")?;

        shell.env.push_path(path)?;

        Ok(())
    }
}

async_fn!{
    async fn alias(shell, line, cmd) {
        if cmd.args.len() != 2
            || !cmd.redirect.is_empty()
            || cmd.chain.is_some()
        {
            return Err(anyhow::format_err!("bad argument"));
        }

        // TODO

        Ok(())
    }
}
