use std::{ fs, io, fmt };
use std::marker::PhantomData;
use std::ffi::{ OsStr, OsString };
use serde::de::{ Deserialize, Deserializer, Visitor, MapAccess };


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
pub fn append_max() -> usize {
    // atomic append max value
    //
    // https://www.notthewizard.com/2014/06/17/are-files-appends-really-atomic/
    // https://stackoverflow.com/questions/1154446/is-file-append-atomic-in-unix
    // https://serverfault.com/questions/599486/what-is-the-size-of-an-atomic-write-to-disk-on-my-system
    // https://stackoverflow.com/questions/3032482/is-appending-to-a-file-atomic-with-windows-ntfs

    #[cfg(target_os = "linux")]
    fn append_max_limit() -> usize {
        0x7ffff000
    }

    #[cfg(all(unix, not(target_os = "linux")))]
    fn append_max_limit() -> usize {
        const DEFAULT_MAX_LIMIT: usize = 256;

        thread_local!{
            static MAX: usize = unsafe {
                match libc::sysconf(libc::_SC_SSIZE_MAX) {
                    -1 => DEFAULT_MAX_LIMIT,
                    n => n
                }
            };
        }

        MAX.with(|&n| n)
    }

    #[cfg(windows)]
    fn append_max_limit() -> usize {
        1024
    }

    append_max_limit()
}

#[inline]
pub fn arg_max() -> usize {
    const DEFAULT_MAX_LIMIT: usize = 16 * 1024;
    const DEFAULT_MIN_LIMIT: usize = 4 * 1024;

    #[cfg(unix)]
    fn arg_max_limit() -> usize {
        thread_local!{
            static MAX: usize = unsafe {
                match libc::sysconf(libc::_SC_ARG_MAX) {
                    -1 => DEFAULT_MAX_LIMIT,
                    n => std::cmp::max(n as usize, DEFAULT_MIN_LIMIT)
                }
            };
        }

        MAX.with(|&n| n)
    }

    #[cfg(windows)]
    fn arg_max_limit() -> usize {
        DEFAULT_MAX_LIMIT
    }

    arg_max_limit()
}

#[derive(Debug, Default)]
pub struct VecMap<K, V>(pub Vec<(K, V)>);

impl<'de, K, V> Deserialize<'de> for VecMap<K, V>
where
    K: Deserialize<'de>,
    V: Deserialize<'de>,
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct VecMapVisitor<K, V> {
            marker: PhantomData<fn() -> VecMap<K, V>>
        }

        impl<K, V> VecMapVisitor<K, V> {
            fn new() -> Self {
                VecMapVisitor {
                    marker: PhantomData
                }
            }
        }

        impl<'de, K, V> Visitor<'de> for VecMapVisitor<K, V>
        where
            K: Deserialize<'de>,
            V: Deserialize<'de>,
        {
            type Value = VecMap<K, V>;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("a vec map")
            }

            fn visit_map<M>(self, mut access: M) -> Result<Self::Value, M::Error>
            where
                M: MapAccess<'de>,
            {
                let mut map = Vec::with_capacity(access.size_hint().unwrap_or(0));

                while let Some((key, value)) = access.next_entry()? {
                    map.push((key, value));
                }

                Ok(VecMap(map))
            }
        }

        deserializer.deserialize_map(VecMapVisitor::new())
    }
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
