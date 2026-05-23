use std::ffi::OsStr;
use std::ops::Range;
use std::collections::HashMap;
use std::path::{ Path, PathBuf };
use std::process::{ Command, Stdio };
use bstr::BString;
use serde::Deserialize;
use crossterm::style::{ Color, Attributes, Attribute };
use crate::util::CowStr;
use crate::shell::env::Environment;


pub struct Config {
    pub theme: Theme,
    pub prompt: Option<Prompt>,
    pub alias: AliasMap,
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
    pub theme: Theme,
    #[serde(default)]
    pub prompt: Option<Prompt>,
    #[serde(default)]
    #[serde(with = "tuple_vec_map")]
    pub alias: Vec<(String, Vec<String>)>,
}

// TODO change color style (like alacritty ?)
#[derive(Deserialize)]
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
    rgb: Option<[u8; 3]>,
    #[serde(default)]
    bold: bool,
    #[serde(default)]
    dim: bool,
    #[serde(default)]
    underlined: bool
}

impl Style {
    pub fn ansi(ansi: u8) -> Style {
        Style {
            ansi: Some(ansi),
            rgb: None,
            bold: false,
            dim: false,
            underlined: false
        }
    }

    pub fn rgb(r: u8, g: u8, b: u8) -> Style {
        Style {
            ansi: None,
            rgb: Some([r, g, b]),
            bold: false,
            dim: false,
            underlined: false
        }
    }    

    pub fn color(&self) -> Option<Color> {
        self.rgb.map(|[r, g, b]| Color::Rgb { r, g, b })
            .or_else(|| self.ansi.map(Color::AnsiValue))
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

impl Default for Theme {
    fn default() -> Self {
        Theme {
            selected: Style::ansi(251),
        
            exe: Style::ansi(27),
            literal: Style::ansi(33),
            variable: Style::ansi(39),
            escape: Style::ansi(128),
            subshell: Style::ansi(39),
            single_str: Style::ansi(3),
            double_str: Style::ansi(3),
            chain: Style::ansi(39),
            redirect: Style::ansi(39),
            comment: Style::ansi(128),
            error: Style::ansi(9),
        }
    }
}

#[derive(Deserialize)]
pub struct Prompt {
    pub exe: String,
    pub args: Vec<String>,
}

pub struct AliasMap {
    list: Vec<String>,
    map: HashMap<BString, Range<usize>>,
}

impl AliasMap {
    pub fn get<'a>(&'a self, exe: &[u8]) -> Option<(&'a String, &'a [String])> {
        let range = self.map.get(exe).cloned()?;
        let args = self.list.get(range)?;
        args.split_first()
    }

    pub fn iter(&self) -> impl Iterator<Item = &BString> {
        self.map.keys()
    }
}

pub fn load(env: &mut Environment, config: PathBuf) -> anyhow::Result<Config> {
    let buf;
    let config = if config.is_file() {
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

    let aliasmap = {
        let mut list = Vec::new();
        let mut map = HashMap::with_capacity(config.alias.len());
        for (k, v) in config.alias {
            if v.is_empty() {
                continue
            }
            
            let start = list.len();
            list.extend(v);
            let end = list.len();
            map.insert(k.into(), start..end);
        }
        list.shrink_to_fit();
        AliasMap { list, map }
    };

    Ok(Config {
        theme: config.theme,
        prompt: config.prompt,
        alias: aliasmap,
    })
}
