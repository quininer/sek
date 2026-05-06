use std::mem;
use std::ops::Range;
use std::ffi::OsStr;
use std::borrow::Cow;
use std::path::{ PathBuf, Path };
use std::cmp::{ self, Ordering };
use std::fs::{ self, ReadDir, DirEntry };
use bstr::{ ByteVec, ByteSlice };
use icu_collator::Collator;
use crate::util::path::file_name_cmp;


const MAX_ENTRY_CAP: usize = 1024;

#[derive(Debug)]
pub struct PathSelector {
    collator: Collator,
    space: usize,
    path: PathBuf,
    filter: Filter,
    pub search: String,
    pub parent: List,
    pub current: List,
    pub children: List
}

#[derive(Debug)]
struct Filter {
    glob: Option<glob::Pattern>,
    hidden_file: bool,
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
    Dir,
    File,
    Other
}

impl PathSelector {
    pub fn new() -> anyhow::Result<PathSelector> {
        let mut prefs = icu_collator::CollatorPreferences::default();
        prefs.numeric_ordering =
            Some(icu_collator::preferences::CollationNumericOrdering::True);
        let collator = Collator::try_new(prefs, Default::default())?;
        
        Ok(PathSelector {
            collator: collator.static_to_owned(),
            space: 0,
            path: PathBuf::new(),
            filter: Filter::default(),
            search: String::new(),
            parent: List::default(),
            current: List::default(),
            children: List::default()
        })
    }

    pub fn set_space(&mut self, space: usize) {
        self.space = space;
    }

    pub fn set_glob(&mut self, glob: Option<glob::Pattern>) {
        self.filter.glob = glob;
    }

    pub fn toggle_hidden_file(&mut self) {
        self.filter.hidden_file = !self.filter.hidden_file;
    }

    pub fn toggle_case_sensitive(&mut self) {
        self.filter.case_sensitive = !self.filter.case_sensitive;
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn selected(&self) -> Option<PathBuf> {
        self.current.queue.get(self.current.cur).map(|entry| entry.path())
    }

    pub fn clear(&mut self) {
        self.path.clear();
        self.filter = Filter::default();
        self.parent.clear();
        self.current.clear();
        self.children.clear();
    }

    pub fn cd(&mut self, path: &Path) -> anyhow::Result<()> {
        if path != Path::new(".") {
            self.path.push(path);
        }

        let filename = if path == Path::new(".") {
            self.current.get().map(|entry| entry.name().into_owned())
        } else {
            self.current.clear();
            None
        };

        self.current.cd(
            &self.collator,
            &self.path,
            filename.as_deref(),
            &self.filter,
            self.space
        )?;

        if let Some(parent) = self.path.parent() {
            self.parent.cd(
                &self.collator,
                parent,
                self.path.file_name(),
                &self.filter,
                self.space
            )?;
        } else {
            self.parent.clear();
        }

        self.update_children()?;

        Ok(())
    }

    pub fn up(&mut self) -> anyhow::Result<()> {
        if let Some(cur) = self.current.cur.checked_sub(1) {
            self.current.cur = cur;

            if !self.current.window.contains(&cur)
                && let Some(start) = self.current.window.start.checked_sub(1)
            {
                self.current.window.start = start;
                self.current.window.end = self.current.window.end.saturating_sub(1);
            }

            self.update_children()?;
        }

        Ok(())
    }

    pub fn down(&mut self) -> anyhow::Result<()> {
        let cur = cmp::min(self.current.cur + 1, self.current.queue.len().saturating_sub(1));
        if self.current.cur != cur {
            self.current.cur = cur;

            if !self.current.window.contains(&cur) {
                self.current.window.start += 1;
                self.current.window.end += 1;
            }

            self.update_children()?;
        }

        Ok(())
    }

    pub fn left(&mut self) -> anyhow::Result<()> {
        if !self.path.pop() {
            return Ok(());
        }

        mem::swap(&mut self.current, &mut self.children);
        mem::swap(&mut self.parent, &mut self.current);

        self.current.fill(&self.collator, &self.filter)?;

        if let Some(parent) = self.path.parent() {
            self.parent.cd(&self.collator, parent, self.path.file_name(), &self.filter, self.space)?;
        } else {
            self.parent.clear();
        }

        Ok(())
    }

    pub fn right(&mut self) -> anyhow::Result<()> {
        if let Some(children) = self.current.queue.get(self.current.cur)
            .filter(|children| children.ty == EntryType::Dir)
        {
            let path = children.entry.path();
            self.path.push(&path);

            mem::swap(&mut self.parent, &mut self.current);
            mem::swap(&mut self.current, &mut self.children);

            self.current.fill(&self.collator, &self.filter)?;

            self.update_children()?;
        }

        Ok(())
    }

    pub fn update_children(&mut self) -> anyhow::Result<()> {
        if let Some(children) = self.current.queue.get(self.current.cur)
            .filter(|children| children.ty == EntryType::Dir)
        {
            self.children.cd(
                &self.collator,
                &children.entry.path(),
                None,
                &self.filter,
                self.space
            )?;
        } else {
            self.children.clear();
        }

        Ok(())
    }
}

impl Default for Filter {
    fn default() -> Filter {
        Filter {
            glob: None,
            hidden_file: true,
            case_sensitive: false
        }
    }
}

impl Filter {
    fn matches(&self, entry: &DirEntry) -> bool {
        if let Some(glob) = self.glob.as_ref() {
            let name = entry.file_name();
            let name = Path::new(&name);

            glob.matches_path_with(name, glob::MatchOptions {
                case_sensitive: self.case_sensitive,
                require_literal_separator: true,
                require_literal_leading_dot: self.hidden_file
            })
        } else if self.hidden_file {
            #[cfg(unix)] {
                let name = entry.file_name();
                !name.as_encoded_bytes().starts_with_str(".")
            }

            #[cfg(windows)] {
                use std::os::windows::fs::MetadataExt;

                const FILE_ATTRIBUTE_HIDDEN: u32 = 0x2;

                entry.metadata()
                    .ok()
                    .map(|metadata| metadata.file_attributes())
                    .filter(|attr| attr & FILE_ATTRIBUTE_HIDDEN != 0)
                    .is_none()
            }
        } else {
            true
        }
    }
}

impl List {
    fn cd(&mut self,
        collator: &Collator,
        path: &Path,
        lookup: Option<&OsStr>,
        filter: &Filter,
        space: usize,
    ) -> anyhow::Result<()> {
        self.queue.clear();
        let mut readdir = path.read_dir()?;

        for entry in readdir.by_ref()
            .filter_map(Result::ok)
            .filter(|entry| filter.matches(entry))
            .take(MAX_ENTRY_CAP)
        {
            if let Ok(entry) = Entry::new(entry) {
                self.queue.push(entry);
            }
        }

        self.queue.sort_by(|x, y| match Ord::cmp(&x.ty, &y.ty) {
            Ordering::Equal => file_name_cmp(collator, &x.name(), &y.name()),
            ord => ord
        });

        self.cur = lookup
            .and_then(|name| self.queue.iter()
                .position(|e| e.name() == name)
            )
            .unwrap_or_default();

        self.readdir = Some(readdir);
        self.update_window(space);

        Ok(())
    }

    fn fill(&mut self, collator: &Collator, filter: &Filter) -> anyhow::Result<()> {
        let readdir = match self.readdir.take() {
            Some(readdir) => readdir,
            None => return Ok(())
        };

        for entry in readdir
            .filter_map(Result::ok)
            .filter(|entry| filter.matches(entry))
            .take(MAX_ENTRY_CAP - self.queue.len())
        {
            self.queue.push(Entry::new(entry)?);
        }

        self.queue.sort_by(|x, y| match Ord::cmp(&x.ty, &y.ty) {
            Ordering::Equal =>
                file_name_cmp(collator, &x.entry.file_name(), &y.entry.file_name()),
            ord => ord
        });

        Ok(())
    }

    fn update_window(&mut self, space: usize) {
        if !self.window.contains(&self.cur) || self.window.len() != space {
            self.window = if let Some(start) = self.cur.checked_sub(space) {
                (start + 1)..(self.cur + 1)
            } else {
                0..space
            };
        }
    }

    fn clear(&mut self) {
        self.cur = 0;
        self.queue.clear();
        self.readdir.take();
    }

    pub fn len(&self) -> usize {
        self.queue.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn get(&self) -> Option<&Entry> {
        self.queue.get(self.cur)
    }

    pub fn take(&self, space: usize) -> impl Iterator<Item = (bool, &Entry)> {
        self.queue.iter()
            .enumerate()
            .skip(self.window.start)
            .map(move |(i, entry)| (i == self.cur, entry))
            .take(self.window.len())
            .take(space)
    }

    pub fn search_up(&mut self, needle: &str) -> bool {
        let prev_cur = self.cur;
        if let Some((cur, _)) = self.queue.iter()
            .enumerate()
            .take(self.cur)
            .rev()
            .find(|(_, e)| e.name()
                .as_encoded_bytes()
                .find(needle.as_bytes())
                .is_some()
            )
        {
            self.cur = cur;

            let space = self.window.len();
            self.update_window(space);
        }

        prev_cur != self.cur
    }

    pub fn search_down(&mut self, needle: &str) -> bool {
        let prev_cur = self.cur;
        if let Some((cur, _)) = self.queue.iter()
            .enumerate()
            .skip(self.cur)
            .skip(1)
            .find(|(_, e)| {
                let name = e.name();
                let name = Vec::from_os_str_lossy(&name);
                name.find(needle.as_bytes()).is_some()
            })
        {
            self.cur = cur;

            let space = self.window.len();
            self.update_window(space);
        }

        prev_cur != self.cur
    }

    pub fn move_to_top(&mut self) {
        self.cur = 0;
    }

    pub fn move_to_bottom(&mut self) {
        self.cur = self.queue.len().saturating_sub(1);
    }
}

impl Entry {
    pub fn new(entry: DirEntry) -> anyhow::Result<Entry> {
        let mut ty = entry.file_type()?;

        if ty.is_symlink() {
            ty = fs::metadata(entry.path())?.file_type();
        }

        let ty = if ty.is_dir() {
            EntryType::Dir
        } else if ty.is_file() {
            EntryType::File
        } else {
            EntryType::Other
        };

        Ok(Entry { entry, ty })
    }

    pub fn path(&self) -> PathBuf {
        self.entry.path()
    }

    pub fn type_(&self) -> EntryType {
        self.ty
    }

    pub fn name(&self) -> Cow<'_, OsStr> {
        // TODO use https://github.com/rust-lang/rust/issues/85573

        Cow::Owned(self.entry.file_name())
    }
}

#[test]
fn test_path_selector() -> anyhow::Result<()> {
    use std::fs;
    use std::hash::{ RandomState, BuildHasher };
    use crate::util::ScopeGuard;

    let dir = {
        let tmpfs = std::env::temp_dir();
        let rand = RandomState::new().hash_one(0x42);
        tmpfs.join(rand.to_string())
    };
    let dir = ScopeGuard(dir, |dir| {
        let _ = fs::remove_dir_all(dir);
    });
    let dir = dir.as_ref().as_path();

    fs::create_dir_all(dir.join("home").join("user").join(".config"))?;
    fs::create_dir_all(dir.join("usr").join("bin"))?;
    fs::write(dir.join("home").join("user").join("sek"), "石")?;

    let mut selector = PathSelector::new()?;
    selector.set_space(128);

    selector.cd(dir)?;
    assert_eq!(selector.path, dir);
    assert_eq!(selector.current.queue.len(), 2);
    assert_eq!(selector.children.queue.len(), 1);

    selector.down()?;
    assert_eq!(selector.current.cur, 1);
    selector.right()?;
    assert_eq!(selector.current.cur, 0);
    assert_eq!(selector.path, dir.join("usr"));
    assert_eq!(selector.parent.queue.len(), 2);
    assert_eq!(selector.current.queue.len(), 1);
    assert_eq!(selector.children.queue.len(), 0);

    selector.down()?;
    assert_eq!(selector.current.cur, 0);
    selector.up()?;
    assert_eq!(selector.current.cur, 0);
    selector.right()?;
    assert_eq!(selector.path, dir.join("usr").join("bin"));
    assert_eq!(selector.parent.queue.len(), 1);
    assert_eq!(selector.current.queue.len(), 0);
    assert_eq!(selector.children.queue.len(), 0);

    selector.cd(&dir.join("home").join("user").join(".config"))?;
    assert_eq!(selector.path, dir.join("home").join("user").join(".config"));
    assert_eq!(selector.parent.queue.len(), 1);
    assert_eq!(selector.current.queue.len(), 0);
    assert_eq!(selector.children.queue.len(), 0);

    selector.left()?;
    assert_eq!(selector.path, dir.join("home").join("user"));
    assert_eq!(selector.parent.queue.len(), 1);
    assert_eq!(selector.current.queue.len(), 1);
    assert_eq!(selector.children.queue.len(), 0);

    selector.down()?;
    assert_eq!(selector.parent.queue.len(), 1);
    assert_eq!(selector.current.queue.len(), 1);
    assert_eq!(selector.children.queue.len(), 0);

    selector.right()?;
    assert_eq!(selector.path, dir.join("home").join("user"));

    selector.left()?;
    assert_eq!(selector.path, dir.join("home"));
    assert_eq!(selector.parent.queue.len(), 2);
    assert_eq!(selector.current.queue.len(), 1);
    assert_eq!(selector.children.queue.len(), 1);

    Ok(())
}
