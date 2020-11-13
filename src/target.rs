use std::net::{TcpStream};
use std::io::{BufReader, Read, Write};
use std::io::{Result};

struct Contex {
    stream: TcpStream,
}

fn init_packet(dest: &mut [u8], src: &[u8], command: u8) -> usize {
    dest[0] = command;

    for (place, b) in dest[1..].iter_mut().zip(src) {
        *place = *b;
    }

    src.len() + 1
}

fn handle_client(stream: TcpStream) -> Result<()> {
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

fn target_loop(_opts: &Opt) {
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



