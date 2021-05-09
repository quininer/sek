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
    theme: Option<Theme>
}

#[derive(Deserialize)]
pub struct Theme {
    exe: u8,
    literal: u8,
    env: u8,
    subshell: u8,
    single_str: u8,
    double_str: u8,
    pipe: u8,
    redirect: u8,
    error: u8
}

impl Default for Theme {
    fn default() -> Theme {
        todo!()
    }
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
        shell.env.set(&key, val.into_owned());
    }

    for key in config.unset_env {
        shell.env.remove(&key);
    }

    for path in config.push_path {
        shell.env.push_path(path.into_owned())?;
    }

    if let Some(theme) = config.theme {
        shell.theme = theme;
    }

    Ok(())
}
