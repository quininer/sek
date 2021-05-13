use std::fs;
use std::collections::VecDeque;


const MAX_HISTORY_CAP: usize = 128;

#[derive(Default)]
pub struct History {
    queue: VecDeque<String>,
    cur: usize,
}

impl History {
    pub fn push(&mut self, cmd: &str) {
        if self.queue.back().map(|cmd| &**cmd) != Some(cmd) {
            let buf = if self.queue.len() >= MAX_HISTORY_CAP {
                self.queue.pop_front()
            } else {
                None
            };
            let cmd = if let Some(mut buf) = buf {
                buf.clear();
                buf.push_str(cmd);
                buf
            } else {
                cmd.into()
            };
            self.queue.push_back(cmd);
        }

        self.cur = 0;
    }

    pub fn up(&mut self) {
        self.cur = std::cmp::min(self.cur + 1, self.queue.len());
    }

    pub fn down(&mut self) {
        self.cur = self.cur.saturating_sub(1);
    }

    pub fn reset(&mut self) {
        self.cur = 0;
    }

    pub fn get(&self) -> Option<&str> {
        self.queue.get(self.queue.len() - self.cur)
            .map(|line| &**line)
    }
}

#[test]
fn test_history() {
    let mut history = History::default();

    history.push("ls src");
    assert_eq!(history.get(), None);

    history.up();
    assert_eq!(history.cur, 1);
    assert_eq!(history.get(), Some("ls src"));

    history.up();
    assert_eq!(history.cur, 1);
    assert_eq!(history.get(), Some("ls src"));

    history.down();
    assert_eq!(history.cur, 0);
    assert_eq!(history.get(), None);

    history.up();
    assert_eq!(history.cur, 1);
    assert_eq!(history.get(), Some("ls src"));

    history.push("echo hello world");
    assert_eq!(history.cur, 0);
    assert_eq!(history.get(), None);

    history.up();
    assert_eq!(history.cur, 1);
    assert_eq!(history.get(), Some("echo hello world"));

    history.up();
    assert_eq!(history.cur, 2);
    assert_eq!(history.get(), Some("ls src"));

    for _ in 0..256 {
        history.push("echo hello world");
    }

    assert_eq!(history.cur, 0);
    assert_eq!(history.get(), None);
    assert_eq!(history.queue.len(), 2);

    for i in 0..256 {
        history.push(format!("echo {}",i ).as_str());
    }

    assert_eq!(history.cur, 0);
    assert_eq!(history.get(), None);
    assert_eq!(history.queue.len(), MAX_HISTORY_CAP);
}
