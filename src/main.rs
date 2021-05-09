// we never spawn
#![allow(clippy::await_holding_refcell_ref)]

mod shell;

use std::{ fs, io };
use std::cell::{ Cell, RefCell };
use anyhow::Context;
use bumpalo::Bump;
use argh::FromArgs;
use scopeguard::defer;
use directories::ProjectDirs;
use getrandom::getrandom;
use crossterm::terminal;


pub struct Global {
    bump: Bump,
    strbuf: RefCell<String>,
    columns: Cell<u16>,
    projdir: ProjectDirs,
    session: u64
}

/// Sek Shell
#[derive(FromArgs)]
struct Options {
    /// print version
    #[argh(switch, short = 'v')]
    version: bool
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> anyhow::Result<()> {
    let options: Options = argh::from_env();

    if options.version {
        println!("{}", env!("CARGO_PKG_VERSION"));

        return Ok(());
    }

    let projdir = ProjectDirs::from("", "", env!("CARGO_PKG_NAME"))
        .context("Unable to retrieve project path from system")?;

    let (size, _) = terminal::size()?;
    let session = {
        let mut buf = [0; 8];
        getrandom(&mut buf)?;
        u64::from_le_bytes(buf)
    };

    let global = Global {
        bump: Bump::new(),
        strbuf: RefCell::new(String::new()),
        columns: Cell::new(size),
        projdir, session
    };

    fs::create_dir_all(global.projdir.data_local_dir())
        .or_else(|err| if err.kind() == io::ErrorKind::AlreadyExists {
            Ok(())
        } else {
            Err(err)
        })?;

//    let mut shell = Shell::new(&global)?;
//
//    if let Some(config) = Config::open(&global)? {
//        config.execute(&mut shell).await?;
//    }

    terminal::enable_raw_mode()?;

    defer!{
        let _ = terminal::disable_raw_mode();
    };

    // shell.runloop(&global).await?;

    Ok(())
}
