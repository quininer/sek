use std::{ io, fmt };
use std::marker::Unpin;
use bumpalo::collections::Vec;
use tokio::io::AsyncRead;


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
    const DEFAULT_MAX_LIMIT: usize = 16 * 1024;
    const DEFAULT_MIN_LIMIT: usize = 4 * 1024;

    #[cfg(unix)]
    fn arg_max_limit() -> usize {
        match unsafe { libc::sysconf(libc::_SC_ARG_MAX) } {
            -1 => DEFAULT_MAX_LIMIT,
            n => std::cmp::max(n as usize, DEFAULT_MIN_LIMIT)
        }
    }

    #[cfg(windows)]
    fn arg_max_limit() -> usize {
        DEFAULT_MAX_LIMIT
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

pub async fn read_to_end<R: AsyncRead + Unpin>(
    mut reader: R,
    tmpbuf: &mut [u8],
    outbuf: &mut Vec<'_, u8>
) -> io::Result<()> {
    use tokio::io::AsyncReadExt;

    loop {
        let n = reader.read(tmpbuf).await?;
        if n == 0 {
            break
        }

        outbuf.extend_from_slice(&tmpbuf[..n]);
    }

    Ok(())
}
