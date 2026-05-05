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
        _ => None
    }
}

async fn cd(shell: &Shell, args: &[BString]) -> anyhow::Result<bool> {
    anyhow::ensure!(args.len() == 1, "cd args length != 1");
    
    let path = args[0].to_path().context("not utf8 path")?;
    shell.env.borrow_mut().cd(path)?;

    Ok(true)    
}
