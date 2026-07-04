pub mod arena;
pub mod path;
pub mod stdout;

use serde::Deserialize;
use std::borrow::Cow;
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::{fmt, io};

macro_rules! matches2 {
    ( $expr:expr, $item:path ) => {
        match $expr {
            $item(val) => Some(val),
            _ => None,
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

#[derive(Deserialize)]
pub struct CowStr<'s>(
    #[serde(borrow)]
    #[serde(deserialize_with = "deserialize_cow")]
    pub Cow<'s, str>,
);

impl AsRef<str> for CowStr<'_> {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

fn deserialize_cow<'de, D>(deserializer: D) -> Result<Cow<'de, str>, D::Error>
where
    D: serde::de::Deserializer<'de>,
{
    use serde::de;
    use std::fmt;

    struct Visitor;

    impl<'de> de::Visitor<'de> for Visitor {
        type Value = Cow<'de, str>;

        fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            formatter.write_str("a string")
        }

        fn visit_borrowed_str<E: de::Error>(self, val: &'de str) -> Result<Self::Value, E> {
            Ok(Cow::Borrowed(val))
        }

        fn visit_str<E: de::Error>(self, val: &str) -> Result<Self::Value, E> {
            self.visit_string(val.into())
        }

        fn visit_string<E: de::Error>(self, val: String) -> Result<Self::Value, E> {
            Ok(Cow::Owned(val))
        }
    }

    deserializer.deserialize_str(Visitor)
}

#[derive(Clone, Copy)]
pub struct FmtDebug<T>(pub T);

impl<T: std::fmt::Debug> fmt::Display for FmtDebug<T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        std::fmt::Debug::fmt(&self.0, f)
    }
}

pub struct MapWindows2<I: Iterator, F> {
    iter: I,
    f: F,
    buffer: Option<[I::Item; 2]>,
}

impl<I, F, U> MapWindows2<I, F>
where
    I: Iterator,
    F: FnMut(&[I::Item; 2]) -> U,
{
    pub fn new(mut iter: I, f: F) -> Self {
        let buffer = iter.next().zip(iter.next()).map(|(x, y)| [x, y]);
        MapWindows2 { iter, f, buffer }
    }
}

impl<I, F, U> Iterator for MapWindows2<I, F>
where
    I: Iterator,
    F: FnMut(&[I::Item; 2]) -> U,
{
    type Item = U;

    fn next(&mut self) -> Option<Self::Item> {
        let items = self.buffer.as_mut()?;
        let result = (self.f)(items);

        if let Some(next) = self.iter.next() {
            items[0] = next;
            items.swap(0, 1);
        } else {
            self.buffer = None;
        }

        Some(result)
    }
}

pub enum Either<L, R> {
    Left(L),
    Right(R),
}

pin_project_lite::pin_project! {
    pub struct Select<L, R> {
        #[pin]
        left: L,
        #[pin]
        right: R,
    }
}

impl<L, R> Select<L, R> {
    pub fn new(left: L, right: R) -> Self {
        Select { left, right }
    }
}

impl<L: Future, R: Future> Future for Select<L, R> {
    type Output = Either<L::Output, R::Output>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.project();

        match this.left.poll(cx) {
            Poll::Ready(result) => Poll::Ready(Either::Left(result)),
            Poll::Pending => this.right.poll(cx).map(Either::Right),
        }
    }
}

pub fn is_contains(haystack: &[u8], needle: &str, case_sensitive: bool) -> bool {
    use bstr::ByteSlice;

    if needle.is_empty() {
        false
    } else if case_sensitive {
        haystack.find(needle.as_bytes()).is_some()
    } else {
        let first = needle.as_bytes()[0];
        let lower = (first as char).to_ascii_lowercase() as u8;
        let upper = (first as char).to_ascii_uppercase() as u8;

        for pos in memchr::memchr2_iter(lower, upper, haystack) {
            let end = pos + needle.len();

            if let Some(substr) = haystack.get(pos..end)
                && substr.eq_ignore_ascii_case(needle.as_bytes())
            {
                return true;
            }
        }

        false
    }
}

#[cfg(unix)]
pub fn setup_signal_handler() -> io::Result<()> {
    let mut result = 0;

    unsafe {
        let mut act: libc::sigaction = std::mem::zeroed();
        act.sa_flags = 0;
        libc::sigemptyset(&mut act.sa_mask);

        // ignore
        act.sa_sigaction = libc::SIG_IGN;

        let nullptr = std::ptr::null_mut();
        result |= libc::sigaction(libc::SIGTSTP, &act, nullptr);
        result |= libc::sigaction(libc::SIGTTOU, &act, nullptr);
        result |= libc::sigaction(libc::SIGINT, &act, nullptr);
    }

    if result == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

#[cfg(unix)]
pub fn reset_signal_ignore() {
    unsafe {
        let mut act: libc::sigaction = std::mem::zeroed();
        act.sa_flags = 0;
        libc::sigemptyset(&mut act.sa_mask);

        // ignore
        act.sa_sigaction = libc::SIG_DFL;

        let nullptr = std::ptr::null_mut();
        libc::sigaction(libc::SIGTSTP, &act, nullptr);
        libc::sigaction(libc::SIGTTOU, &act, nullptr);
        libc::sigaction(libc::SIGINT, &act, nullptr);
    }
}
