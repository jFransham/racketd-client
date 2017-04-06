#[macro_use]
extern crate serde_derive;

extern crate tempdir;
extern crate serde;
extern crate serde_json;
extern crate clap;

use std::io::prelude::*;
use std::io::{
    stdin,
    stdout,
    stderr,
    BufReader,
};
use std::process::Command;
use std::sync::Arc;
use std::sync::atomic::{
    AtomicBool,
    Ordering,
};
use std::fs::{
    File,
    OpenOptions,
};
use std::net::{
    Shutdown,
    TcpStream,
};
use std::time::Duration;
use std::path::{
    Path,
    PathBuf,
};
use clap::{App, Arg};
use tempdir::TempDir;

#[derive(Serialize)]
struct ServerInput {
    file: String,
    stdin: String,
    args: Vec<String>,
}

fn spawn_server() {
    writeln!(stderr(), "Can't find racketd, spawning...").unwrap();
    Command::new("/bin/sh")
        .arg("-c")
        .arg("racketd & disown")
        .spawn()
        .unwrap();
}

fn connect() -> Result<TcpStream, std::io::Error> {
    TcpStream::connect("127.0.0.1:65511")    
}

fn connect_and_wait() -> Result<TcpStream, std::io::Error> {
    connect().or_else(|e| {
        std::thread::sleep(Duration::from_millis(500));
        Err(e)
    })
}

fn retry<R, E>(func: fn() -> Result<R, E>, n: usize) -> Result<R, E> {
    func().or_else(|e| if n > 0 { retry(func, n - 1) } else { Err(e) })
}

// TODO: Fifos hang on open, find out why this is so that using this in the
//       middle of a pipe workflow doesn't cause the whole stdin to be eagerly
//       consumed.
fn make_anonymous_fifo<P: AsRef<Path>>(dir: &TempDir, name: P) -> PathBuf {
    let newfile = dir.path().join(name);

    File::create(&newfile).unwrap();

    newfile
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
        .arg(
            Arg::with_name("ARGS")
                .help(
                    "The arguments for the script"
                )
                .multiple(true)
                .last(true)
        )
        .get_matches();

    let tempdir = TempDir::new("racketd-client").unwrap();

    let o_file: Option<String> = matches.value_of("FILE")
        .and_then(|filename|
            if filename == "-" {
                None
            } else {
                Some(
                    std::fs::canonicalize(
                        Path::new(filename)
                    ).unwrap().to_string_lossy().into()
                )
            }
        );

    let endflag = Arc::new(AtomicBool::new(false));

    let (stdin_file, thread): (String, _) =  {
        let file = make_anonymous_fifo(&tempdir, "input");
        let file_c = file.clone();
        let endflag_borrow = endflag.clone();

        let thread = std::thread::spawn(move || {
            let mut out = OpenOptions::new()
                .read(false)
                .append(true)
                .open(file_c)
                .unwrap();
            let mut s_in = stdin();
            let mut buf = [0; 1024];

            while let Ok(n) = s_in.read(&mut buf) {
                if n == 0 || endflag_borrow.load(Ordering::Relaxed) {
                    break;
                }

                out.write(&buf[..n]).unwrap();
            }
        });

        (file.to_string_lossy().into(), thread)
    };

    let server_stdin_file = if let Some(_) = o_file {
        stdin_file.clone()
    } else {
        "/dev/null".into()
    };

    let file = o_file.unwrap_or(stdin_file);

    let mut stream = retry(connect_and_wait, 3).or_else(|_| {
        spawn_server();
        retry(connect_and_wait, 3)
    }).unwrap();

    stream.write(
        serde_json::to_string(
            &ServerInput {
                file: file,
                stdin: server_stdin_file,
                args: matches.values_of("ARGS")
                    .map(|args| args.map(str::to_string).collect())
                    .unwrap_or(vec![]),
            }
        ).unwrap().as_bytes()
    ).unwrap();

    stream.flush().unwrap();
    stream.shutdown(Shutdown::Write).unwrap();

    let mut buf = [0; 256];
    let s_out = stdout();
    let mut out = s_out.lock();

    while let Ok(n) = stream.read(&mut buf) {
        if n == 0 { break; }

        out.write(&buf[..n]).unwrap();
    }
}
