use std::{ cmp, fmt };
use std::path::Path;
use bumpalo::collections::String;
use unicode_width::UnicodeWidthStr;
use crate::editor::history::History;


#[derive(Default)]
pub struct Buffer {
    buf: Vec<char>,
    cur: usize,
    pub history: History
}

impl Buffer {
    pub fn is_empty(&self) -> bool {
        self.history.get()
            .map(|line| line.is_empty())
            .unwrap_or_else(|| self.buf.is_empty())
    }

    pub fn len(&self) -> usize {
        if let Some(line) = self.history.get() {
            line.len()
        } else {
            self.buf.iter()
                .map(|c| c.len_utf8())
                .sum()
        }
    }

    pub fn first(&self) -> Option<char> {
        self.buf.first().copied()
    }

    pub fn push(&mut self, c: char) {
        self.make();
        self.buf.insert(self.cur, c);
        self.cur += 1;
    }

    pub fn insert_path(&mut self, path: &Path) {
        use bstr::{ ByteVec, ByteSlice };

        self.make();

        let path = Vec::from_path_lossy(path);

        for c in path.chars() {
            self.buf.insert(self.cur, c);
            self.cur += 1;
        }
    }

    pub fn backspace(&mut self) {
        self.make();
        if self.cur != 0 {
            self.buf.remove(self.cur - 1);
            self.cur -= 1;
        }
    }

    pub fn delete(&mut self) {
        self.make();
        if self.buf.len() > self.cur {
            self.buf.remove(self.cur);
        }
    }

    pub fn move_head(&mut self) {
        self.make();
        self.cur = 0;
    }

    pub fn move_end(&mut self) {
        self.make();
        self.cur = self.buf.len();
    }

    pub fn move_left(&mut self) {
        self.make();
        self.cur = self.cur.saturating_sub(1);
    }

    pub fn move_right(&mut self) {
        self.make();
        self.cur = cmp::min(self.cur + 1, self.buf.len());
    }

    fn make(&mut self) {
        if let Some(buf) = self.history.get() {
            self.buf.clear();
            self.buf.extend(buf.chars());
            self.history.reset();
            self.cur = self.buf.len();
        }
    }

    pub fn read_into<'a>(&self, buf: &'a mut String<'_>) {
        buf.clear();

        if let Some(line) = self.history.get() {
            buf.push_str(line);
        } else {
            for &c in &self.buf {
                buf.push(c);
            }
        }
    }

    pub fn read_into_and_width<'a>(&self, buf: &'a mut String<'_>)
        -> (u16, u16)
    {
        buf.clear();

        if let Some(line) = self.history.get() {
            buf.push_str(line);
            let width = buf.width() as u16 + 1;

            (width, width)
        } else {
            for &c in &self.buf[..self.cur] {
                buf.push(c);
            }

            let cursor_bytes = buf.len();
            let cursor_width = buf.width() as u16 + 1;

            for &c in &self.buf[self.cur..] {
                buf.push(c);
            }

            let all_width = cursor_width + buf[cursor_bytes..].width() as u16;

            (cursor_width, all_width)
        }
    }

    pub fn clear(&mut self) {
        self.buf.clear();
        self.cur = 0;
        self.history.reset();
    }
}

impl fmt::Display for Buffer {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        for &c in &self.buf {
            write!(f, "{}", c)?;
        }

        Ok(())
    }
}

#[test]
fn test_buffer() {
    let mut buf = Buffer::default();
    buf.push('a');
    buf.push('b');
    buf.push('c');
    assert_eq!(format!("{}", buf), "abc");
    assert_eq!(buf.cur, 3);

    buf.backspace();
    assert_eq!(format!("{}", buf), "ab");
    assert_eq!(buf.cur, 2);

    buf.delete();
    assert_eq!(format!("{}", buf), "ab");
    assert_eq!(buf.cur, 2);

    buf.move_left();
    buf.move_left();
    buf.delete();
    assert_eq!(format!("{}", buf), "b");
    assert_eq!(buf.cur, 0);

    buf.backspace();
    assert_eq!(format!("{}", buf), "b");
    assert_eq!(buf.cur, 0);

    buf.move_right();
    buf.backspace();
    assert_eq!(format!("{}", buf), "");
    assert_eq!(buf.cur, 0);
}
