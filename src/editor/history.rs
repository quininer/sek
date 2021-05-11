use std::fs;
use std::io::Write;
use bstr::{ BStr, ByteSlice };
use serde::{ Serialize, Deserialize };
use time::OffsetDateTime;
use intrusive_collections::{ intrusive_adapter, KeyAdapter };
use intrusive_collections::xor_linked_list::{ self, XorLinkedList as LinkedList };
use intrusive_collections::rbtree::{ self, RBTree as RbTree };
use crate::Global;
use crate::util::append_max;


pub struct History<'g> {
    fd: fs::File,

    list: LinkedList<ListAdapter<'g, &'g HistoryLine<'g>>>,
    tree: RbTree<TreeAdapter<'g, &'g HistoryLine<'g>>>,
    buf: Vec<u8>
}

pub struct HistoryCursor<'a, 'g> {
    history: &'a History<'g>,
    cursor: CursorState<'a, 'g>,
    count: usize
}

enum CursorState<'a, 'g> {
    None,
    Empty(xor_linked_list::Cursor<'a, ListAdapter<'g, &'g HistoryLine<'g>>>),
    Query(rbtree::Cursor<'a, TreeAdapter<'g, &'g HistoryLine<'g>>>)
}

pub struct Value<T: Copy + ?Sized> {
    list_link: xor_linked_list::Link,
    tree_link: rbtree::Link,
    value: T
}

#[derive(Serialize, Deserialize)]
pub struct HistoryLine<'g> {
    #[serde(borrow)]
    pub cmd: &'g BStr,
    pub when: OffsetDateTime,
    pub session: u64
}

impl<'g> History<'g> {
    pub fn from_fd(global: &'g Global) -> anyhow::Result<Self> {
        let path = global.projdir.data_local_dir().join("history");
        let fd = fs::OpenOptions::new()
            .read(true)
            .append(true)
            .create(true)
            .open(&path)?;

        let mut list: LinkedList<ListAdapter<'_, &'_ HistoryLine<'_>>> =
            LinkedList::new(ListAdapter::NEW);
        let mut tree = RbTree::new(TreeAdapter::NEW);

        if let Some(buf) = fd.metadata()
            .ok()
            .filter(|metadata| metadata.len() != 0)
            .and_then(|_| unsafe { memmap2::Mmap::map(&fd).ok() })
        {
            let iter = serde_cbor::Deserializer::from_slice(&buf)
                .into_iter::<HistoryLine>();

            for line in iter {
                let line = match line {
                    Ok(line) => line,
                    Err(_) => continue
                };

                // skip continuous command
                if let Some(last) = list.back().get() {
                    if last.value.cmd == line.cmd {
                        continue
                    }
                }

                let cmd = global.bump.alloc_slice_copy(line.cmd.as_ref());
                let line = HistoryLine {
                    cmd: cmd.as_bstr(),
                    ..line
                };
                let line: &_ = global.bump.alloc(line);
                let val = global.bump.alloc(Value::new(line));

                list.push_back(val);
                tree.insert(val);
            }
        }

        Ok(History {
            fd, list, tree,
            buf: Vec::new()
        })
    }

    pub fn cursor<'a>(&'a self) -> HistoryCursor<'a, 'g> {
        HistoryCursor {
            history: self,
            cursor: CursorState::None,
            count: 0
        }
    }

    pub fn push(&mut self, global: &'g Global, line: &'g HistoryLine<'g>) -> anyhow::Result<()> {
        let value = global.bump.alloc(Value::new(line));
        self.list.push_back(value);
        self.tree.insert(value);

        self.buf.clear();
        serde_cbor::to_writer(&mut self.buf, line)?;

        if self.buf.len() <= append_max() {
            self.fd.write(&self.buf)?;
        }

        Ok(())
    }
}

impl<'a, 'g> HistoryCursor<'a, 'g> {
    pub fn lookup(&mut self, line: &str) {
        if line.is_empty() {
            self.cursor = CursorState::None;
        } else {
            self.cursor =
                CursorState::Query(self.history.tree.find(line.as_bytes().as_bstr()));
        }
    }

    pub fn is_last(&self, line: &str) -> bool {
        self.history.list
            .back()
            .get()
            .filter(|value| value.value.cmd == line.as_bytes())
            .is_some()
    }

    pub fn as_line(&self) -> Option<&'g BStr> {
        match &self.cursor {
            CursorState::None => None,
            CursorState::Empty(cursor) => cursor.get().map(|val| val.value.cmd),
            CursorState::Query(cursor) => cursor.get().map(|val| val.value.cmd)
        }
    }

    pub fn up(&mut self) {
        match &mut self.cursor {
            CursorState::None => self.cursor = CursorState::Empty(self.history.list.back()),
            CursorState::Empty(cursor) => cursor.move_prev(),
            CursorState::Query(cursor) => cursor.move_prev(),
        }

        self.count += 1;
    }

    pub fn down(&mut self) {
        self.count = self.count.checked_sub(1)
            .unwrap_or_default();

        match &mut self.cursor {
            CursorState::None => (),
            CursorState::Empty(cursor) => cursor.move_next(),
            CursorState::Query(cursor) => cursor.move_next(),
        }
    }

}

intrusive_adapter! {
    pub ListAdapter<'a, T> =
        &'a Value<T>: Value<T> {
            list_link: xor_linked_list::Link
        }
        where
            T: Copy + ?Sized + 'a
}

intrusive_adapter! {
    pub TreeAdapter<'a, T> =
        &'a Value<T>: Value<T> {
            tree_link: rbtree::Link
        }
        where
            T: Copy + ?Sized + 'a
}

impl<'a, 'g> KeyAdapter<'a> for TreeAdapter<'g, &'g HistoryLine<'g>> {
    type Key = &'g BStr;

    fn get_key(&self, value: &'a Value<&'g HistoryLine<'g>>) -> Self::Key {
        value.value.cmd
    }
}

impl<T> Value<T>
where
    T: Copy + ?Sized
{
    pub fn new(value: T) -> Value<T> {
        Value {
            list_link: xor_linked_list::Link::new(),
            tree_link: rbtree::Link::new(),
            value
        }
    }
}
