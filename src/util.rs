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

pub fn file_name_cmp(x: &OsStr, y: &OsStr) -> Ordering {
    if let (Some(x), Some(y)) = (x.to_str(), y.to_str()) {
        lexical_sort::natural_cmp(x, y)
    } else {
        Ord::cmp(x, y)
    }
}

pub struct EscapePath<'a>(pub &'a str);

impl fmt::Display for EscapePath<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use std::fmt::Write;
        use bstr::ByteSlice;

        let s = self.0.as_bytes();

        if s.find_byteset(r#"\$()'""#).is_none() {
            f.write_str(self.0)?;
        } else if s.find_byte(b'\'').is_none() {
            f.write_char('\'')?;
            f.write_str(self.0)?;
            f.write_char('\'')?;
        } else {
            f.write_char('"')?;
            for c in s.chars() {
                match c {
                    '"' => f.write_str(r#"\""#)?,
                    '$' => f.write_str(r#"\$"#)?,
                    '\\' => f.write_str(r#"\\"#)?,
                    c => f.write_char(c)?
                }
            }
            f.write_char('"')?;
        }

        Ok(())
    }
}
