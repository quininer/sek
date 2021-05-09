pub mod env;
pub mod parser;
pub mod config;
pub mod process;

use std::io;
use std::rc::Rc;
use std::cell::RefCell;
use anyhow::Context;
use bumpalo::Bump;
use directories::UserDirs;
use crate::Global;
use crate::shell::env::Env;
use crate::shell::process::Morgue;

pub struct Shell {
    pub bump: Rc<RefCell<Bump>>,
    pub term: io::Stdout,
    pub env: Env,
    pub morgue: Morgue,
    pub userdir: UserDirs,
}

impl Shell {
    pub fn new(_global: &Global) -> anyhow::Result<Self> {
        Ok(Shell {
            bump: Rc::new(RefCell::new(Bump::new())),
            term: io::stdout(),
            env: Env::default(),
            morgue: Morgue::default(),
            userdir: UserDirs::new()
                .context("Unable to retrieve user path from system")?
        })
    }
}
