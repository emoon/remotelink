use crate::options::*;
use anyhow::Result;
use std::io::{BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::thread;
use crate::messages::*;

struct Contex {
    stream: TcpStream,
}

fn handle_client(stream: &mut TcpStream) -> Result<()> {
    println!("Incoming connection from: {}", stream.peer_addr()?);

    //let mut filebuffer = Vec::new();

    loop {
        let header = get_header(stream)?;

        match header.msg_type {
            Messages::FistbumpRequest => {
                let msg: FistbumpRequest = get_message(stream, header)?;



            },
        }

        /*
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
        */
    }
}

pub fn target_loop(_opts: &Opt) {
    let listener = TcpListener::bind("0.0.0.0:8888").expect("Could not bind");
    println!("Wating incoming host");
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
