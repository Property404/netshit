use std::io::{Write, Read};

use virtser::VirtSer;
use anyhow::Result;
use utf8_parser::Utf8Parser;


fn main() -> Result<()> {

    let mut parser = Utf8Parser::new();
    println!("Opening serial device");
    let mut ser = VirtSer::new()?;
    println!("Looping");
    ser.write_all(b"Hello darling\n")?;
    ser.write_all(b"Hello lovelies\n")?;
    loop {
        let mut buf = [0;1];
        ser.read_exact(&mut buf)?;
        match parser.push(buf[0])? {
            None => {},
            Some(c) => {print!("{c}")}
        };
        ser.write_all(&buf)?; // echo
    }
}
