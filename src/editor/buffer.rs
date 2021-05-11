use std::{ cmp, fmt };
use bstr::{ ByteSlice, ByteVec };
use bumpalo::collections::String;
use unicode_width::UnicodeWidthStr;
use crate::util::Fill;


#[derive(Default)]
pub struct Buffer {
    buf: Vec<char>,
    cur: usize
}

impl Buffer {
    pub fn is_empty(&self) -> bool {
        self.buf.is_empty()
    }

    pub fn len(&self) -> usize {
        self.buf.iter()
            .map(|c| c.len_utf8())
            .sum()
    }

    pub fn first(&self) -> Option<char> {
        self.buf.first().copied()
    }

    pub fn push(&mut self, c: char) {
        self.buf.insert(self.cur, c);
        self.cur += 1;
    }

    pub fn backspace(&mut self) {
        if self.cur != 0 {
            self.buf.remove(self.cur - 1);
            self.cur -= 1;
        }
    }

    pub fn delete(&mut self) {
        if self.buf.len() > self.cur {
            self.buf.remove(self.cur);
        }
    }

    pub fn move_left(&mut self) {
        self.cur = self.cur.saturating_sub(1);
    }

    pub fn move_right(&mut self) {
        self.cur = cmp::min(self.cur + 1, self.buf.len());
    }

    pub fn read_into<'a>(&self, buf: &'a mut String<'_>) {
        buf.clear();

        for &c in &self.buf {
            buf.push(c);
        }
    }

    pub fn ready_render<'a>(&self, buf: &'a mut String<'_>, fillsize: Option<u16>)
        -> (u16, Fill)
    {
        buf.clear();

        for &c in &self.buf[..self.cur] {
            buf.push(c);
        }

        let cursor_bytes = buf.len();
        let cursor = buf.width() as u16 + 1;

        for &c in &self.buf[self.cur..] {
            buf.push(c);
        }

        let len = fillsize
            .and_then(|size| {
                let strlen = cursor + buf[cursor_bytes..].width() as u16;
                size.checked_sub(strlen)
            })
            .unwrap_or(0);

        (cursor, Fill::empty(len))
    }

    pub fn clear(&mut self) {
        self.buf.clear();
        self.cur = 0;
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
