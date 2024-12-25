pub mod shell;

use std::path::PathBuf;
use argh::FromArgs;


/// The Sek Shell
#[derive(FromArgs)]
struct Options {
    /// use specified config
    #[argh(option, short = 'c')]
    config: Option<PathBuf>,

    /// use specified pwd
    #[argh(option, short = 'p')]
    pwd: Option<PathBuf>,

    /// print version
    #[argh(switch, short = 'v')]
    version: bool
}

fn main() -> anyhow::Result<()> {
    let mut options: Options = argh::from_env();

    if options.version {
        println!("{}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }

    if let Some(pwd) = options.pwd.take() {
        std::env::set_current_dir(pwd)?;
    }

    todo!()
}
