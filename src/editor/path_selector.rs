use std::mem;
use std::ffi::OsStr;
use std::path::{ PathBuf, Path };
use std::cmp::{ self, Ordering };
use std::fs::{ self, ReadDir, DirEntry };
use crate::util::file_name_cmp;


const MAX_ENTRY_CAP: usize = 1024;

#[derive(Debug)]
pub struct PathSelector {
    min_cap: usize,
    path: PathBuf,
    glob: Option<glob::Pattern>,
    parent: List,
    current: List,
    sub: List
}

#[derive(Default, Debug)]
pub struct List {
    cur: usize,
    queue: Vec<Entry>,
    readdir: Option<ReadDir>
}

#[derive(Debug)]
pub struct Entry {
    entry: DirEntry,
    ty: EntryType
}

#[derive(PartialEq, Eq, PartialOrd, Ord, Clone, Copy, Debug)]
pub enum EntryType {
    Symlink,
    Dir,
    File,
    Other
}

impl PathSelector {
    pub fn new(columns: u16) -> PathSelector {
        PathSelector {
            min_cap: cmp::max(columns / 2, 1) as usize,
            path: PathBuf::new(),
            glob: None,
            parent: List::default(),
            current: List::default(),
            sub: List::default()
        }
    }

    pub fn set_columns(&mut self, columns: u16) {
        self.min_cap = cmp::max(columns / 2, 1) as usize;
    }

    pub fn set_glob(&mut self, glob: Option<glob::Pattern>) {
        self.glob = glob;
    }

    pub fn cd(&mut self, path: &Path) -> anyhow::Result<()> {
        self.path.push(path);

        self.current.cd(&self.path, None, self.glob.as_ref(), MAX_ENTRY_CAP)?;

        if let Some(parent) = self.path.parent() {
            self.parent.cd(parent, self.path.file_name(), self.glob.as_ref(), self.min_cap)?;
        } else {
            self.parent.clear();
        }

        if let Some(sub) = self.current.queue.get(self.current.cur) {
            self.sub.cd(&sub.entry.path(), None, self.glob.as_ref(), self.min_cap)?;
        } else {
            self.sub.clear();
        }

        Ok(())
    }

    pub fn up(&mut self) -> anyhow::Result<()> {
        if let Some(cur) = self.current.cur.checked_sub(1) {
            self.current.cur = cur;

            if let Some(sub) = self.current.queue.get(cur)
                .filter(|sub| sub.ty == EntryType::Dir)
            {
                self.sub.cd(&sub.entry.path(), None, self.glob.as_ref(), self.min_cap)?;
            } else {
                self.sub.clear();
            }
        }

        Ok(())
    }

    pub fn down(&mut self) -> anyhow::Result<()> {
        let cur = cmp::min(self.current.cur + 1, self.current.queue.len());
        if self.current.cur != cur {
            self.current.cur = cur;

            if let Some(sub) = self.current.queue.get(cur)
                .filter(|sub| sub.ty == EntryType::Dir)
            {
                self.sub.cd(&sub.entry.path(), None, self.glob.as_ref(), self.min_cap)?;
            } else {
                self.sub.clear();
            }
        }

        Ok(())
    }

    pub fn left(&mut self) -> anyhow::Result<()> {
        if !self.path.pop() {
            return Ok(());
        }

        mem::swap(&mut self.current, &mut self.sub);
        mem::swap(&mut self.parent, &mut self.current);

        self.current.fill(self.glob.as_ref())?;

        if let Some(parent) = self.path.parent() {
            self.parent.cd(parent, self.path.file_name(), self.glob.as_ref(), self.min_cap)?;
        } else {
            self.parent.clear();
        }

        Ok(())
    }

    pub fn right(&mut self) -> anyhow::Result<()> {
        if let Some(sub) = self.current.queue.get(self.current.cur)
            .filter(|sub| sub.ty == EntryType::Dir)
        {
            let path = sub.entry.path();
            self.path.push(&path);

            mem::swap(&mut self.parent, &mut self.current);
            mem::swap(&mut self.current, &mut self.sub);

            self.current.fill(self.glob.as_ref())?;

            if let Some(sub) = self.current.queue.get(self.current.cur) {
                self.sub.cd(&sub.entry.path(), None, self.glob.as_ref(), self.min_cap)?;
            } else {
                self.sub.clear();
            }
        }

        Ok(())
    }
}

impl List {
    fn cd(&mut self,
        path: &Path,
        lookup: Option<&OsStr>,
        glob: Option<&glob::Pattern>,
        cap: usize,
    ) -> anyhow::Result<()> {
        self.queue.clear();
        let mut readdir = path.read_dir()?;

        for entry in readdir.by_ref()
            .filter_map(Result::ok)
            .filter(|entry| if let Some(glob) = glob {
                let file_name = entry.file_name();
                let file_name = file_name.to_string_lossy();
                glob.matches(&file_name)
            } else {
                true
            })
            .take(cap)
        {
            self.queue.push(Entry::new(entry)?);
        }

        self.queue.sort_by(|x, y| match Ord::cmp(&x.ty, &y.ty) {
            Ordering::Equal => file_name_cmp(&x.entry.file_name(), &y.entry.file_name()),
            ord => ord
        });

        self.cur = if let Some(name) = lookup {
            self.queue.binary_search_by(|e| file_name_cmp(&e.entry.file_name(),  name))
                .ok()
                .unwrap_or(0)
        } else {
            0
        };

        self.readdir = Some(readdir);

        Ok(())
    }

    fn fill(&mut self, glob: Option<&glob::Pattern>) -> anyhow::Result<()> {
        let readdir = match self.readdir.take() {
            Some(readdir) => readdir,
            None => return Ok(())
        };

        for entry in readdir
            .filter_map(Result::ok)
            .filter(|entry| if let Some(glob) = glob {
                let file_name = entry.file_name();
                let file_name = file_name.to_string_lossy();
                glob.matches(&file_name)
            } else {
                true
            })
            .take(MAX_ENTRY_CAP - self.queue.len())
        {
            self.queue.push(Entry::new(entry)?);
        }

        self.queue.sort_by(|x, y| match Ord::cmp(&x.ty, &y.ty) {
            Ordering::Equal =>
                file_name_cmp(&x.entry.file_name(), &y.entry.file_name()),
            ord => ord
        });

        Ok(())
    }

    fn clear(&mut self) {
        self.cur = 0;
        self.queue.clear();
        self.readdir.take();
    }
}

impl Entry {
    pub fn new(entry: DirEntry) -> anyhow::Result<Entry> {
        let ty = EntryType::from(entry.file_type()?);
        Ok(Entry { entry, ty })
    }
}

impl From<fs::FileType> for EntryType {
    fn from(ty: fs::FileType) -> EntryType {
        if ty.is_dir() {
            EntryType::Dir
        } else if ty.is_file() {
            EntryType::File
        } else if ty.is_symlink() {
            EntryType::Symlink
        } else {
            EntryType::Other
        }
    }
}

#[test]
fn test_path_selector() -> anyhow::Result<()> {
    use std::fs;

    let dir = tempfile::tempdir()?;
    let dir = dir.path();

    fs::create_dir_all(dir.join("home").join("user").join(".config"))?;
    fs::create_dir_all(dir.join("usr").join("bin"))?;
    fs::write(dir.join("home").join("user").join("sek"), "石")?;

    let mut selector = PathSelector::new(128);

    selector.cd(dir)?;
    assert_eq!(selector.path, dir);
    assert_eq!(selector.current.queue.len(), 2);
    assert_eq!(selector.sub.queue.len(), 1);

    selector.down()?;
    assert_eq!(selector.current.cur, 1);
    selector.right()?;
    assert_eq!(selector.current.cur, 0);
    assert_eq!(selector.path, dir.join("usr"));
    assert_eq!(selector.parent.queue.len(), 2);
    assert_eq!(selector.current.queue.len(), 1);
    assert_eq!(selector.sub.queue.len(), 0);

    selector.down()?;
    assert_eq!(selector.current.cur, 1);
    selector.up()?;
    assert_eq!(selector.current.cur, 0);
    selector.right()?;
    assert_eq!(selector.path, dir.join("usr").join("bin"));
    assert_eq!(selector.parent.queue.len(), 1);
    assert_eq!(selector.current.queue.len(), 0);
    assert_eq!(selector.sub.queue.len(), 0);

    selector.cd(&dir.join("home").join("user").join(".config"))?;
    assert_eq!(selector.path, dir.join("home").join("user").join(".config"));
    assert_eq!(selector.parent.queue.len(), 2);
    assert_eq!(selector.current.queue.len(), 0);
    assert_eq!(selector.sub.queue.len(), 0);

    selector.left()?;
    assert_eq!(selector.path, dir.join("home").join("user"));
    assert_eq!(selector.parent.queue.len(), 1);
    assert_eq!(selector.current.queue.len(), 2);
    assert_eq!(selector.sub.queue.len(), 0);

    selector.down()?;
    assert_eq!(selector.parent.queue.len(), 1);
    assert_eq!(selector.current.queue.len(), 2);
    assert_eq!(selector.sub.queue.len(), 0);

    selector.right()?;
    assert_eq!(selector.path, dir.join("home").join("user"));

    selector.left()?;
    assert_eq!(selector.path, dir.join("home"));
    assert_eq!(selector.parent.queue.len(), 2);
    assert_eq!(selector.current.queue.len(), 1);
    assert_eq!(selector.sub.queue.len(), 2);

    Ok(())
}
