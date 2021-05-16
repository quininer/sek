// we never spawn
#![allow(clippy::await_holding_refcell_ref)]

mod util;
mod editor;
mod shell;

use argh::FromArgs;
use scopeguard::defer;
use crossterm::terminal;


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

    let mut shell = shell::config::load().await?;

    terminal::enable_raw_mode()?;

    defer!{
        let _ = terminal::disable_raw_mode();
    };

    shell.start().await?;

    Ok(())
}
