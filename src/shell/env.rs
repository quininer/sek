use std::{ io, env, mem };
use std::collections::HashMap;
use std::path::{ Path, PathBuf };
use std::ffi::{ OsStr, OsString };
use anyhow::Context;
use directories::UserDirs;

pub struct Env {
    map: HashMap<OsString, OsString>,
    userdir: UserDirs,
    prev_pwd: Option<PathBuf>,
    pwd: PathBuf,
}

impl Env {
    pub fn new() -> anyhow::Result<Env> {
        let map = env::vars_os().collect();
        let pwd = env::current_dir()?;
        Ok(Env {
            map, pwd,
            userdir: UserDirs::new()
                .context("Unable to retrieve user path from system")?,
            prev_pwd: None
        })
    }

    pub fn get(&self, name: &OsStr) -> Option<&OsStr> {
        self.map.get(name).map(std::ops::Deref::deref)
    }

    pub fn cd(&mut self, path: &Path) -> io::Result<()> {
        env::set_current_dir(path)?;

        self.prev_pwd =
            Some(mem::replace(&mut self.pwd, env::current_dir()?));

        #[cfg(windows)]
        self.set("CD".as_ref(), self.pwd.clone().into());

        self.set("PWD".as_ref(), self.pwd.clone().into());

        Ok(())
    }

    pub fn go_home(&mut self) -> io::Result<()> {
        self.prev_pwd =
            Some(mem::replace(&mut self.pwd, self.userdir.home_dir().into()));

        env::set_current_dir(&self.pwd)?;

        #[cfg(windows)]
        self.set("CD".as_ref(), self.pwd.clone().into());

        self.set("PWD".as_ref(), self.pwd.clone().into());

        Ok(())
    }

    pub fn go_back(&mut self) -> io::Result<()> {
        if let Some(pwd) = self.prev_pwd.take() {
            self.prev_pwd = Some(mem::replace(&mut self.pwd, pwd));

            env::set_current_dir(&self.pwd)?;

            #[cfg(windows)]
            self.set("CD".as_ref(), self.pwd.clone().into());

            self.set("PWD".as_ref(), self.pwd.clone().into());
        }

        Ok(())
    }

    pub fn set(&mut self, name: &OsStr, val: OsString) -> Option<OsString> {
        if let Some(value) = self.map.get_mut(name) {
            Some(mem::replace(value, val))
        } else {
            self.map.insert(name.into(), val)
        }
    }

    pub fn push_path(&mut self, val: PathBuf) -> anyhow::Result<()> {
        let paths = self.get("PATH".as_ref()).unwrap_or_default();
        let paths = env::split_paths(paths).chain(Some(val));
        let paths = env::join_paths(paths)?;

        env::set_var("PATH", &paths);
        self.set("PATH".as_ref(), paths);

        Ok(())
    }

    pub fn remove(&mut self, name: &OsStr) -> Option<OsString> {
        self.map.remove(name)
    }

    pub fn home(&self) -> &Path {
        self.userdir.home_dir()
    }

    pub fn as_map(&self) -> &HashMap<OsString, OsString> {
        &self.map
    }
}
