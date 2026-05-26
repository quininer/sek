use std::env;
use std::path::PathBuf;
use argh::FromArgs;
use tokio::runtime;


/// The Sek Shell
#[derive(FromArgs)]
struct Options {
    /// use specified config
    #[argh(option, short = 'c')]
    config: Option<PathBuf>,
    
    /// use specified work directory
    #[argh(option, short = 'p')]
    pwd: Option<PathBuf>,
}

fn main() -> anyhow::Result<()> {
    let mut options: Options = argh::from_env();

    let pwd = if let Some(pwd) = options.pwd.take() {
        pwd
    } else {
        env::current_dir()?
    };

    #[cfg(unix)]
    sek::util::set_signal_ignore();

    let mut builder = runtime::Builder::new_current_thread();

    #[cfg(unix)]
    builder.enable_io();

    let rt = builder.build()?;
    let shell = sek::shell::Shell::new(pwd, options.config.take())?;
    rt.block_on(shell.start())       
}
