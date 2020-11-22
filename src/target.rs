use crate::options::*;
use crate::messages;
use crate::messages::{Messages, FistbumpRequest, FistbumpReply };
use anyhow::*;
use std::net::{TcpListener, TcpStream};
use std::thread;
use std::io::{Read, Write};
use crate::message_stream::{MessageStream, TransitionToRead};
use std::process::Child;

struct Context {
    /// Used for tracking running executable.
    proc: Option<Child>,
}

impl Context

fn handle_fistbump_request(<S: Write + Read>(
    msg_stream: &mut MessageStream,
    stream: &mut S,


/// Handles incoming messages and sends back reply (if needed)
fn handle_incoming_msg<S: Write + Read>(
    msg_stream: &mut MessageStream,
    stream: &mut S,
    message: Messages,
) -> Result<()> {
    match message {
        Messages::FistbumpRequest => {
            let msg: FistbumpRequest = bincode::deserialize(&msg_stream.data)?;

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

            msg_stream.begin_write_message(stream, &fistbump_reply, Messages::FistbumpReply, TransitionToRead::Yes)?;

            println!("target: sending data back");
        }

        Messages::LaunchExecutableRequest => {
            dbg!(msg_stream.data.len());
            let file: bincode::Result<messages::LaunchExecutableRequest> = bincode::deserialize(&msg_stream.data);

            match file {
                Ok(f) => {
                    println!("Want to launch {} size {}", f.path, f.data.len());
                }

                Err(e) => {
                    dbg!(&e);
                    panic!(e);
                },
            }
        },

        _ => {
            // if we didn't handle the message switch over to waiting for new data
            dbg!(message);
        },
    }


    Ok(())
}


fn handle_client(stream: &mut TcpStream) -> Result<()> {
    println!("Incoming connection from: {}", stream.peer_addr()?);

    stream.set_nonblocking(true)?;

    let mut msg_stream = MessageStream::new();

    msg_stream.begin_read(stream, false)?;

    //let mut filebuffer = Vec::new();

    loop {
        let msg = msg_stream.update(stream)?;

        match msg {
            Some(msg) => handle_incoming_msg(&mut msg_stream, stream, msg)?,
            _ => (),
        }

        std::thread::sleep(std::time::Duration::from_millis(1));
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

