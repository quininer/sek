use std::{ io, env, mem };
use std::convert::TryInto;
use std::path::{ Path, PathBuf };
use std::ffi::{ OsStr, OsString };
use std::collections::{ HashMap, HashSet };
use anyhow::Context;
use bstr::ByteSlice;
use directories::UserDirs;
use xorf::{ Filter, Xor16 };
use crate::shell::builtin::BUILTIN_COMMANDS;
use crate::shell::config::Config;

pub struct Env {
    map: HashMap<OsString, OsString>,
    userdir: UserDirs,
    exe_filter: Option<ExeFilter>,
    prev_pwd: Option<PathBuf>,
    pwd: PathBuf,
}

struct ExeFilter {
    filter: Xor16,
    keys: (u64, u64)
}

impl Env {
    pub fn new(config: &Config) -> anyhow::Result<Env> {
        let map: HashMap<_, _> = env::vars_os().collect();
        let pwd = env::current_dir()?;

        let exe_filter = if let Some(paths) = map.get(OsStr::new("PATH")) {
            Some(ExeFilter::new(paths, config)?)
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

    fn set_pwd(&mut self) {
        #[cfg(windows)]
        self.set("CD".as_ref(), self.pwd.clone().into());

        self.set("PWD".as_ref(), self.pwd.clone().into());
    }

    pub fn cd(&mut self, path: &Path) -> io::Result<()> {
        env::set_current_dir(path)?;

        self.prev_pwd =
            Some(mem::replace(&mut self.pwd, env::current_dir()?));
        self.set_pwd();

        Ok(())
    }

    pub fn go_home(&mut self) -> io::Result<()> {
        self.prev_pwd =
            Some(mem::replace(&mut self.pwd, self.userdir.home_dir().into()));

        env::set_current_dir(&self.pwd)?;
        self.set_pwd();

        Ok(())
    }

    pub fn go_back(&mut self) -> io::Result<()> {
        if let Some(pwd) = self.prev_pwd.take() {
            self.prev_pwd = Some(mem::replace(&mut self.pwd, pwd));

            env::set_current_dir(&self.pwd)?;
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

    pub fn pwd(&self) -> &Path {
        &self.pwd
    }

    pub fn exists(&self, name: &[u8]) -> bool {
        self.exe_filter
            .as_ref()
            .map(|filter| filter.exists(name))
            .unwrap_or(true)
    }

    pub fn as_map(&self) -> &HashMap<OsString, OsString> {
        &self.map
    }
}

impl ExeFilter {
    fn new(paths: &OsStr, config: &Config) -> anyhow::Result<ExeFilter> {
        let mut keybuf = [0; 16];
        getrandom::getrandom(&mut keybuf)?;
        let key0 = u64::from_le_bytes(keybuf[..8].try_into()?);
        let key1 = u64::from_le_bytes(keybuf[8..].try_into()?);

        let mut exeset: HashSet<u64> = HashSet::with_capacity(256);

        #[cfg(windows)]
        let path_exts = {
            use std::borrow::Cow;

            let path_exts = env::var_os("PATHEXT")
                .map(Cow::Owned)
                .unwrap_or(Cow::Borrowed(OsStr::new(env::consts::EXE_EXTENSION)));

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
                        exeset.insert(hash((key0, key1), name));
                    }
                }
            }

            #[cfg(windows)] {
                if path.extension()
                    .map(|ext| path_exts.iter().any(|ext2| ext.eq_ignore_ascii_case(ext2)))
                    .unwrap_or(false)
                {
                    if let Some(name) = path.file_stem()
                        .and_then(<[u8]>::from_os_str)
                    {
                        exeset.insert(hash((key0, key1), name));
                    }
                }
            }
        }

        for (name, _) in BUILTIN_COMMANDS {
            exeset.insert(hash((key0, key1), name.as_bytes()));
        }

        for name in config.alias.keys() {
            exeset.insert(hash((key0, key1), name.as_bytes()));
        }

        let exelist = exeset.into_iter().collect::<Vec<_>>();

        Ok(ExeFilter {
            filter: Xor16::from(&exelist),
            keys: (key0, key1)
        })
    }

    fn exists(&self, name: &[u8]) -> bool {
        let val = hash(self.keys, name);
        self.filter.contains(&val)
    }
}

fn hash(keys: (u64, u64), name: &[u8]) -> u64 {
    use siphasher::sip::SipHasher;
    use std::hash::Hasher;

    let mut hasher = SipHasher::new_with_keys(keys.0, keys.1);
    hasher.write(name);
    hasher.finish()
}
