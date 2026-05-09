use std::path::{ Path, PathBuf };
use std::ffi::OsStr;
use std::process::{ Command, Stdio };
use serde::Deserialize;
use crossterm::style::{ Color, Attributes, Attribute };
use crate::util::CowStr;
use crate::shell::env::Environment;


#[derive(Default)]
pub struct Config {
    pub theme: Theme   
}

#[derive(Deserialize, Default)]
pub struct ConfigFormat<'a> {
    #[serde(default)]
    #[serde(borrow)]
    #[serde(with = "tuple_vec_map")]
    #[serde(rename = "set-env")]
    pub set_env: Vec<(CowStr<'a>, String)>,
    #[serde(default)]
    #[serde(rename = "unset-env")]
    pub unset_env: Vec<CowStr<'a>>,
    #[serde(default)]
    #[serde(rename = "push-path")]
    pub push_path: Vec<CowStr<'a>>,
    #[serde(default)]
    pub theme: Option<Theme>
}

// TODO change color style (like alacritty ?)
#[derive(Deserialize, Default)]
pub struct Theme {
    // background color
    pub selected: Style,

    pub exe: Style,
    pub literal: Style,
    pub variable: Style,
    pub escape: Style,
    pub subshell: Style,
    pub single_str: Style,
    pub double_str: Style,
    pub chain: Style,
    pub redirect: Style,
    pub comment: Style,
    pub error: Style,
}

#[derive(Deserialize, Default, Clone, Copy)]
pub struct Style {
    ansi: Option<u8>,
    #[serde(default)]
    bold: bool,
    #[serde(default)]
    dim: bool,
    #[serde(default)]
    underlined: bool
}

impl Style {
    pub fn new(ansi: u8) -> Style {
        Style {
            ansi: Some(ansi),
            bold: false,
            dim: false,
            underlined: false
        }
    }

    pub fn color(&self) -> Option<Color> {
        self.ansi.map(Color::AnsiValue)
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
}

fn default_theme() -> Theme {
    Theme {
        selected: Style::new(10),
        
        exe: Style::new(27),
        literal: Style::new(33),
        variable: Style::new(39),
        escape: Style::new(128),
        subshell: Style::new(39),
        single_str: Style::new(3),
        double_str: Style::new(3),
        chain: Style::new(39),
        redirect: Style::new(39),
        comment: Style::new(128),
        error: Style::new(9),
    }
}

pub fn load(env: &mut Environment, config: PathBuf) -> anyhow::Result<Config> {
    let buf;
    let config = if config.exists() {
        let child = Command::new(config)
            .current_dir(env.pwd())
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()?;

        let output = child.wait_with_output()?;

        if !output.status.success() {
            return Err(anyhow::format_err!("build config failed: {}", output.status));
        }

        buf = output.stdout;
        serde_json::from_slice(&buf)?
    } else {
        ConfigFormat::default()
    };

    for (key, val) in config.set_env {
        env.set(OsStr::new(key.as_ref()), val.into());
    }

    for key in config.unset_env {
        env.unset(OsStr::new(key.as_ref()));
    }

    for path in config.push_path {
        env.push_path(Path::new(path.as_ref()))?;
    }

    Ok(Config {
        theme: config.theme.unwrap_or_else(default_theme)
    })
}
