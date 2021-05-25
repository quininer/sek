use std::ffi::OsStr;
use std::path::Path;
use std::borrow::Cow;
use std::process::Stdio;
use std::collections::HashMap;
use serde::Deserialize;
use tokio::process::Command;
use bumpalo::collections::String as BumpString;
use crossterm::style::{ Color, Attribute, Attributes };
use crate::shell::Shell;


#[derive(Deserialize, Default)]
pub struct Config<'a> {
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

#[derive(Deserialize)]
pub struct Theme {
    pub exe: Style,
    pub literal: Style,
    pub env: Style,
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

#[derive(Default)]
pub struct AliasMap(HashMap<String, String>);

impl Default for Theme {
    fn default() -> Theme {
        Theme {
            exe: Style::new(27),
            literal: Style::new(33),
            env: Style::new(39),
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

impl AliasMap {
    pub fn replace(&self, buf: &mut BumpString<'_>) {
        use logos::Logos;
        use crate::shell::parser::Token;

        let mut lex = Token::lexer(&buf);
        while let Some(token) = lex.next() {
            match token {
                Token::Text => break,
                Token::Empty => (),
                _ => return
            }
        }

        let span = lex.span();
        let exe = &buf[span.clone()];

        if let Some(newexe) = self.0.get(exe) {
            buf.replace_range(span, newexe);
        }
    }
}

pub async fn load(path: &Path) -> anyhow::Result<Shell> {
    use std::io;
    use std::rc::Rc;
    use std::cell::RefCell;
    use bumpalo::Bump;
    use crate::util::arg_max;
    use crate::shell::env::Env;
    use crate::shell::process::Morgue;

    let config = if path.exists() {
        let child = Command::new(path)
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()?;

        let output = child.wait_with_output().await?;

        if !output.status.success() {
            return Err(anyhow::format_err!("bad exit status: {}", output.status));
        }

        serde_json::from_slice(&output.stdout)?
    } else {
        Config::default()
    };

    let mut env = Env::new(&config)?;

    for (key, val) in config.set_env {
        env.set(OsStr::new(&*key), val.into());
    }

    for key in config.unset_env {
        env.remove(OsStr::new(&*key));
    }

    for path in config.push_path {
        env.push_path(path.into_owned())?;
    }

    Ok(Shell {
        env,
        bump: Rc::new(RefCell::new(Bump::new())),
        term: io::stdout(),
        theme: config.theme.unwrap_or_default(),
        alias: AliasMap(config.alias),
        arg_max: arg_max(),
        morgue: Morgue::default(),
        last_status: true
    })
}
