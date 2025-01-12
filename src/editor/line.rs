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
            false => self.indices[cur]
        }
    }

    fn update<F: FnOnce() -> bool>(&mut self, cur: usize, is_ascii: F) {
        let is_ascii = self.indices.is_empty() && is_ascii();
        
        if !is_ascii {
            let idx = self.indices.get(cur).copied().unwrap_or(cur);
            if self.indices.is_empty() {
                self.indices.extend(self.buf.char_indices().map(|(idx, _)| idx));
            } else {
                self.indices.truncate(cur);
                self.indices.extend(self.buf[idx..].char_indices().map(|(idx, _)| idx));
            }
        }
    }

    pub fn push(&mut self, cur: &mut usize, c: char) {
        self.buf.insert(self.index(*cur), c);
        self.update(*cur, || c.is_ascii());
        *cur += 1;
    }

    pub fn replace(&mut self, range: &mut Range<usize>, s: &str) {
        let (start, end) = if range.end > range.start {
            (&mut range.start, &mut range.end)
        } else {
            (&mut range.end, &mut range.start)
        };

        let bytes_range = self.index(*start)..self.index(*end);
        self.buf.replace_range(bytes_range.clone(), s);

        if bytes_range.len() != s.len() {
            self.update(*start, || s.is_ascii());
            *end = *start + s.chars().count();
        }
    }

    pub fn push_str(&mut self, cur: &mut usize, s: &str) {
        self.buf.insert_str(self.index(*cur), s);
        self.update(*cur, || s.is_ascii());
        *cur += s.chars().count();
    }

    pub fn backspace(&mut self, cur: &mut usize) {
        if *cur != 0 {
            let idx = self.index(*cur);
            if let Some(prev_char) = self.buf[..idx].chars().last() {
                self.buf.remove(idx - prev_char.len_utf8());
                *cur -= 1;
                self.update(*cur, || true);
            }
        }
    }

    pub fn delete(&mut self, cur: usize) {
        let idx = self.index(cur);
        if self.buf.len() > idx {
            self.buf.remove(idx);
            self.update(cur, || true);
        }
    }

    pub fn delete_to_end(&mut self, cur: usize) {
        let idx = self.index(cur);
        if self.buf.len() > idx {
            self.buf.truncate(idx);
            self.update(cur, || true);
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
    let mut cur = 0;
    buf.push(&mut cur, 'a');
    buf.push(&mut cur, 'b');
    buf.push(&mut cur, 'c');
    assert_eq!(format!("{}", buf), "abc");
    assert_eq!(cur, 3);

    buf.backspace(&mut cur);
    assert_eq!(format!("{}", buf), "ab");
    assert_eq!(cur, 2);

    buf.delete(cur);
    assert_eq!(format!("{}", buf), "ab");
    assert_eq!(cur, 2);

    buf.move_left(&mut cur);
    buf.move_left(&mut cur);
    buf.delete(cur);
    assert_eq!(format!("{}", buf), "b");
    assert_eq!(cur, 0);

    buf.backspace(&mut cur);
    assert_eq!(format!("{}", buf), "b");
    assert_eq!(cur, 0);

    buf.move_right(&mut cur);
    buf.backspace(&mut cur);
    assert_eq!(format!("{}", buf), "");
    assert_eq!(cur, 0);

    buf.push(&mut cur, '中');
    buf.push(&mut cur, '文');
}
