use nix::{
    pty::{OpenptyResult, openpty},
    sys::termios::{self, BaudRate, LocalFlags, SetArg, cfmakeraw},
    unistd::ttyname,
};
use std::{
    fs::File,
    io::{Read, Write},
    os::fd::{AsRawFd, FromRawFd, IntoRawFd, RawFd},
    path::{Path, PathBuf},
    time::Duration,
};
mod error;
pub use error::{Error, Result};

#[derive(Copy, Clone, Debug)]
pub struct VirtSerBuilder {
    baud_rate: BaudRate,
    echo: bool,
    raw: bool,
}

impl VirtSerBuilder {
    /// Create a new VirtSer builder
    #[must_use]
    pub const fn new() -> Self {
        Self {
            baud_rate: BaudRate::B115200,
            echo: false,
            raw: true,
        }
    }

    /// Set the baud rate
    #[must_use]
    pub const fn set_baud_rate(mut self, baud_rate: BaudRate) -> Self {
        self.baud_rate = baud_rate;
        self
    }

    /// If true, set echo on. If false, set echo off
    #[must_use]
    pub const fn set_echo(mut self, echo: bool) -> Self {
        self.echo = echo;
        self
    }

    /// If true, set raw mode on. If false, set raw mode off
    #[must_use]
    pub const fn set_raw(mut self, raw: bool) -> Self {
        self.raw = raw;
        self
    }

    /// Build a new [VirtSer]
    pub fn build(self) -> Result<VirtSer> {
        let OpenptyResult { master, slave } = openpty(None, None)?;

        set_nonblocking(master.as_raw_fd())?;

        let master_file = unsafe { File::from_raw_fd(master.into_raw_fd()) };
        let slave_path = ttyname(&slave)?;
        let slave_file = unsafe { File::from_raw_fd(slave.into_raw_fd()) };

        set_raw(&master_file, self.raw)?;
        set_raw(&slave_file, self.raw)?;
        set_echo(&slave_file, self.echo)?;
        set_baud_rate(&slave_file, self.baud_rate)?;

        Ok(VirtSer {
            master_file,
            _slave_file: slave_file,
            slave_path,
        })
    }
}

impl Default for VirtSerBuilder {
    fn default() -> Self {
        VirtSerBuilder::new()
    }
}

/// Virtual serial device accessible via a PTS
#[derive(Debug)]
pub struct VirtSer {
    master_file: File,
    _slave_file: File,
    slave_path: PathBuf,
}

impl VirtSer {
    /// Get PTS path
    pub fn path(&self) -> &Path {
        &self.slave_path
    }
}

impl Read for VirtSer {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        loop {
            match self.master_file.read(buf) {
                Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                    continue;
                }
                other => {
                    return other;
                }
            }
        }
    }
}

impl Write for VirtSer {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        loop {
            match self.master_file.write(buf) {
                Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(Duration::from_millis(100));
                    continue;
                }
                other => {
                    return other;
                }
            }
        }
    }
    fn flush(&mut self) -> std::io::Result<()> {
        self.master_file.flush()
    }
}

// Credit: Pavel Kuzmin (license: MIT)
// https://github.com/s00d/virtualport/blob/ad3809c28ad942d8036e01f5669e5214d698c178/src/pty.rs
fn set_nonblocking(fd: RawFd) -> Result {
    use nix::fcntl::{F_GETFL, F_SETFL, OFlag, fcntl};
    let flags = fcntl(fd, F_GETFL)?;
    let new_flags = OFlag::from_bits_truncate(flags) | OFlag::O_NONBLOCK;
    fcntl(fd, F_SETFL(new_flags))?;
    Ok(())
}

fn set_baud_rate(file: &File, baud: BaudRate) -> Result {
    let mut termio = termios::tcgetattr(file)?;
    termios::cfsetispeed(&mut termio, baud)?;
    termios::cfsetospeed(&mut termio, baud)?;
    termios::tcsetattr(file, SetArg::TCSANOW, &termio)?;
    Ok(())
}

fn set_echo(file: &File, echo: bool) -> Result {
    let mut termios = termios::tcgetattr(file)?;
    if !echo {
        termios.local_flags.remove(LocalFlags::ECHO);
    }
    termios::tcsetattr(file, SetArg::TCSANOW, &termios)?;
    Ok(())
}

fn set_raw(file: &File, raw: bool) -> Result {
    let mut termios = termios::tcgetattr(file)?;

    if raw {
        cfmakeraw(&mut termios);
    }

    termios::tcsetattr(file, SetArg::TCSANOW, &termios)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const TVS: &[&str] = &[
        "Hello, world!",
        "Howdy\ndo!\n",
        "Wow\rza!\n",
        "😀\r\r\x01\x02\x03\x04DAGÁN -",
    ];

    // Write to master, read from slave
    #[test]
    fn blocking_write() {
        let mut ser = VirtSerBuilder::new().build().unwrap();

        for tv in TVS {
            // Write to master
            ser.write_all(tv.as_bytes()).unwrap();

            // Read from slave
            let mut buf = vec![0; tv.len()];
            ser._slave_file.read_exact(&mut buf).unwrap();

            println!("Testing: {tv}");
            assert_eq!(tv.as_bytes(), buf);
        }
    }

    // Write to slave, read from master
    #[test]
    fn blocking_read() {
        let mut ser = VirtSerBuilder::new().build().unwrap();

        for tv in TVS {
            // Write to slave
            ser._slave_file.write_all(tv.as_bytes()).unwrap();

            // Read from master
            let mut buf = vec![0; tv.len()];
            ser.read_exact(&mut buf).unwrap();

            println!("Testing: {tv}");
            assert_eq!(tv.as_bytes(), buf);
        }
    }
}
