use std::{ cmp, fmt };
use std::ops::Range;
use std::collections::VecDeque;
use icu_segmenter::{ WordSegmenter, WordSegmenterBorrowed };
use crate::util::MapWindows2;


pub struct EditableLine {
    line: Line,
    list: VecDeque<Line>,
    // empty when buf is ascii
    indices: Vec<usize>,
    current: usize,
    segmenter: WordSegmenterBorrowed<'static>,
}

#[derive(Default, Clone)]
pub struct Line {
    buf: String,
    cursor: Range<usize>,
}

const MAX_HISTORY: usize = 1024;

impl Default for EditableLine {
    fn default() -> Self {
        EditableLine {
            line: Line::default(),
            list: VecDeque::new(),
            indices: Vec::new(),
            current: 0,
            segmenter: WordSegmenter::new_auto(Default::default()),
        }
    }
}

impl EditableLine {
    pub fn as_str(&self) -> &str {
        self.line().buf.as_str()
    }
    
    pub fn is_empty(&self) -> bool {
        self.as_str().is_empty()
    }

    pub fn bytes_len(&self) -> usize {
        self.as_str().len()
    }

    pub fn char_len(&self) -> usize {
        if self.indices.is_empty() {
            self.bytes_len()
        } else {
            self.indices.len()
        }
    }

    pub fn first(&self) -> Option<char> {
        self.as_str().chars().next()
    }

    pub fn inclusive(&self, cursor: Range<usize>) -> Range<usize> {
        let (start, end) = if cursor.end > cursor.start {
            (cursor.start, cursor.end)
        } else {
            (cursor.end, cursor.start)
        };
        start..cmp::min(end + 1, self.char_len())
    }

    pub fn span(&self, cursor: Range<usize>) -> Range<usize> {
        let start = self.index(cursor.start);
        let end = self.index(cursor.end);
        if start <= end {
            start..end
        } else {
            end..start
        }
    }

    fn current(&self) -> usize {
        self.list.len() - self.current
    }

    fn line(&self) -> &Line {
        self.list.get(self.current()).unwrap_or(&self.line)
    }

    fn line_mut(&mut self) -> &mut Line {
        self.list.get_mut(self.current()).unwrap_or(&mut self.line)
    }

    fn index(&self, cur: usize) -> usize {
        match self.indices.is_empty() {
            true => cur,
            false => self.indices.get(cur)
                .copied()
                .unwrap_or(self.bytes_len())
        }
    }

    fn update<F: FnOnce() -> bool>(&mut self, cur: usize, idx: usize, is_ascii: F) {
        let is_ascii = self.indices.is_empty() && is_ascii();
        
        if !is_ascii {
            let buf = &self.list.get(self.current()).unwrap_or(&self.line).buf;
            if self.indices.is_empty() {
                self.indices.extend(buf.char_indices().map(|(offset, _)| offset));
            } else {
                self.indices.truncate(cur);
                self.indices.extend(buf[idx..].char_indices().map(|(offset, _)| idx + offset));
            }
        }
    }

    pub fn push(&mut self, _cur: &mut usize, c: char) {
        let cur = self.line().cursor.end;
        let idx = self.index(cur);
        self.line_mut().buf.insert(idx, c);
        self.update(cur, idx, || c.is_ascii());
        self.line_mut().cursor.end += 1;
    }

    pub fn replace(&mut self, _cur: usize, s: char) {
        let cur = self.line().cursor.end;
        let idx = self.index(cur);
        let next_len = self.line()
            .buf[idx..]
            .chars()
            .next()
            .map(char::len_utf8)
            .unwrap_or_default();
        let mut sbuf = [0; 4];
        let sbuf = s.encode_utf8(&mut sbuf);
        self.line_mut()
            .buf
            .replace_range(idx..(idx + next_len), sbuf);

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
        self.line_mut()
            .buf
            .replace_range(bytes_range.clone(), s);

        self.update(*start, bytes_range.start, || s.is_ascii());
        *end = *start + s.chars().count();
    }

    pub fn push_str(&mut self, _cur: &mut usize, s: &str) {
        let cur = self.line().cursor.end;
        let idx = self.index(cur);
        self.line_mut()
            .buf
            .insert_str(idx, s);
        self.update(cur, idx, || s.is_ascii());
        self.line_mut().cursor.end += s.chars().count();
    }

    pub fn backspace(&mut self, _cur: &mut usize) {
        let cur = self.line().cursor.end;
        if cur != 0 {
            let idx = self.index(cur);
            if let Some(prev_char) = self.as_str()[..idx].chars().last() {
                let idx = idx - prev_char.len_utf8();
                self.line_mut()
                    .buf
                    .remove(idx);
                self.line_mut().cursor.end -= 1;
                self.update(cur - 1, idx, || true);
            }
        }
    }

    pub fn delete(&mut self, _cur: usize) {
        let cur = self.line().cursor.end;
        let idx = self.index(cur);
        if self.bytes_len() > idx {
            self.line_mut().buf.remove(idx);
            self.update(cur, idx, || true);
        }
    }

    pub fn delete_to_end(&mut self, _cur: usize) {
        let cur = self.line().cursor.end;
        let idx = self.index(cur);
        if self.bytes_len() > idx {
            self.line_mut().buf.truncate(idx);
            self.update(cur, idx, || true);
        }
    }

    pub fn move_head(&mut self, _cur: &mut usize) {
        self.line_mut().cursor.end = 0;
    }

    pub fn move_end(&mut self, _cur: &mut usize) {
        self.line_mut().cursor.end = self.char_len();
    }

    pub fn move_left(&mut self, _cur: &mut usize) {
        let cur = &mut self.line_mut().cursor.end;
        *cur = cur.saturating_sub(1);
    }

    pub fn move_right(&mut self, _cur: &mut usize) {
        let len = self.char_len();
        let cur = &mut self.line_mut().cursor.end;
        *cur = cmp::min(*cur + 1, len);
    }

    pub fn move_left_word(&self, _cur: usize) -> Range<usize> {
        let cur = self.line().cursor.end;
        let idx = self.index(cur);
        let buf = &self.as_str()[..idx];

        let iter = self.segmenter.segment_str(buf);
        let iter = MapWindows2::new(iter, |&[start, end]| start..end);

        if let Some(span) = iter.last() {
            let start = buf[..span.start].chars().count();
            let end = start + buf[span].chars().count();
            start..end
        } else {
            cur..cur
        }
    }

    pub fn move_right_word(&self, _cur: usize) -> Range<usize> {
        let cur = self.line().cursor.end;
        let idx = self.index(cur);
        let buf = &self.as_str()[idx..];

        let iter = self.segmenter.segment_str(buf);
        let mut iter = MapWindows2::new(iter, |&[start, end]| start..end);

        if let Some(span) = iter.next() {
            let start = cur + buf[..span.start].chars().count();
            let end = start + buf[span].chars().count();
            start..end
        } else {
            cur..cur
        }
    }

    pub fn clear(&mut self) {
        self.indices.clear();
        self.line_mut().cursor = 0..0;
        self.line_mut().buf.clear();
    }

    pub fn up(&mut self) {
        self.current = std::cmp::min(self.current + 1, self.list.len());
        let is_ascii = self.as_str().is_ascii();
        self.update(0, 0, || is_ascii);
    }

    pub fn down(&mut self) {
        self.current = self.current.saturating_sub(1);
        let is_ascii = self.as_str().is_ascii();
        self.update(0, 0, || is_ascii);
    }

    pub fn submit(&mut self) {
        if let Some(line) = self.list.get(self.current()) {
            self.line.buf.clear();
            self.line.buf.push_str(&line.buf);
            self.line.cursor = line.cursor.clone();
            let is_ascii = self.line.buf.is_ascii();
            self.update(0, 0, || is_ascii);
        }
                
        if self.list.back().map(|line| line.buf.as_str()) == Some(&self.line.buf) {
            // TODO cursor

            self.current = 0;
            return
        }
        
        let line = if self.list.len() >= MAX_HISTORY {
            self.list.pop_front()
        } else {
            None
        };
        let line = if let Some(mut line) = line {
            line.buf.clear();
            line.buf.push_str(&self.line.buf);
            self.line.cursor = line.cursor.clone();
            line
        } else {
            self.line.clone()
        };

        self.list.push_back(line);
        self.current = 0;
    }

    pub fn split(&self, mid: usize) -> (&str, &str) {
        let mid = self.index(mid);
        self.as_str().split_at(mid)
    }    
}

impl fmt::Display for EditableLine {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        fmt::Display::fmt(self.as_str(), f)
    }
}

#[test]
fn test_buffer() {
    let mut buf = EditableLine::default();
    let mut range = 0..0;
    buf.push(&mut range.end, 'a');
    buf.push(&mut range.end, 'b');
    buf.push(&mut range.end, 'c');
    assert_eq!(buf.as_str(), "abc");
    assert_eq!(range.end, 3);

    buf.backspace(&mut range.end);
    assert_eq!(buf.as_str(), "ab");
    assert_eq!(range.end, 2);

    buf.delete(range.end);
    assert_eq!(buf.as_str(), "ab");
    assert_eq!(range.end, 2);

    buf.move_left(&mut range.end);
    buf.move_left(&mut range.end);
    buf.delete(range.end);
    assert_eq!(buf.as_str(), "b");
    assert_eq!(range.end, 0);

    buf.backspace(&mut range.end);
    assert_eq!(buf.as_str(), "b");
    assert_eq!(range.end, 0);

    buf.move_right(&mut range.end);
    buf.backspace(&mut range.end);
    assert_eq!(buf.as_str(), "");
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

    buf.clear();
    range = 0..0;
    buf.push(&mut range.end, '中');
    buf.push(&mut range.end, '文');
    buf.move_left(&mut range.end);

    let (x, y) = buf.split(range.end);
    assert_eq!(x, "中");
    assert_eq!(y, "文");

    buf.push_str(&mut range.end, "hello world");
    buf.move_head(&mut range.end);
    range.start = range.end;

    range = buf.move_right_word(range.end);
    assert_eq!(&buf.as_str()[buf.span(range.clone())], "中");
    range = buf.move_right_word(range.end);
    assert_eq!(&buf.as_str()[buf.span(range.clone())], "hello");
    range = buf.move_right_word(range.end);
    assert_eq!(&buf.as_str()[buf.span(range.clone())], " ");
    range = buf.move_right_word(range.end);
    assert_eq!(&buf.as_str()[buf.span(range.clone())], "world");
    range = buf.move_right_word(range.end);
    assert_eq!(&buf.as_str()[buf.span(range.clone())], "文");

    range.start = range.end;
    range = buf.move_left_word(range.start);
    assert_eq!(&buf.as_str()[buf.span(range.clone())], "文");
    range = buf.move_left_word(range.start);
    assert_eq!(&buf.as_str()[buf.span(range.clone())], "world");
}
