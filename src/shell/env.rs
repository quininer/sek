use std::{ io, env, mem };
use std::borrow::Cow;
use std::ffi::{ OsStr, OsString };
use std::path::{ Path, PathBuf };
use std::collections::HashMap;
use anyhow::Context;
use directories::UserDirs;


pub struct Environment {
    pub max_args_len: usize,
    pub map: HashMap<OsString, OsString>,
    userdir: UserDirs,
    prev_pwd: Option<PathBuf>,
    pwd: PathBuf,
}

impl Environment {
    pub fn new(pwd: PathBuf) -> anyhow::Result<Self> {
        cfg_select! {
            target_os = "linux" => {
                let max_args_len = match unsafe { libc::sysconf(libc::_SC_ARG_MAX) } {
                    -1 => 1024 * 1024,
                    n => std::cmp::max(n as usize, 4 * 1024)
                };
            },
            windows => {
                // https://docs.microsoft.com/zh-cn/windows/win32/api/processthreadsapi/nf-processthreadsapi-createprocessa
                let max_args_len = 32767;
            },
            _ => {
                let max_args_len = 4 * 1024;
            }
        }

        Ok(Environment {
            map: env::vars_os().collect(),
            userdir: UserDirs::new()
                .context("Unable to retrieve user path from system")?,
            prev_pwd: None,
            pwd, max_args_len
        })
    }

    pub fn get(&self, name: &OsStr) -> Option<&OsStr> {
        self.map.get(name).map(std::ops::Deref::deref)
    }

    fn set_pwd(&mut self) {
        #[cfg(windows)]
        self.set("CD".as_ref(), self.pwd.clone().into());

        self.set("PWD".as_ref(), self.pwd.clone().into());
    }

    pub fn cd(&mut self, path: &Path) -> io::Result<()> {
        let newpath = self.pwd.join(path).canonicalize()?;

        if !newpath.is_dir() {
            return Err(io::Error::new(
                io::ErrorKind::NotADirectory,
                "path not a directory"
            ));
        }

        self.prev_pwd = Some(mem::replace(&mut self.pwd, newpath));
        self.set_pwd();

        Ok(())
    }

    pub fn go_home(&mut self) -> io::Result<()> {
        let home = self.userdir.home_dir().canonicalize()?;
        self.prev_pwd = Some(mem::replace(&mut self.pwd, home));
        self.set_pwd();

        Ok(())
    }

    pub fn go_back(&mut self) -> io::Result<()> {
        if let Some(pwd) = self.prev_pwd.take() {
            let pwd = pwd.canonicalize()?;
            self.prev_pwd = Some(mem::replace(&mut self.pwd, pwd));
            self.set_pwd();
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

    pub fn push_path(&mut self, val: &Path) -> anyhow::Result<()> {
        let paths = self.get("PATH".as_ref()).unwrap_or_default();
        let paths = env::split_paths(paths)
            .map(PathBuf::into_os_string)
            .map(Cow::Owned)
            .chain(Some(Cow::Borrowed(val.as_os_str())));
        let paths = env::join_paths(paths)?;

        self.set("PATH".as_ref(), paths);

        Ok(())
    }

    pub fn unset(&mut self, name: &OsStr) -> Option<OsString> {
        self.map.remove(name)
    }

    pub fn home(&self) -> &Path {
        self.userdir.home_dir()
    }

    pub fn pwd(&self) -> &Path {
        &self.pwd
    }
}
