use std::fs::File;
use std::mem::ManuallyDrop;
use std::io::{ self, BufWriter };
use std::cell::{ RefCell, RefMut };

pub struct Stdout {
    stdout: io::Stdout,
    writer: RefCell<BufWriter<StdoutRaw>>
}

pub struct StdoutLocked<'lock> {
    _lock: io::StdoutLock<'lock>,
    writer: RefMut<'lock, BufWriter<StdoutRaw>>
}

struct StdoutRaw(ManuallyDrop<File>);

impl Stdout {
    pub fn lock(&self) -> StdoutLocked<'_> {
        StdoutLocked {
            _lock: self.stdout.lock(),
            writer: self.writer.borrow_mut()
        }
    }
}

impl io::Write for StdoutRaw {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0.write(buf)
    }

    fn write_vectored(&mut self, bufs: &[io::IoSlice<'_>]) -> io::Result<usize> {
        self.0.write_vectored(bufs)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.0.flush()
    }
}

impl io::Write for StdoutLocked<'_> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.writer.write(buf)
    }

    fn write_vectored(&mut self, bufs: &[io::IoSlice<'_>]) -> io::Result<usize> {
        self.writer.write_vectored(bufs)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.writer.flush()
    }
}

impl From<io::Stdout> for Stdout {
    fn from(value: io::Stdout) -> Self {
        let writer = StdoutRaw::from(&value);
        Stdout {
            stdout: value,
            writer: RefCell::new(BufWriter::new(writer))
        }
    }
}

impl From<&'_ io::Stdout> for StdoutRaw {
    #[cfg(unix)]
    fn from(value: &'_ io::Stdout) -> Self {
        use std::os::fd::{ FromRawFd, AsRawFd };
        
        StdoutRaw(ManuallyDrop::new(unsafe {
            File::from_raw_fd(value.as_raw_fd())
        }))
    }

    #[cfg(windows)]
    fn from(value: &'_ io::Stdout) -> Self {
        use std::os::windows::io::{ FromRawHandle, AsRawHandle };
        
        StdoutRaw(ManuallyDrop::new(unsafe {
            File::from_raw_handle(value.as_raw_handle())
        }))
    }    
}
