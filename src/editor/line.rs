use std::{ cmp, fmt };
use super::selection::Range;
use icu_segmenter::{ WordSegmenter, WordSegmenterBorrowed };
use crate::util::MapWindows2;
use crate::ipc;


pub struct EditableLine {
    history: Vec<Line>,
    segmenter: WordSegmenterBorrowed<'static>,
    line: Line,
    // empty when buf is ascii
    indices: Vec<usize>,
    current: usize,
}

#[derive(Clone)]
pub struct Line {
    buf: String,
    cursor: Range,
}

impl Default for Line {
    fn default() -> Self {
        Line {
            buf: String::new(),
            cursor: Range::point(0),
        }
    }
}

#[derive(Default)]
pub struct Suggestion {
    buf: String,
    kind: SuggestionKind
}

#[derive(Default)]
enum SuggestionKind {
    #[default]
    Nothing,
    Requested(ipc::RequestId),
    Suffix,
    History(usize),
}

const MAX_HISTORY: usize = 1024;

impl Default for EditableLine {
    fn default() -> Self {
        EditableLine {
            segmenter: WordSegmenter::new_auto(Default::default()),
            line: Line::default(),
            history: Vec::new(),
            indices: Vec::new(),
            current: 0,
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

    /// The range the text operations act on, a zero width cursor covers the
    /// char at its head.
    pub fn text_range(&self) -> Range {
        self.line().cursor.min_width_1(self.char_len())
    }

    pub fn span(&self, cursor: Range) -> std::ops::Range<usize> {
        let start = self.index(cursor.from());
        let end = self.index(cursor.to());

        start..end
    }

    pub fn is_editing(&self) -> bool {
        self.current == 0
    }

    pub fn is_point_end(&self) -> bool {
        let end = self.line().cursor.head;
        end == self.char_len()
    }

    /// The insert position of the cursor.
    pub fn point(&self) -> usize {
        self.line().cursor.head
    }

    pub fn cursor(&self) -> Range {
        self.line().cursor
    }

    pub fn cursor_mut(&mut self) -> &mut Range {
        &mut self.line_mut().cursor
    }

    /// Collapse the cursor onto its head.
    pub fn collapse(&mut self) {
        let head = self.line_mut().cursor.head;
        self.line_mut().cursor = Range::point(head);
    }

    pub fn select_span(&mut self, span: std::ops::Range<usize>) {
        let start = self.as_str()[..span.start].chars().count();
        let end = start + self.as_str()[span].chars().count();
        *self.cursor_mut() = Range::new(start, end)
    }

    pub fn selected(&self) -> &str {
        let span = self.span(self.text_range());
        &self.as_str()[span]
    }

    fn line(&self) -> &Line {
        self.history.get(self.current()).unwrap_or(&self.line)
    }

    fn line_mut(&mut self) -> &mut Line {
        let idx = self.current();
        self.history.get_mut(idx).unwrap_or(&mut self.line)
    }

    fn current(&self) -> usize {
        self.history.len() - self.current
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
            let buf = &self.history.get(self.current()).unwrap_or(&self.line).buf;
            if self.indices.is_empty() {
                self.indices.extend(buf.char_indices().map(|(offset, _)| offset));
            } else {
                self.indices.truncate(cur);
                self.indices.extend(buf[idx..].char_indices().map(|(offset, _)| idx + offset));
            }
        }
    }

    fn make(&mut self) {
        if self.current != 0 {
            let idx = self.history.len() - self.current;
            let line = &self.history[idx];
            self.line.cursor = line.cursor;
            self.line.buf.clear();
            self.line.buf.push_str(&line.buf);
            self.current = 0;
            self.history.clear();
        }
    }

    pub fn push(&mut self, c: char) {
        self.make();

        let cur = self.line.cursor.head;
        let idx = self.index(cur);
        self.line.buf.insert(idx, c);
        self.update(cur, idx, || c.is_ascii());
        self.line.cursor.head += 1;
    }

    #[allow(dead_code)]
    pub fn replace(&mut self, s: char) {
        self.make();

        let cur = self.line.cursor.head;
        let idx = self.index(cur);
        let next_len = self.line
            .buf[idx..]
            .chars()
            .next()
            .map(char::len_utf8)
            .unwrap_or_default();
        let mut sbuf = [0; 4];
        let sbuf = s.encode_utf8(&mut sbuf);
        self.line.buf.replace_range(idx..(idx + next_len), sbuf);

        if next_len != sbuf.len() {
            self.update(cur, idx, || s.is_ascii());
        }
    }

    /// The cursor then covers the new text.
    pub fn replace_in_selection(&mut self, s: &str, clipboard: Option<&mut String>) {
        self.make();

        let range = self.text_range();

        let bytes_range = self.index(range.from())..self.index(range.to());
        let line = &mut self.line;

        if let Some(buf) = clipboard {
            buf.clear();
            buf.push_str(&line.buf[bytes_range.clone()]);
        }
        
        line.buf.replace_range(bytes_range.clone(), s);

        self.update(range.from(), bytes_range.start, || s.is_ascii());
        self.line.cursor = Range::new(range.from(), range.from() + s.chars().count());
    }

    pub fn push_str(&mut self, s: &str) {
        self.make();

        let cur = self.line.cursor.head;
        self.insert_str_at(cur, s);
    }

    pub fn push_str_at(&mut self, at: usize, s: &str) {
        self.make();

        self.insert_str_at(at, s);
    }

    fn insert_str_at(&mut self, at: usize, s: &str) {
        let idx = self.index(at);
        self.line.buf.insert_str(idx, s);
        self.update(at, idx, || s.is_ascii());
        self.line.cursor = Range::point(at + s.chars().count());
    }

    pub fn backspace(&mut self) {
        self.make();

        let cur = self.line.cursor.head;
        if cur != 0 {
            let idx = self.index(cur);
            if let Some(prev_char) = self.as_str()[..idx].chars().last() {
                let idx = idx - prev_char.len_utf8();
                self.line.buf.remove(idx);
                self.line.cursor.head -= 1;
                self.update(cur - 1, idx, || true);
            }
        }
    }

    pub fn delete(&mut self) {
        self.make();

        let cur = self.line.cursor.head;
        let idx = self.index(cur);
        if self.bytes_len() > idx {
            self.line.buf.remove(idx);
            self.update(cur, idx, || true);
        }
    }

    #[allow(dead_code)]
    pub fn delete_to_end(&mut self, _cur: usize) {
        self.make();

        let cur = self.line.cursor.head;
        let idx = self.index(cur);
        if self.bytes_len() > idx {
            self.line.buf.truncate(idx);
            self.update(cur, idx, || true);
        }
    }

    pub fn move_head(&mut self) {
        self.line_mut().cursor.head = 0;
    }

    pub fn move_end(&mut self) {
        self.line_mut().cursor.head = self.char_len();
    }

    pub fn move_left(&mut self) {
        let cur = &mut self.line_mut().cursor.head;
        *cur = cur.saturating_sub(1);
    }

    pub fn move_right(&mut self) {
        let len = self.char_len();
        let cur = &mut self.line_mut().cursor.head;
        *cur = cmp::min(*cur + 1, len);
    }

    pub fn move_left_word(&self) -> Range {
        let cur = self.line().cursor.head;
        let idx = self.index(cur);
        let buf = &self.as_str()[..idx];

        let iter = self.segmenter.segment_str(buf);
        let iter = MapWindows2::new(iter, |&[start, end]| start..end);

        if let Some(span) = iter.last() {
            let start = buf[..span.start].chars().count();
            let end = start + buf[span].chars().count();
            Range::new(start, end)
        } else {
            Range::point(cur)
        }
    }

    pub fn move_right_word(&self) -> Range {
        let cur = self.line().cursor.head;
        let idx = self.index(cur);
        let buf = &self.as_str()[idx..];

        let iter = self.segmenter.segment_str(buf);
        let mut iter = MapWindows2::new(iter, |&[start, end]| start..end);

        if let Some(span) = iter.next() {
            let start = cur + buf[..span.start].chars().count();
            let end = start + buf[span].chars().count();
            Range::new(start, end)
        } else {
            Range::point(cur)
        }
    }

    pub fn clear(&mut self) {
        self.history.clear();
        self.current = 0;
        self.indices.clear();
        self.line_mut().cursor = Range::point(0);
        self.line_mut().buf.clear();
    }

    pub fn up(&mut self) {
        self.current = std::cmp::min(self.current + 1, self.history.len());
        let is_ascii = self.as_str().is_ascii();
        self.update(0, 0, || is_ascii);
    }

    pub fn down(&mut self) {
        self.current = self.current.saturating_sub(1);
        let is_ascii = self.as_str().is_ascii();
        self.update(0, 0, || is_ascii);
    }

    pub fn set_history(&mut self, commands: Vec<&str>) {
        let iter = commands.into_iter()
            .rev()
            .map(|s| {
                let len = s.chars().count();
                Line { buf: s.into(), cursor: Range::point(len) }
            })
            .take(MAX_HISTORY);
        self.history.extend(iter);
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

impl Suggestion {
    pub fn has_suggest(&self) -> bool {
        !matches!(self.kind, SuggestionKind::Nothing)
    }
    
    pub fn as_str<'a>(&'a self, line: &'a EditableLine) -> &'a str {
        match self.kind {
            SuggestionKind::Nothing | SuggestionKind::Requested(_) => "",
            SuggestionKind::Suffix => &self.buf,
            SuggestionKind::History(idx) => {
                let history = &line.history[idx];
                history.buf.strip_prefix(&line.line.buf).unwrap_or_default()
            }
        }
    }

    pub fn clear(&mut self) {
        self.buf.clear();
        self.kind = SuggestionKind::Nothing;
    }

    pub fn requested(&self) -> Option<ipc::RequestId> {
        matches2!(self.kind, SuggestionKind::Requested)
    }

    pub fn set_requested(&mut self, request_id: ipc::RequestId) {
        self.kind = SuggestionKind::Requested(request_id);
    }
    
    pub fn set_suffix(&mut self, value: &str) {
        self.buf.clear();
        self.buf.push_str(value);
        self.kind = SuggestionKind::Suffix;
    }

    pub fn set_history(&mut self, line: &EditableLine) {
        if let Some((idx, _)) = line.history.iter()
            .map(|line| line.buf.as_str())
            .enumerate()
            .rev()
            .find(|(_, s)| s.starts_with(&line.line.buf))
        {
            self.kind = SuggestionKind::History(idx);
        }
    }

    pub fn apply(&mut self, line: &mut EditableLine) {
        match self.kind {
            SuggestionKind::Nothing | SuggestionKind::Requested(_) => (),
            SuggestionKind::Suffix => {
                line.push_str(&self.buf);
                line.collapse();
            },
            SuggestionKind::History(idx) => {
                line.line.buf.clear();
                line.line.buf.push_str(&line.history[idx].buf);
                let is_ascii = line.line.buf.is_ascii();
                line.update(0, 0, || is_ascii);
                line.move_end();
                line.collapse();
            }
        }

        self.clear();
    }
}

#[test]
fn test_buffer() {
    let mut buf = EditableLine::default();
    buf.push('a');
    buf.push('b');
    buf.push('c');
    assert_eq!(buf.as_str(), "abc");
    assert_eq!(buf.cursor().head, 3);

    buf.backspace();
    assert_eq!(buf.as_str(), "ab");
    assert_eq!(buf.cursor().head, 2);

    buf.delete();
    assert_eq!(buf.as_str(), "ab");
    assert_eq!(buf.cursor().head, 2);

    buf.move_left();
    buf.move_left();
    buf.delete();
    assert_eq!(buf.as_str(), "b");
    assert_eq!(buf.cursor().head, 0);

    buf.backspace();
    assert_eq!(buf.as_str(), "b");
    assert_eq!(buf.cursor().head, 0);

    buf.move_right();
    buf.backspace();
    assert_eq!(buf.as_str(), "");
    assert_eq!(buf.cursor().head, 0);

    buf.push('中');
    buf.push('文');
    buf.move_left();
    buf.move_left();
    buf.push('a');
    buf.push('a');
    buf.push('a');
    buf.move_right();
    buf.move_right();

    let (x, y) = buf.split(buf.cursor().head);
    assert_eq!(x, "aaa中文");
    assert_eq!(y, "");

    buf.clear();
    buf.push('中');
    buf.push('文');
    buf.move_left();

    let (x, y) = buf.split(buf.cursor().head);
    assert_eq!(x, "中");
    assert_eq!(y, "文");

    buf.push_str("hello world");
    buf.move_head();
    buf.collapse();

    let range = buf.move_right_word();
    *buf.cursor_mut() = range;
    assert_eq!(buf.selected(), "中");
    let range = buf.move_right_word();
    *buf.cursor_mut() = range;
    assert_eq!(buf.selected(), "hello");
    let range = buf.move_right_word();
    *buf.cursor_mut() = range;
    assert_eq!(buf.selected(), " ");
    let range = buf.move_right_word();
    *buf.cursor_mut() = range;
    assert_eq!(buf.selected(), "world");
    let range = buf.move_right_word();
    *buf.cursor_mut() = range;
    assert_eq!(buf.selected(), "文");
}
