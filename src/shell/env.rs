use std::{ io, env, mem };
use std::collections::HashMap;
use std::path::{ Path, PathBuf };
use std::ffi::{ OsStr, OsString };

pub struct Env(pub HashMap<OsString, OsString>);

impl Default for Env {
    fn default() -> Env {
        Env(env::vars_os().collect())
    }
}

impl Env {
    pub fn get(&self, name: &OsStr) -> Option<&OsStr> {
        self.0.get(name).map(std::ops::Deref::deref)
    }

    pub fn cd(&mut self, path: &Path) -> io::Result<()> {
        env::set_current_dir(path)?;

        let cd = env::current_dir()?;

        #[cfg(windows)]
        self.set("CD".as_ref(), cd.clone().into());

        self.set("PWD".as_ref(), cd.into());

        Ok(())
    }

    pub fn set(&mut self, name: &OsStr, val: OsString) -> Option<OsString> {
        if let Some(value) = self.0.get_mut(name) {
            Some(mem::replace(value, val))
        } else {
            self.0.insert(name.into(), val)
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
        self.0.remove(name)
    }
}
