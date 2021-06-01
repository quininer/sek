use std::{ io, fmt };
use std::ffi::OsStr;
use std::cmp::Ordering;


#[derive(Clone, Copy)]
pub struct Fill {
    star: char,
    len: u16
}

impl Fill {
    pub fn empty(len: u16) -> Fill {
        Fill { star: ' ', len }
    }

    pub fn flag(len: u16) -> Fill {
        Fill { star: '^', len }
    }
}

impl fmt::Display for Fill {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        for _ in 0..self.len {
            write!(f, "{}", self.star)?;
        }

        Ok(())
    }
}

#[derive(Clone, Copy)]
pub struct FmtDebug<T>(pub T);

impl<T: std::fmt::Debug> fmt::Display for FmtDebug<T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        std::fmt::Debug::fmt(&self.0, f)
    }
}

#[inline]
pub fn arg_max() -> usize {
    #[cfg(unix)]
    fn arg_max_limit() -> usize {
        match unsafe { libc::sysconf(libc::_SC_ARG_MAX) } {
            -1 => 1024 * 1024,
            n => std::cmp::max(n as usize, 4 * 1024)
        }
    }

    #[cfg(windows)]
    fn arg_max_limit() -> usize {
        // https://docs.microsoft.com/zh-cn/windows/win32/api/processthreadsapi/nf-processthreadsapi-createprocessa

        32767
    }

    arg_max_limit()
}

pub struct DynWriter<'a>(pub &'a mut dyn io::Write);

impl io::Write for DynWriter<'_> {
    #[inline]
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0.write(buf)
    }

    #[inline]
    fn flush(&mut self) -> io::Result<()> {
        self.0.flush()
    }

    #[inline]
    fn write_vectored(&mut self, bufs: &[io::IoSlice<'_>]) -> io::Result<usize> {
        self.0.write_vectored(bufs)
    }
}

// TODO use human sort
pub fn file_name_cmp(x: &OsStr, y: &OsStr) -> Ordering {
    Ord::cmp(x, y)
}
