use std::{ io, env, mem };
use std::borrow::Cow;
use std::ffi::{ OsStr, OsString };
use std::path::{ Path, PathBuf };
use anyhow::Context;
use directories::{ UserDirs, ProjectDirs };
use crate::util::path;


pub struct Environment {
    pub max_args_len: usize,
    pub map: Vec<(OsString, OsString)>,
    pub projdir: ProjectDirs,
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

        let mut map: Vec<(OsString, OsString)> = env::vars_os().collect();
        map.sort_by(|(x, _), (y, _)| x.cmp(y));

        Ok(Environment {
            projdir: ProjectDirs::from("", "", env!("CARGO_PKG_NAME"))
                .context("Unable to retrieve project path from system")?,
            userdir: UserDirs::new()
                .context("Unable to retrieve user path from system")?,
            prev_pwd: None,
            map, pwd, max_args_len
        })
    }

    pub fn get(&self, name: &OsStr) -> Option<&OsStr> {
        self.map.binary_search_by(|(k, _)| k.as_os_str().cmp(name))
            .map(|idx| self.map[idx].1.as_os_str())
            .ok()
    }

    pub fn search(&self, prefix: &OsStr) -> impl Iterator<Item = (&OsStr, &OsStr)> {
        let idx = self.map.partition_point(|(k, _)| k < prefix);
        self.map.get(idx..)
            .into_iter()
            .flatten()
            .take_while(|(k, _)| k.as_encoded_bytes().starts_with(prefix.as_encoded_bytes()))
            .map(|(k, v)| (k.as_os_str(), v.as_os_str()))
    }

    fn set_pwd(&mut self) {
        #[cfg(windows)]
        self.set("CD".as_ref(), self.pwd.clone().into());

        self.set("PWD".as_ref(), self.pwd.clone().into());
    }

    pub fn cd(&mut self, path: &Path) -> io::Result<()> {
        let newpath = path::dir(&self.pwd.join(path))?;
        self.prev_pwd = Some(mem::replace(&mut self.pwd, newpath));
        self.set_pwd();
        Ok(())
    }

    pub fn go_home(&mut self) -> io::Result<()> {
        let newpath = path::dir(self.userdir.home_dir())?;
        self.prev_pwd = Some(mem::replace(&mut self.pwd, newpath));
        self.set_pwd();
        Ok(())
    }

    pub fn go_back(&mut self) -> io::Result<()> {
        if let Some(pwd) = self.prev_pwd.take() {
            let pwd = path::dir(&pwd)?;
            self.prev_pwd = Some(mem::replace(&mut self.pwd, pwd));
            self.set_pwd();
        }

        Ok(())
    }

    pub fn set(&mut self, name: &OsStr, val: OsString) -> Option<OsString> {
        match self.map.binary_search_by(|(k, _)| k.as_os_str().cmp(name)) {
            Ok(idx) => Some(mem::replace(&mut self.map[idx].1, val)),
            Err(idx) => {
                self.map.insert(idx, (name.into(), val));
                None
            }
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
        self.map.binary_search_by(|(k, _)| k.as_os_str().cmp(name))
            .map(|idx| self.map.remove(idx).1)
            .ok()
    }

    pub fn home(&self) -> &Path {
        self.userdir.home_dir()
    }

    pub fn pwd(&self) -> &Path {
        &self.pwd
    }

    pub fn shrink_to_fit(&mut self) {
        self.map.shrink_to_fit();
    }
}
