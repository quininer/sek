use std::{ fs, io, env };
use std::io::Write;
use std::ffi::OsStr;
use std::path::Path;
use crate::config::Config;
use crate::shell::env::Environment;


pub struct Cache {
    pub exe_set: ExeSet,
}

pub struct ExeSet {
    set: fst::Set<Vec<u8>>,
}

pub fn load(config: &Config, env: &Environment)
    -> anyhow::Result<Cache>
{
    let cache_dir = env.projdir.cache_dir();

    fs::create_dir_all(cache_dir)?;
    
    let exe_set = ExeSet::load(config, env, &cache_dir.join("exeset.fst"))?;

    //

    Ok(Cache { exe_set })
}

impl ExeSet {
    fn load(config: &Config, env: &Environment, path: &Path)
        -> anyhow::Result<ExeSet>
    {
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
                    // log err ?
                    fs::remove_file(path)?;
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
                let Ok(mut metadata) = entry.metadata()
                    else {
                        continue
                    };
                if metadata.is_symlink()
                    && let path = entry.path()
                    && let Ok(symlink_metadata) = fs::metadata(&path)
                {
                    metadata = symlink_metadata;
                }
                
                if metadata.is_file() {
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

        list.extend(config.alias.keys().map(|a| a.to_vec()));
        list.sort();
        list.dedup();

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

    pub fn search<'a>(&'a self, prefix: &'a str)
        -> impl Iterator<Item = Box<[u8]>> + 'a
    {
        use fst::{ Automaton, IntoStreamer, Streamer };
        use fst::automaton::{ StartsWith, Str };

        struct SearchResult<'a>(fst::set::Stream<'a, StartsWith<Str<'a>>>);

        impl<'a> Iterator for SearchResult<'a> {
            type Item = Box<[u8]>;

            fn next(&mut self) -> Option<Self::Item> {
                self.0.next().map(Box::from)
            }
        }

        let matcher = Str::new(prefix).starts_with();
        SearchResult(self.set.search(matcher).into_stream())
    }

    pub fn exist(&self, name: &str) -> bool {
        self.set.contains(name)
    }
}
