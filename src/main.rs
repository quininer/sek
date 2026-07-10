use std::path::PathBuf;
use argh::FromArgs;

/// The Sek Shell
#[derive(FromArgs)]
struct Options {
    /// use specified config
    #[argh(option, short = 'c')]
    config: Option<PathBuf>,
}

fn main() -> anyhow::Result<()> {
    let mut options: Options = argh::from_env();

    sek::start_shell(options.config.take())
}
