use std::io;
use std::ffi::OsStr;
use std::path::Path;
use std::borrow::Cow;
use std::process::Stdio;
use serde::Deserialize;
use tokio::process::Command;
use crate::Global;
use crate::shell::Shell;


#[derive(Deserialize)]
struct Config<'a> {
    #[serde(with = "tuple_vec_map")]
    set_env: Vec<(Cow<'a, OsStr>, Cow<'a, OsStr>)>,
    unset_env: Vec<Cow<'a, OsStr>>,
    push_path: Vec<Cow<'a, Path>>,
}

pub async fn load(global: &Global, shell: &mut Shell) -> anyhow::Result<()> {
    let path = global.projdir.config_dir().join("config");

    if !path.exists() {
        return Ok(());
    }

    let child = Command::new(path)
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()?;

    let output = child.wait_with_output().await?;

    if !output.status.success() {
        return Err(anyhow::format_err!("bad exit status: {}", output.status));
    }

    let config: Config = serde_json::from_slice(&output.stdout)?;

    for (key, val) in config.set_env {
        shell.env.set(&key, val.into());
    }

    for key in config.unset_env {
        shell.env.remove(&key);
    }

    for path in config.push_path {
        shell.env.push_path(path.into_owned())?;
    }

    Ok(())
}
