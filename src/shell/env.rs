use std::{ io, env, mem };
use std::collections::{ HashMap, HashSet };
use std::path::{ Path, PathBuf };
use std::ffi::{ OsStr, OsString };
use anyhow::Context;
use bstr::ByteSlice;
use directories::UserDirs;
use xorf::{ Filter, Xor8 };
use crate::util::hash;

pub struct Env {
    map: HashMap<OsString, OsString>,
    userdir: UserDirs,
    prev_pwd: Option<PathBuf>,
    pwd: PathBuf,
    exe_filter: Option<Xor8>
}

impl Env {
    pub fn new() -> anyhow::Result<Env> {
        let map: HashMap<_, _> = env::vars_os().collect();
        let pwd = env::current_dir()?;

        let exe_filter = if let Some(paths) = map.get(OsStr::new("PATH")) {
            let mut exeset: HashSet<Vec<u8>> = HashSet::with_capacity(256);

            #[cfg(windows)]
            let path_exts = {
                let path_exts = map.get(OsStr::new("PATHEXT"))
                    .map(|val| &**val)
                    .unwrap_or(OsStr::new(env::consts::EXE_EXTENSION));

                env::split_paths(&path_exts)
                    .map(PathBuf::into_os_string)
                    .collect::<Vec<_>>()
            };

            for path in env::split_paths(paths)
                .filter_map(|path| path.read_dir().ok())
                .flatten()
                .filter_map(Result::ok)
                .map(|entry| entry.path())
            {
                let metadata = match path.metadata() {
                    Ok(e) => e,
                    Err(_) => continue
                };

                if !metadata.is_file() {
                    continue;
                }

                #[cfg(unix)] {
                    use std::os::unix::fs::PermissionsExt;

                    if metadata.permissions().mode() & 0o111 != 0 {
                        if let Some(name) = path.file_stem()
                            .and_then(<[u8]>::from_os_str)
                        {
                            exeset.insert(name.into());
                        }
                    }
                }

                #[cfg(windows)] {
                    if path.extension()
                        .filter(|ext| path_exts.iter().any(|ext2| ext.eq_ignore_ascii_case(ext2)))
                        .is_some()
                    {
                        if let Some(name) = path.file_stem()
                            .and_then(<[u8]>::from_os_str)
                        {
                            exeset.insert(name.into());
                        }
                    }
                }
            }

            let exelist = exeset.into_iter()
                .map(|exe| hash(&exe))
                .collect::<Vec<_>>();

            Some(Xor8::from(&exelist))
        } else {
            None
        };

        Ok(Env {
            map, pwd, exe_filter,
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

    pub fn filter(&self, name: &[u8]) -> bool {
        self.exe_filter
            .as_ref()
            .filter(|filter| filter.contains(&hash(name)))
            .is_some()
    }

    pub fn as_map(&self) -> &HashMap<OsString, OsString> {
        &self.map
    }
}
