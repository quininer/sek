use std::env;
use std::path::PathBuf;
use argh::FromArgs;


/// The Sek Shell
#[derive(FromArgs)]
struct Options {
    /// use specified pwd
    #[argh(option, short = 'p')]
    pwd: Option<PathBuf>
}

fn main() -> anyhow::Result<()> {
    let options: Options = argh::from_env();

    if let Some(pwd) = options.pwd {
        env::set_current_dir(pwd)?;
    }
    
    Ok(())
}
