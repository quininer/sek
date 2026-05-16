use std::ops::Range;
use unicode_width::UnicodeWidthStr;

#[derive(Debug, Default)]
pub struct CompleteSelector {
    space: (u16, u16),
    pub window: Range<usize>,
    pub cur: usize,
    pub column: usize,
    pub width: usize,
    pub list: Vec<String>,
    pub desc: Vec<String>,
}

impl CompleteSelector {
    pub fn set_space(&mut self, space: (u16, u16)) {
        self.space = space;
    }

    pub fn update(&mut self) {
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
            .unwrap_or_default();
        self.column = (usize::from(self.space.0) / self.width).max(1);

        let row = div_roundup(self.list.len(), self.column);
        let hint = div_roundup(self.cur + 1, self.column).saturating_sub(1);
        
        let window_len = usize::from(self.space.1).min(6);
        self.window = if hint + window_len > row {
            row.saturating_sub(window_len)..row
        } else {
            hint..(hint + window_len).min(row)
        };
    }

    pub fn update_window(&mut self) {
        let row = div_roundup(self.list.len(), self.column);
        let hint = div_roundup(self.cur + 1, self.column).saturating_sub(1);
        let window_len = usize::from(self.space.1).min(6);

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
}

fn div_roundup(x: usize, y: usize) -> usize {
    if x == 0 || y == 0 {
        return 0
    }

    let rem = !x.is_multiple_of(y);
    (x / y) + rem as usize
}
