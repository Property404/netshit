use nix::{
    pty::{OpenptyResult, openpty},
    sys::termios::{self, BaudRate, InputFlags, LocalFlags, SetArg},
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
}

impl VirtSerBuilder {
    /// Create a new VirtSer builder
    #[must_use]
    pub const fn new() -> Self {
        Self {
            baud_rate: BaudRate::B115200,
            echo: false,
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

    /// Build a new [VirtSer]
    pub fn build(self) -> Result<VirtSer> {
        let OpenptyResult { master, slave } = openpty(None, None)?;

        set_nonblocking(master.as_raw_fd())?;

        let master_file = unsafe { File::from_raw_fd(master.into_raw_fd()) };
        let slave_path = ttyname(&slave)?;
        let slave_file = unsafe { File::from_raw_fd(slave.into_raw_fd()) };

        set_flags(&slave_file)?;
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

fn set_flags(file: &File) -> Result {
    let mut termios = termios::tcgetattr(file)?;

    // Canonical mode is not suited to our purpose
    // (line by line)
    termios.local_flags.remove(LocalFlags::ICANON);

    // Don't convert carriage to newline
    termios.input_flags.remove(InputFlags::ICRNL);

    termios::tcsetattr(file, SetArg::TCSANOW, &termios)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blocking_write() {
        let mut ser = VirtSerBuilder::new().build().unwrap();
        let mut slave = File::open(ser.path()).unwrap();

        let tvs = ["Hello, world!", "Howdy\ndo!\n", "Wow\rza!\n"];

        for tv in tvs {
            // Write to master
            ser.write_all(tv.as_bytes()).unwrap();

            // Read from slave
            let mut buf = vec![0; tv.len()];
            slave.read_exact(&mut buf).unwrap();

            println!("Testing: {tv}");
            assert_eq!(buf, tv.as_bytes());
        }
    }
}
