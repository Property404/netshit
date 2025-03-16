#![allow(unused_mut, unused_imports, dead_code)]
use nix::{
    errno::Errno,
    pty::{OpenptyResult, openpty},
    sys::termios::{self, BaudRate, ControlFlags, LocalFlags, SetArg},
};
use std::{
    fs::File,
    io::{Read, Write},
    os::fd::{AsRawFd, FromRawFd, IntoRawFd, OwnedFd, RawFd},
    pin::{Pin, pin},
    time::Duration,
};
mod error;
pub use error::{Error, Result};

pub struct VirtSer {
    master_file: File,
    slave_file: File,
    slave_name: String,
}

impl VirtSer {
    pub fn new() -> Result<Self> {
        let OpenptyResult { master, slave } = openpty(None, None)?;
        set_nonblocking(master.as_raw_fd())?;

        let slave_name = get_file_name(slave.as_raw_fd());
        let slave_file = unsafe { File::from_raw_fd(slave.into_raw_fd()) };
        println!("{slave_name}: ");

        set_echo(&slave_file, false)?;
        set_baud_rate(&slave_file, BaudRate::B115200)?;

        let master_name = get_file_name(master.as_raw_fd());
        let mut master_file = unsafe { File::from_raw_fd(master.into_raw_fd()) };
        println!("{master_name}: ");

        master_file.write_all(b"Howdy y'all\n").unwrap();

        Ok(Self {
            master_file,
            slave_file,
            slave_name,
        })
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

fn get_file_name(fd: RawFd) -> String {
    use libc::ttyname;
    use std::ffi::CStr;
    let ret = unsafe { ttyname(fd) };
    if ret.is_null() {
        "unknown".to_string()
    } else {
        let path = unsafe { CStr::from_ptr(ret).to_string_lossy() };
        path.to_string()
    }
}

fn set_baud_rate(file: &File, baud: BaudRate) -> Result {
    let mut termio = termios::tcgetattr(file)?;
    termios::cfsetispeed(&mut termio, baud)?;
    termios::cfsetospeed(&mut termio, baud)?;
    termios::tcsetattr(file, SetArg::TCSANOW, &termio)?;
    Ok(())
}

fn set_echo(file: &File, echo: bool) -> Result {
    let mut termios = termios::tcgetattr(&file)?;
    if !echo {
        termios.local_flags.remove(LocalFlags::ECHO);
    }
    termios::tcsetattr(&file, SetArg::TCSANOW, &termios)?;
    Ok(())
}
