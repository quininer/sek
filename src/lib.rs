// # Safety
//
// We don’t use `spawn`, so there is no task that runs at the same time.
#![allow(clippy::await_holding_refcell_ref)]

#[macro_use]
mod util;
mod ui;
mod shell;
mod editor;
mod config;
mod cache;
mod ipc;

use std::path::PathBuf;
use tokio::runtime;

pub fn start_shell(config: Option<PathBuf>) -> anyhow::Result<()> {
    #[cfg(unix)] {
        util::setup_signal_handler()?;
    }

    let mut builder = runtime::Builder::new_current_thread();

    #[cfg(unix)]
    builder.enable_io();
    builder.enable_time();

    let rt = builder.build()?;
    let shell = shell::Shell::new(config)?;
    rt.block_on(shell.start())    
}
