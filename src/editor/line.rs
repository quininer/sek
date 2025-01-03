use std::{ cmp, fmt };

use logos::Source;


#[derive(Default)]
pub struct EditableLine {
    buf: String,
    // empty when buf is ascii
    indices: Vec<usize>,
    cur: usize,
}

impl EditableLine {
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

    fn index(&self) -> usize {
        match self.indices.is_empty() {
            true => self.cur,
            false => self.indices[self.cur]
        }
    }

    fn update<F: FnOnce() -> bool>(&mut self, is_ascii: F) {
        let is_ascii = self.indices.is_empty() && is_ascii();
        
        if !is_ascii {
            let idx = self.indices[self.cur];
            self.indices.truncate(self.cur);
            self.indices.extend(self.buf[idx..].char_indices().map(|(idx, _)| idx));
        }
    }

    pub fn push(&mut self, c: char) {
        self.buf.insert(self.index(), c);
        self.update(|| c.is_ascii());
        self.cur += 1;
    }    

    pub fn replace(&mut self, c: char) {
        let idx = self.index();

        if let Some(cc) = self.buf[idx..].chars().next() {
            let mut buf = [0; 4];
            let c = c.encode_utf8(&mut buf);
            let cc_len = cc.len_utf8();

            self.buf.replace_range(idx..(idx + cc_len), c);

            if cc_len != c.len() {
                self.update(|| c.is_ascii());
            }
        }
    }

    pub fn push_str(&mut self, s: &str) {
        self.buf.insert_str(self.index(), s);
        self.update(|| s.is_ascii());
        self.cur += s.chars().count();
    }

    pub fn backspace(&mut self) {
        if self.cur != 0 {
            let idx = self.index();
            if let Some(prev_char) = self.buf[..idx].chars().last() {
                self.buf.remove(idx - prev_char.len_utf8());
                self.cur -= 1;
                self.update(|| true);
            }
        }
    }

    pub fn delete(&mut self) {
        let idx = self.index();
        if self.buf.len() > idx {
            self.buf.remove(idx);
            self.update(|| true);
        }
    }

    pub fn delete_to_end(&mut self) {
        let idx = self.index();
        if self.buf.len() > idx {
            self.buf.truncate(idx);
            self.update(|| true);
            self.cur = self.cur.saturating_sub(1);
        }
    }

    pub fn move_head(&mut self) {
        self.cur = 0;
    }

    pub fn move_end(&mut self) {
        self.cur = self.char_len();
    }

    pub fn move_left(&mut self) {
        self.cur = self.cur.saturating_sub(1);
    }

    pub fn move_right(&mut self) {
        self.cur = cmp::min(self.cur + 1, self.char_len());
    }

    pub fn clear(&mut self) {
        self.buf.clear();
        self.cur = 0;
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
