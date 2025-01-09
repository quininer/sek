pub mod arena;
pub mod stdout;

use std::io;

macro_rules! matches2 {
    ( $expr:expr, $item:path ) => {
        match $expr {
            $item ( val ) => Some(val),
            _ => None
        }
    };
}

pub struct ScopeGuard<T, F: Fn(&mut T)>(pub T, pub F);

impl<T, F: Fn(&mut T)> AsRef<T> for ScopeGuard<T, F> {
    fn as_ref(&self) -> &T {
        &self.0
    }
}

impl<T, F: Fn(&mut T)> AsMut<T> for ScopeGuard<T, F> {
    fn as_mut(&mut self) -> &mut T {
        &mut self.0
    }
}

impl<T, F: Fn(&mut T)> Drop for ScopeGuard<T, F> {
    fn drop(&mut self) {
        (self.1)(&mut self.0);
    }
}

pub struct RefWriter<'a>(pub &'a mut dyn io::Write);

impl RefWriter<'_> {
    pub fn reborrow(&mut self) -> RefWriter<'_> {
        RefWriter(self.0)
    }
}

impl io::Write for RefWriter<'_> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.0.flush()
    }
}
