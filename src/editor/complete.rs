use std::ops::Range;
use unicode_width::UnicodeWidthStr;
use crate::util::is_contains;

#[derive(Debug, Default)]
pub struct CompleteSelector {
    space: (u16, u16),
    pub window: Range<usize>,
    pub cur: usize,
    pub column: usize,
    pub width: usize,
    pub list: Vec<String>,
    pub desc: Vec<String>,
    pub search: String,
}

const MAX_HEIGHT: usize = 12;

impl CompleteSelector {
    pub fn set_space(&mut self, space: (u16, u16)) {
        self.space = space;
    }

    pub fn search_up(&mut self) {
        if self.search.is_empty() {
            return
        }

        if let Some((cur, _)) = self.list.iter()
            .enumerate()
            .take(self.cur)
            .rev()
            .find(|(_, s)| is_contains(s.as_bytes(), &self.search, false))
        {
            self.cur = cur;
            self.update_window();
        }
    }

    pub fn search_down(&mut self) {
        if self.search.is_empty() {
            return
        }
        
        if let Some((cur, _)) = self.list.iter()
            .enumerate()
            .skip(self.cur + 1)
            .find(|(_, s)| is_contains(s.as_bytes(), &self.search, false))
        {
            self.cur = cur;
            self.update_window();
        }
    }

    pub fn update(&mut self) {
        if self.list.is_empty() {
            return;
        }
        
        let max_width = usize::from(self.space.0);
        self.width = self.list
            .iter()
            .enumerate()
            .map(|(idx, s)| {
                let comp = s.width();
                let desc = self.desc
                    .get(idx)
                    // ' (desc)'
                    .map(|desc| desc.width() + 3)
                    .unwrap_or_default();
                // 'comp (desc) '
                comp + desc + 1
            })
            .max()
            .unwrap_or_default()
            .min(max_width);
        self.column = (max_width / self.width).max(1);

        let row = div_roundup(self.list.len(), self.column);
        let hint = div_roundup(self.cur + 1, self.column).saturating_sub(1);
        
        let window_len = usize::from(self.space.1).min(MAX_HEIGHT);
        self.window = if hint + window_len > row {
            row.saturating_sub(window_len)..row
        } else {
            hint..(hint + window_len).min(row)
        };
    }

    pub fn update_window(&mut self) {
        let row = div_roundup(self.list.len(), self.column);
        let hint = div_roundup(self.cur + 1, self.column).saturating_sub(1);
        let window_len = usize::from(self.space.1).min(MAX_HEIGHT);

        if hint < self.window.start {
            self.window = hint..(hint + window_len).min(row);
        } else if hint >= self.window.end {
            self.window = if hint + window_len > row {
                row.saturating_sub(window_len)..row
            } else {
                (hint.saturating_sub(window_len) + 1)..(hint + 1)
            };
        }
    }

    pub fn clear(&mut self) {
        self.cur = 0;
        self.list.clear();
        self.desc.clear();
        self.search.clear();
    }
}

fn div_roundup(x: usize, y: usize) -> usize {
    if x == 0 || y == 0 {
        return 0
    }

    let rem = !x.is_multiple_of(y);
    (x / y) + rem as usize
}

#[test]
fn complete_selector_shows_more_items_after_resize() {
    let mut selector = CompleteSelector::default();
    selector.list = (0..12)
        .map(|idx| format!("item-{idx}"))
        .collect();
    selector.cur = 3;

    selector.set_space((10, 4));
    selector.update();
    let initial_column = selector.column;
    let initial_capacity = selector.window.len() * selector.column;

    selector.set_space((40, 4));
    selector.update();

    assert!(selector.column > initial_column);
    assert!(selector.window.contains(&0));
    assert!(selector.window.contains(&(selector.cur / selector.column)));
    assert!(selector.window.len() * selector.column > initial_capacity);
}
