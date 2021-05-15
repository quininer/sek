use std::{ fs, io, fmt };
use std::ffi::OsStr;
use std::path::Path;
use std::borrow::Cow;
use std::process::Stdio;
use serde::Deserialize;
use tokio::process::Command;
use crossterm::style::{ style, Color, Attribute, Attributes };
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
    pub escape: Style,
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
        const DEFAULT_COLOR: u8 = 0xff;

        Theme {
            exe: Style::new(DEFAULT_COLOR),
            literal: Style::new(DEFAULT_COLOR),
            env: Style::new(DEFAULT_COLOR),
            escape: Style::new(DEFAULT_COLOR),
            subshell: Style::new(DEFAULT_COLOR),
            single_str: Style::new(3),
            double_str: Style::new(3),
            pipe: Style::new(DEFAULT_COLOR),
            redirect: Style::new(DEFAULT_COLOR),
            error: Style::new(9),
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

    pub fn color(&self) -> Color {
        Color::AnsiValue(self.ansi)
    }

    pub fn attr(&self) -> Option<Attributes> {
        let mut attr = Attributes::default();
        if self.bold {
            attr.set(Attribute::Bold);
        }
        if self.dim {
            attr.set(Attribute::Dim);
        }
        if self.underlined {
            attr.set(Attribute::Underlined);
        }
        if attr != Attributes::default() {
            Some(attr)
        } else {
            None
        }
    }

    pub fn push<W: io::Write>(&self, mut term: W) -> anyhow::Result<()> {
        use crossterm::{ queue, style };

        queue!(term, style::SetForegroundColor(self.color()))?;
        if let Some(attr) = self.attr() {
            queue!(term, style::SetAttributes(attr))?;
        }
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
    } else if path2.exists() {
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
