use std::{ fs, io, fmt };
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
    pub exe: Style,
    pub literal: Style,
    pub env: Style,
    pub subshell: Style,
    pub single_str: Style,
    pub double_str: Style,
    pub pipe: Style,
    pub redirect: Style,
    pub error: Style
}

#[derive(Deserialize)]
pub struct Style {
    ansi: u8,
    #[serde(default)]
    bold: bool,
    #[serde(default)]
    dim: bool,
    #[serde(default)]
    underlined: bool
}

impl Default for Theme {
    fn default() -> Theme {
        Theme {
            exe: Style::new(0x37),
            literal: Style::new(0x37),
            env: Style::new(0x37),
            subshell: Style::new(0x37),
            single_str: Style::new(0x37),
            double_str: Style::new(0x37),
            pipe: Style::new(0x37),
            redirect: Style::new(0x37),
            error: Style::new(0x37),
        }
    }
}

impl Style {
    pub fn new(ansi: u8) -> Style {
        Style {
            ansi,
            bold: false,
            dim: false,
            underlined: false
        }
    }

    pub fn print<D, W>(&self, val: D, mut term: W) -> anyhow::Result<()>
    where
        D: fmt::Display + Clone,
        W: io::Write
    {
        use crossterm::queue;
        use crossterm::style::{ style, Color, Attribute, PrintStyledContent };

        let mut val = style(val).with(Color::AnsiValue(self.ansi));

        if self.bold {
            val = val.attribute(Attribute::Bold);
        }

        if self.dim {
            val = val.attribute(Attribute::Dim);
        }

        if self.underlined {
            val = val.attribute(Attribute::Underlined);
        }

        queue!(term, PrintStyledContent(val))?;

        Ok(())
    }
}

pub async fn load(global: &Global, shell: &mut Shell) -> anyhow::Result<()> {
    let path = global.projdir.config_dir().join("config");
    let path2 = global.projdir.config_dir().join("config.json");

    let buf = if path.exists() {
        let child = Command::new(path)
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()?;

        let output = child.wait_with_output().await?;

        if !output.status.success() {
            return Err(anyhow::format_err!("bad exit status: {}", output.status));
        }

        output.stdout
    } else if path.exists() {
        fs::read(path2)?
    } else {
        return Ok(());
    };

    let config: Config = serde_json::from_slice(&buf)?;

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
