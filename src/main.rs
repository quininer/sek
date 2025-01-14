use std::env;
use std::path::PathBuf;
use argh::FromArgs;
use anyhow::Context;
use directories::ProjectDirs;


/// The Sek Shell
#[derive(FromArgs)]
struct Options {
    /// use specified config
    #[argh(option, short = 'c')]
    config: Option<PathBuf>,
    
    /// use specified pwd
    #[argh(option, short = 'p')]
    pwd: Option<PathBuf>
}

fn main() -> anyhow::Result<()> {
    let mut options: Options = argh::from_env();

    let pwd = if let Some(pwd) = options.pwd.take() {
        pwd
    } else {
        env::current_dir()?
    };

    let confpath = if let Some(path) = options.config.take() {
        path
    } else {
        let projdir = ProjectDirs::from("", "", env!("CARGO_PKG_NAME"))
            .context("Unable to retrieve project path from system")?;
        projdir.config_dir().join("config")
    };    

    sek::shell::Shell::new(pwd, confpath)?.start()?;
    
    Ok(())
}
