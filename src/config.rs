use std::path::Path;
use std::borrow::Cow;
use std::collections::HashMap;
use serde::Deserialize;
use crossterm::style::{ Color, Attributes, Attribute };


#[derive(Default)]
pub struct Config {
    pub alias: HashMap<String, String>,
    pub theme: Theme   
}

#[derive(Deserialize, Default)]
pub struct ConfigFormat<'a> {
    #[serde(default)]
    #[serde(with = "tuple_vec_map")]
    #[serde(rename = "set-env")]
    pub set_env: Vec<(Cow<'a, str>, String)>,
    #[serde(default)]
    #[serde(rename = "unset-env")]
    pub unset_env: Vec<Cow<'a, str>>,
    #[serde(default)]
    #[serde(rename = "push-path")]
    pub push_path: Vec<Cow<'a, Path>>,
    #[serde(default)]
    pub alias: HashMap<String, String>,
    pub theme: Option<Theme>
}

#[derive(Deserialize, Default)]
pub struct Theme {
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
    pub error: Style
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

pub fn default_theme() -> Theme {
    Theme {
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
