use std::env;
use std::path::PathBuf;
use argh::FromArgs;
use anyhow::Context;
use tokio::runtime;
use directories::ProjectDirs;
// use sek::daemon;


/// The Sek Shell
#[derive(FromArgs)]
struct Options {
    /// use specified config
    #[argh(option, short = 'c')]
    config: Option<PathBuf>,
    
    /// use specified pwd
    #[argh(option, short = 'p')]
    pwd: Option<PathBuf>,

    // /// sub command
    // #[argh(subcommand)]
    // subcmd: Option<SubCommand>
}

// #[derive(FromArgs)]
// #[argh(subcommand)]
// enum SubCommand {
//     // Daemon(daemon::Options)
// }

fn main() -> anyhow::Result<()> {
    let mut options: Options = argh::from_env();

    let projdir = ProjectDirs::from("", "", env!("CARGO_PKG_NAME"))
        .context("Unable to retrieve project path from system")?;    

    let pwd = if let Some(pwd) = options.pwd.take() {
        pwd
    } else {
        env::current_dir()?
    };

    let confpath = if let Some(path) = options.config.take() {
        path
    } else {
        projdir.config_dir().join("config")
    };

    let rt = runtime::Builder::new_current_thread()
        .enable_io()
        .build()?;

    // if let Some(SubCommand::Daemon(daemon)) = options.subcmd {
    //     rt.block_on(daemon.exec(projdir, confpath))
    // } else {
        let shell = sek::shell::Shell::new(projdir, pwd, confpath)?;
        rt.block_on(shell.start())       
    // }
}
