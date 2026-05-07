use std::pin::Pin;
use anyhow::Context;
use bstr::{ ByteSlice, BString };
use crate::shell::Shell;


pub type BuiltinCommand =
    for<'a> fn(&'a Shell, &'a [BString]) -> BuiltinFuture<'a>;

pub type BuiltinFuture<'a> =
    Pin<Box<dyn Future<Output = anyhow::Result<bool>> + 'a>>;

pub fn builtin_command(cmd: &[u8]) -> Option<BuiltinCommand> {
    // TODO use phf ?

    match cmd {
        b"cd" => Some(|shell, args| Box::pin(cd(shell, args))),
        b"set-env" => Some(|shell, args| Box::pin(set_env(shell, args))),
        b"unset-env" => Some(|shell, args| Box::pin(unset_env(shell, args))),
        b"push-path" => Some(|shell, args| Box::pin(push_path(shell, args))),
        _ => None
    }
}

async fn cd(shell: &Shell, args: &[BString]) -> anyhow::Result<bool> {
    anyhow::ensure!(args.len() == 1, "cd args length != 1");
    
    let path = &args[0];

    if path == "-" {
        shell.env.borrow_mut().go_back()?;
    } else {
        let path = path.to_path().context("not os str path")?;
        shell.env.borrow_mut().cd(path)?;
    }

    Ok(true)    
}

async fn set_env(shell: &Shell, args: &[BString]) -> anyhow::Result<bool> {
    anyhow::ensure!(args.len() == 2, "set-env args length != 2");
    
    let key = &args[0].to_os_str().context("not os str key")?;
    let val = &args[1].to_os_str().context("not os str value")?;
    shell.env.borrow_mut().set(key, val.into());

    Ok(true)    
}

async fn unset_env(shell: &Shell, args: &[BString]) -> anyhow::Result<bool> {
    anyhow::ensure!(args.len() == 1, "unset-env args length != 1");
    
    let key = &args[0].to_os_str().context("not os str key")?;
    shell.env.borrow_mut().unset(key);

    Ok(true)    
}

async fn push_path(shell: &Shell, args: &[BString]) -> anyhow::Result<bool> {
    anyhow::ensure!(args.len() == 1, "push-path args length != 1");
    
    let path = &args[0].to_path().context("not os str value")?;
    shell.env.borrow_mut().push_path(path)?;

    Ok(true)    
}
