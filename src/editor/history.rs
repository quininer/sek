use std::fs;
use std::collections::VecDeque;


const MAX_HISTORY_CAP: usize = 128;

#[derive(Default)]
pub struct History {
    queue: VecDeque<String>,
    cursor: usize,
}

#[derive(Default)]
pub struct BufPool {
    queue: Vec<String>
}

impl History {
    pub fn push(&mut self, pool: &mut BufPool, cmd: &str) {
        if self.queue.back().map(|cmd| &**cmd) != Some(cmd) {
            let cmd = if let Some(mut cmdbuf) = pool.queue.pop() {
                cmdbuf.clear();
                cmdbuf.push_str(cmd);
                cmdbuf
            } else {
                cmd.into()
            };
            self.queue.push_back(cmd);
        }

        if self.queue.len() > MAX_HISTORY_CAP {
            if let Some(cmdbuf) = self.queue.pop_front() {
                pool.queue.push(cmdbuf);
            }
        }

        self.cursor = 0;
    }

    pub fn up(&mut self) {
        self.cursor = std::cmp::min(self.cursor + 1, self.queue.len());
    }

    pub fn down(&mut self) {
        self.cursor = self.cursor.saturating_sub(1);
    }

    pub fn current(&self) -> &str {
        let cur = self.queue.len() - self.cursor;
        &self.queue[cur]
    }
}
