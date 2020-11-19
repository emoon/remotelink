use crate::options::*;
use crate::messages;
use crate::messages::{Messages, FistbumpRequest, FistbumpReply };
use anyhow::*;
use std::net::{TcpListener, TcpStream};
use std::thread;

struct Contex {
    stream: TcpStream,
}

/*
 *
 *

        match stream.read(&mut buf[..]) {
            Ok(n) if n > 0 => {
                let msg = std::str::from_utf8(&buf[..n]).unwrap();
                println!("{}: {}", addr, msg.trim());
            }
            Ok(_) => {
                // Connection closed.
                return stream.shutdown(net::Shutdown::Both);
            }
            Err(err) if err.kind() == io::ErrorKind::WouldBlock => {
                // Nothing left to read.
                break;
            }
            Err(err) => {
                panic!(err);
            }
        }
    }
}
*/
fn handle_client(stream: &mut TcpStream) -> Result<()> {
    println!("Incoming connection from: {}", stream.peer_addr()?);

    stream.set_nonblocking(true)?;

    //let mut filebuffer = Vec::new();

    loop {
        let header = messages::get_header(stream)?;
        let data = messages::get_data(stream, header)?;

        match header.msg_type {
            Messages::FistbumpRequest => {
                let msg: FistbumpRequest = messages::get_message(&data)?;

                println!("target: got FistbumpRequest");

                if msg.version_major != messages::REMOTELINK_MAJOR_VERSION {
                    return Err(anyhow!("Major version miss-match (target {} host {})",
                        messages::REMOTELINK_MAJOR_VERSION, msg.version_major));
                }

                if msg.version_minor != messages::REMOTELINK_MINOR_VERSION {
                    println!("Minor version miss-matching, but continuing");
                }

                let fistbump_reply = FistbumpReply {
                    version_major: messages::REMOTELINK_MAJOR_VERSION,
                    version_minor: messages::REMOTELINK_MINOR_VERSION,
                };

                println!("target: sending data back");

                messages::send_message(stream, &fistbump_reply, Messages::FistbumpReply)?;
            }

            _ => (),
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
            Ok(mut stream) => {
                thread::spawn(move || {
                    handle_client(&mut stream).unwrap_or_else(|error| eprintln!("{:?}", error));
                });
            }
        }
    }
}
