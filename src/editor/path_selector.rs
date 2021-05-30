use std::mem;
use std::ops::Range;
use std::ffi::OsStr;
use std::borrow::Cow;
use std::path::{ PathBuf, Path };
use std::cmp::{ self, Ordering };
use std::fs::{ self, ReadDir, DirEntry };
use crate::util::file_name_cmp;


const MAX_ENTRY_CAP: usize = 1024;

#[derive(Debug)]
pub struct PathSelector {
    space: usize,
    path: PathBuf,
    filter: Filter,
    pub need_init: bool,
    pub parent: List,
    pub current: List,
    pub sub: List
}

#[derive(Debug)]
struct Filter {
    glob: Option<glob::Pattern>,
    skip_dot: bool,
    case_sensitive: bool
}

#[derive(Default, Debug)]
pub struct List {
    window: Range<usize>,
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
    pub fn new(space: usize) -> PathSelector {
        PathSelector {
            space,
            path: PathBuf::new(),
            filter: Filter::default(),
            need_init: false,
            parent: List::default(),
            current: List::default(),
            sub: List::default()
        }
    }

    pub fn set_space(&mut self, space: usize) {
        self.space = space;
    }

    pub fn set_glob(&mut self, glob: Option<glob::Pattern>) {
        self.filter.glob = glob;
    }

    pub fn set_skip_dot(&mut self, flag: bool) {
        self.filter.skip_dot = flag;
    }

    pub fn set_case_sensitive(&mut self, flag: bool) {
        self.filter.case_sensitive = flag;
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn cd(&mut self, path: &Path) -> anyhow::Result<()> {
        self.path.push(path);

        self.current.cd(&self.path, None, &self.filter, MAX_ENTRY_CAP)?;

        if let Some(parent) = self.path.parent() {
            self.parent.cd(parent, self.path.file_name(), &self.filter, self.space)?;
        } else {
            self.parent.clear();
        }

        if let Some(sub) = self.current.queue.get(self.current.cur) {
            self.sub.cd(&sub.entry.path(), None, &self.filter, self.space)?;
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
                self.sub.cd(&sub.entry.path(), None, &self.filter, self.space)?;
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
                self.sub.cd(&sub.entry.path(), None, &self.filter, self.space)?;
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

        self.current.fill(&self.filter)?;

        if let Some(parent) = self.path.parent() {
            self.parent.cd(parent, self.path.file_name(), &self.filter, self.space)?;
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

            self.current.fill(&self.filter)?;

            if let Some(sub) = self.current.queue.get(self.current.cur) {
                self.sub.cd(&sub.entry.path(), None, &self.filter, self.space)?;
            } else {
                self.sub.clear();
            }
        }

        Ok(())
    }
}

impl Default for Filter {
    fn default() -> Filter {
        Filter {
            glob: None,
            skip_dot: true,
            case_sensitive: true
        }
    }
}

impl Filter {
    fn matches(&self, name: &str) -> bool {
        if let Some(glob) = self.glob.as_ref() {
            glob.matches_with(name, glob::MatchOptions {
                case_sensitive: self.case_sensitive,
                require_literal_separator: true,
                require_literal_leading_dot: self.skip_dot
            })
        } else if self.skip_dot {
            !name.starts_with('.')
        } else {
            true
        }
    }
}

impl List {
    fn cd(&mut self,
        path: &Path,
        lookup: Option<&OsStr>,
        filter: &Filter,
        space: usize,
    ) -> anyhow::Result<()> {
        self.queue.clear();
        let mut readdir = path.read_dir()?;

        for entry in readdir.by_ref()
            .filter_map(Result::ok)
            .filter(|entry| {
                let file_name = entry.file_name();
                let file_name = file_name.to_string_lossy();
                filter.matches(&file_name)
            })
            .take(space)
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

    fn fill(&mut self, filter: &Filter) -> anyhow::Result<()> {
        let readdir = match self.readdir.take() {
            Some(readdir) => readdir,
            None => return Ok(())
        };

        for entry in readdir
            .filter_map(Result::ok)
            .filter(|entry| {
                let file_name = entry.file_name();
                let file_name = file_name.to_string_lossy();
                filter.matches(&file_name)
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

    pub fn take(&self, space: usize) -> impl Iterator<Item = &Entry> {
        // TODO window

        self.queue.iter()
    }
}

impl Entry {
    pub fn new(entry: DirEntry) -> anyhow::Result<Entry> {
        let ty = EntryType::from(entry.file_type()?);
        Ok(Entry { entry, ty })
    }

    pub fn name(&self) -> Cow<'_, OsStr> {
        // TODO use https://github.com/rust-lang/rust/issues/85573

        Cow::Owned(self.entry.file_name())
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
    assert_eq!(selector.parent.queue.len(), 1);
    assert_eq!(selector.current.queue.len(), 0);
    assert_eq!(selector.sub.queue.len(), 0);

    selector.left()?;
    assert_eq!(selector.path, dir.join("home").join("user"));
    assert_eq!(selector.parent.queue.len(), 1);
    assert_eq!(selector.current.queue.len(), 1);
    assert_eq!(selector.sub.queue.len(), 0);

    selector.down()?;
    assert_eq!(selector.parent.queue.len(), 1);
    assert_eq!(selector.current.queue.len(), 1);
    assert_eq!(selector.sub.queue.len(), 0);

    selector.right()?;
    assert_eq!(selector.path, dir.join("home").join("user"));

    selector.left()?;
    assert_eq!(selector.path, dir.join("home"));
    assert_eq!(selector.parent.queue.len(), 2);
    assert_eq!(selector.current.queue.len(), 1);
    assert_eq!(selector.sub.queue.len(), 1);

    Ok(())
}
