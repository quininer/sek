use argh::FromArgs;
use std::path::PathBuf;
use tokio::runtime;

/// The Sek Shell
#[derive(FromArgs)]
struct Options {
    /// use specified config
    #[argh(option, short = 'c')]
    config: Option<PathBuf>,
}

fn main() -> anyhow::Result<()> {
    let mut options: Options = argh::from_env();

    #[cfg(unix)]
    {
        sek::util::setup_signal_handler()?;
    }

    let mut builder = runtime::Builder::new_current_thread();

    #[cfg(unix)]
    builder.enable_io();

    let rt = builder.build()?;
    let shell = sek::shell::Shell::new(options.config.take())?;
    rt.block_on(shell.start())
}
