use std::{ cmp, fmt };
use std::ops::Range;


#[derive(Default)]
pub struct EditableLine {
    buf: String,
    // empty when buf is ascii
    indices: Vec<usize>,
}

impl EditableLine {
    pub fn as_str(&self) -> &str {
        self.buf.as_str()
    }
    
    pub fn is_empty(&self) -> bool {
        self.buf.is_empty()
    }

    pub fn len(&self) -> usize {
        self.buf.len()
    }

    pub fn first(&self) -> Option<char> {
        self.buf.chars().next()
    }

    fn char_len(&self) -> usize {
        if self.indices.is_empty() {
            self.buf.len()
        } else {
            self.indices.len()
        }
    }

    fn index(&self, cur: usize) -> usize {
        match self.indices.is_empty() {
            true => cur,
            false => self.indices.get(cur)
                .copied()
                .unwrap_or(self.buf.len())
        }
    }

    fn update<F: FnOnce() -> bool>(&mut self, cur: usize, idx: usize, is_ascii: F) {
        let is_ascii = self.indices.is_empty() && is_ascii();
        
        if !is_ascii {
            if self.indices.is_empty() {
                self.indices.extend(self.buf.char_indices().map(|(offset, _)| offset));
            } else {
                self.indices.truncate(cur);
                self.indices.extend(self.buf[idx..].char_indices().map(|(offset, _)| idx + offset));
            }
        }
    }

    pub fn push(&mut self, cur: &mut usize, c: char) {
        let idx = self.index(*cur);
        self.buf.insert(idx, c);
        self.update(*cur, idx, || c.is_ascii());
        *cur += 1;
    }

    pub fn replace(&mut self, cur: usize, s: char) {
        let idx = self.index(cur);
        let next_len = self.buf[idx..]
            .chars()
            .next()
            .map(char::len_utf8)
            .unwrap_or_default();
        let mut sbuf = [0; 4];
        let sbuf = s.encode_utf8(&mut sbuf);
        self.buf.replace_range(idx..(idx + next_len), sbuf);

        if next_len != sbuf.len() {
            self.update(cur, idx, || s.is_ascii());
        }
    }

    pub fn replace_str(&mut self, range: &mut Range<usize>, s: &str) {
        let (start, end) = if range.end > range.start {
            (&mut range.start, &mut range.end)
        } else {
            (&mut range.end, &mut range.start)
        };

        let bytes_range = self.index(*start)..self.index(*end);
        self.buf.replace_range(bytes_range.clone(), s);

        self.update(*start, bytes_range.start, || s.is_ascii());
        *end = *start + s.chars().count();
    }

    pub fn push_str(&mut self, cur: &mut usize, s: &str) {
        let idx = self.index(*cur);
        self.buf.insert_str(idx, s);
        self.update(*cur, idx, || s.is_ascii());
        *cur += s.chars().count();
    }

    pub fn backspace(&mut self, cur: &mut usize) {
        if *cur != 0 {
            let idx = self.index(*cur);
            if let Some(prev_char) = self.buf[..idx].chars().last() {
                let idx = idx - prev_char.len_utf8();
                self.buf.remove(idx);
                *cur -= 1;
                self.update(*cur, idx, || true);
            }
        }
    }

    pub fn delete(&mut self, cur: usize) {
        let idx = self.index(cur);
        if self.buf.len() > idx {
            self.buf.remove(idx);
            self.update(cur, idx, || true);
        }
    }

    pub fn delete_to_end(&mut self, cur: usize) {
        let idx = self.index(cur);
        if self.buf.len() > idx {
            self.buf.truncate(idx);
            self.update(cur, idx, || true);
        }
    }

    pub fn move_head(&mut self, cur: &mut usize) {
        *cur = 0;
    }

    pub fn move_end(&mut self, cur: &mut usize) {
        *cur = self.char_len();
    }

    pub fn move_left(&mut self, cur: &mut usize) {
        *cur = cur.saturating_sub(1);
    }

    pub fn move_right(&mut self, cur: &mut usize) {
        *cur = cmp::min(*cur + 1, self.char_len());
    }

    pub fn clear(&mut self, range: &mut Range<usize>) {
        self.indices.clear();
        self.buf.clear();
        range.start = 0;
        range.end = 0;
    }

    pub fn split(&self, mid: usize) -> (&str, &str) {
        let mid = self.index(mid);
        self.buf.split_at(mid)
    }    

    pub fn split3(&self, start: usize, end: usize) -> (&str, &str, &str) {
        let start = self.index(start);
        let end = self.index(end);

        let (head, s2) = self.buf.split_at(end);
        let (s0, s1) = head.split_at(start);
        (s0, s1, s2)
    }
}

impl fmt::Display for EditableLine {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        fmt::Display::fmt(self.buf.as_str(), f)
    }
}

#[test]
fn test_buffer() {
    let mut buf = EditableLine::default();
    let mut range = 0..0;
    buf.push(&mut range.end, 'a');
    buf.push(&mut range.end, 'b');
    buf.push(&mut range.end, 'c');
    assert_eq!(format!("{}", buf), "abc");
    assert_eq!(range.end, 3);

    buf.backspace(&mut range.end);
    assert_eq!(format!("{}", buf), "ab");
    assert_eq!(range.end, 2);

    buf.delete(range.end);
    assert_eq!(format!("{}", buf), "ab");
    assert_eq!(range.end, 2);

    buf.move_left(&mut range.end);
    buf.move_left(&mut range.end);
    buf.delete(range.end);
    assert_eq!(format!("{}", buf), "b");
    assert_eq!(range.end, 0);

    buf.backspace(&mut range.end);
    assert_eq!(format!("{}", buf), "b");
    assert_eq!(range.end, 0);

    buf.move_right(&mut range.end);
    buf.backspace(&mut range.end);
    assert_eq!(format!("{}", buf), "");
    assert_eq!(range.end, 0);

    buf.push(&mut range.end, '中');
    buf.push(&mut range.end, '文');
    buf.move_left(&mut range.end);
    buf.move_left(&mut range.end);
    buf.push(&mut range.end, 'a');
    buf.push(&mut range.end, 'a');
    buf.push(&mut range.end, 'a');
    buf.move_right(&mut range.end);
    buf.move_right(&mut range.end);

    let (x, y) = buf.split(range.end);
    assert_eq!(x, "aaa中文");
    assert_eq!(y, "");

    buf.clear(&mut range);
    buf.push(&mut range.end, '中');
    buf.push(&mut range.end, '文');
    buf.move_left(&mut range.end);

    let (x, y) = buf.split(range.end);
    assert_eq!(x, "中");
    assert_eq!(y, "文");}
