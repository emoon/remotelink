use std::fs::File;
use std::io::{BufReader, Error, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::os::unix::fs::PermissionsExt;
use std::process::Command;
use std::thread;
use structopt::StructOpt;

#[derive(StructOpt, Debug)]
#[structopt(name = "r2link")]
struct Opt {
    #[structopt(short, long)]
    debug: bool,
    #[structopt(short, long)]
    server: bool,
    #[structopt(short, long)]
    target: Option<String>,
    #[structopt(short, long)]
    filename: Option<String>,
}

const START_FILE: u8 = 0;
const FILE_CHUNK: u8 = 1;
const END_FILE: u8 = 2;

// TODO: Optimize
fn copy_data(target: &mut Vec<u8>, src: &[u8]) {
    for t in src {
        target.push(*t);
    }
}

fn handle_client(stream: TcpStream) -> Result<(), Error> {
    println!("Incoming connection from: {}", stream.peer_addr()?);
    let mut data: [u8; 1024] = [0; 1024];
    let mut stream = BufReader::new(stream);
    let mut filebuffer = Vec::new();

    loop {
        let bytes_read = { stream.read(&mut data)? };
        if bytes_read == 0 {
            return Ok(());
        }

        let id = data[0];

        match id {
            START_FILE => {
                let filename = std::str::from_utf8(&data[1..bytes_read]).unwrap();
                println!("Client is about to send {} (len {})", filename, bytes_read);
            }

            FILE_CHUNK => {
                println!("Got file chunk size {}", bytes_read);
                copy_data(&mut filebuffer, &data[1..bytes_read]);
            }

            END_FILE => {
                println!("Got file end chunk size {}", bytes_read);
                copy_data(&mut filebuffer, &data[1..bytes_read]);

                {
                    let mut file = File::create("test")?;
                    file.write_all(&filebuffer)?;
                }

                // make exe executable
                std::fs::set_permissions("test", std::fs::Permissions::from_mode(0o700)).unwrap();

                let output = Command::new("./test")
                    .output()
                    .expect("failed to execute process");

                println!("status: {}", output.status);
                std::io::stdout().write_all(&output.stdout).unwrap();
            }

            _ => (),
        }
    }
}
fn server_loop(_opts: &Opt) {
    let listener = TcpListener::bind("0.0.0.0:8888").expect("Could not bind");
    for stream in listener.incoming() {
        match stream {
            Err(e) => eprintln!("failed: {}", e),
            Ok(stream) => {
                thread::spawn(move || {
                    handle_client(stream).unwrap_or_else(|error| eprintln!("{:?}", error));
                });
            }
        }
    }
}

fn init_packet(dest: &mut [u8], src: &[u8], command: u8) -> usize {
    dest[0] = command;

    for (place, b) in dest[1..].iter_mut().zip(src) {
        *place = *b;
    }

    src.len() + 1
}

fn client_loop(opts: &Opt, _ip_address: &str) {
    let mut stream = TcpStream::connect("127.0.0.1:8888").expect("Could not connect to server");

    if let Some(filename) = opts.filename.as_ref() {
        let mut data: [u8; 1024] = [0; 1024];
        let chunk_size = 1023;
        let mut file = File::open(&filename).unwrap();

        init_packet(&mut data, filename.as_bytes(), START_FILE);
        stream.write_all(&data).unwrap();

        loop {
            let size = file.read(&mut data[1..]).unwrap();
            data[0] = FILE_CHUNK;

            if size != chunk_size {
                data[0] = END_FILE;
            }

            stream.write_all(&data).unwrap();

            if size < chunk_size {
                break;
            }
        }

        thread::sleep(std::time::Duration::from_millis(500));
    }
}

fn main() {
    let opt = Opt::from_args();

    if opt.server {
        server_loop(&opt);
    } else if opt.target.is_some() {
        client_loop(&opt, opt.target.as_ref().unwrap());
    } else {
        println!("Must pass --server or --client");
    }
}
