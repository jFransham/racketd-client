extern crate clap;

use std::io::prelude::*;
use std::io::{
    stdin,
    stderr,
    BufReader,
};
use std::process::Command;
use std::fs::File;
use std::net::{
    Shutdown,
    TcpStream,
};
use std::time::Duration;
use clap::{App, Arg};

fn spawn_server() {
    writeln!(stderr(), "Can't find racketd, spawning...").unwrap();
    Command::new("/bin/sh")
        .arg("-c")
        .arg("racketd & disown")
        .spawn()
        .unwrap();
    std::thread::sleep(Duration::from_secs(1));
}

fn connect() -> Result<TcpStream, std::io::Error> {
    TcpStream::connect("127.0.0.1:65511")    
}

fn main() {
    let matches = App::new("racketd-client")
        .version("0.1")
        .about(
            "Send racket file to racketd"
        )
        .author("One F Jef")
        .arg(
            Arg::with_name("FILE")
                .help(
                    "The file to read, if not supplied (or `-`) will read from \
                     stdin"
                )
                .index(1)
        )
        .get_matches();

    let file: Box<BufRead> = matches.value_of("FILE")
        .and_then(|filename|
            if filename == "-" {
                None
            } else {
                Some(
                    File::open(filename).map(|f|
                        Box::new(BufReader::new(f)) as _
                    )
                )
            }
        )
        .unwrap_or_else(
            || Ok(Box::new(BufReader::new(stdin())) as _)
        )
        .expect("Could not open file");

    let mut stream = connect().or_else(|_| {
        spawn_server();
        connect()
    }).unwrap();

    for line in file.lines() {
        let ln = line.unwrap();
        stream.write(ln.as_bytes()).unwrap();
        stream.write(b"\n").unwrap();
    }

    stream.flush().unwrap();
    stream.shutdown(Shutdown::Write).unwrap();

    let mut output = String::new();
    stream.read_to_string(&mut output).unwrap();

    print!("{}", output);
}
