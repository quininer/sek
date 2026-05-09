use std::io::Write;
use std::{ fs, io, env };
use std::ffi::OsStr;
use std::path::Path;
use crate::shell::env::Environment;


pub struct Cache {
    pub exe_set: ExeSet,
}

pub struct ExeSet {
    set: fst::Set<Vec<u8>>,
}

pub fn load(env: &Environment, cache_dir: &Path) -> anyhow::Result<Cache> {
    fs::create_dir_all(cache_dir)?;
    
    let exe_set = ExeSet::load(env, &cache_dir.join("exeset.fst"))?;

    //

    Ok(Cache { exe_set })
}

impl ExeSet {
    pub fn load(env: &Environment, path: &Path) -> anyhow::Result<ExeSet> {
        let mut maybe_data = fs::read(path)
            .map(Some)
            .or_else(|err| if err.kind() == io::ErrorKind::NotFound {
                Ok(None)
            } else {
                Err(err)
            })?;

        if let Some(data) = maybe_data.take() {
            match fst::Set::new(data) {
                Ok(set) => return Ok(ExeSet { set }),
                Err(_err) => {
                    fs::remove_file(path)?;

                    // log err ?
                }
            }
        }
        
        let paths = env.get(OsStr::new("PATH")).unwrap_or_default();
        let mut list = Vec::new();

        for dir in env::split_paths(paths) {
            for entry in dir.read_dir()
                .ok()
                .into_iter()
                .flatten()
                .flat_map(|entry| entry.ok())
            {
                if let Ok(metadata) = entry.metadata()
                    && metadata.is_file()
                {
                    #[cfg(unix)] {
                        use std::os::unix::fs::PermissionsExt;

                        if metadata.permissions().mode() & 0o111 != 0
                            && let Ok(name) = entry.file_name().into_string()
                        {
                            list.push(name.into_bytes());
                        }
                    }
                    
                    #[cfg(windows)] {
                        if let Some(name_without_ext) = name.strip_suffix(".exe")
                            .or_else(|| name.strip_suffix(".bat"))
                        {
                            list.push(Box::from(name_without_ext));
                        }
                    }
                }
            }
        }

        list.sort();
        list.dedup();
        list.shrink_to_fit();

        let exe_set = ExeSet { set: fst::Set::from_iter(list).unwrap() };

        let maybe_fd = fs::File::create_new(path)
            .map(Some)
            .or_else(|err| if err.kind() == io::ErrorKind::AlreadyExists {
                Ok(None)
            } else {
                Err(err)
            })?;

        if let Some(mut fd) = maybe_fd {
            fd.write_all(exe_set.set.as_fst().as_bytes())?;
        }

        Ok(exe_set)
    }

    pub fn search(&self, prefix: &str) -> impl fst::Streamer<'_, Item = &[u8]> {
        use fst::{ Automaton, IntoStreamer };

        let matcher = fst::automaton::Str::new(prefix).starts_with();
        self.set.search(matcher).into_stream()
    }

    pub fn exist(&self, name: &str) -> bool {
        self.set.contains(name)
    }
}
