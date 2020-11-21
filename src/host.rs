//use std::fs::File;
//use std::io::{BufReader, Error, Read, Write};
use anyhow::*;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::fs::File;

use crate::message_stream::MessageStream;
use crate::messages::*;
use crate::options::Opt;

fn fistbump<T: Write + Read>(stream: &mut T) -> Result<()> {
    let fistbump_request = FistbumpRequest {
        version_major: REMOTELINK_MAJOR_VERSION,
        version_minor: REMOTELINK_MINOR_VERSION,
    };

    let mut msg_stream = MessageStream::new();

    println!("host: sending message");

    // as socket is in blocking mode at this point we expect this to return with the correct data directly
    if !msg_stream.begin_write_message(stream, &fistbump_request, Messages::FistbumpRequest)? {
        return Err(anyhow!(
            "Message write wasn't finished, should have completed directly"
        ));
    }

    match msg_stream.begin_read(stream, true)? {
        Some(msg) => {
            if msg == Messages::FistbumpReply {
                let _message: FistbumpReply = bincode::deserialize(&msg_stream.data)?;
            // TODO: validate that versions match
            } else {
                return Err(anyhow!(
                    "Incorrect message returned for FistbumpRequest {:?}",
                    msg
                ));
            }
        }

        None => {
            return Err(anyhow!(
                "Incorrect data from message reader, should have been message"
            ))
        }
    }

    Ok(())
}

/// Handles incoming messages and sends back reply (if needed)
fn handle_incoming_msg<S: Write + Read>(
    _msg_stream: &mut MessageStream,
    _stream: &mut S,
    _message: Messages,
) -> Result<()> {
    Ok(())
}

fn send_file<S: Write + Read>(
    msg_stream: &mut MessageStream,
    stream: &mut S,
    filename: &str) -> Result<()> {

    dbg!();

    let mut buffer = Vec::new();
    let mut f = File::open(filename)?;
    f.read_to_end(&mut buffer)?;

    let file_request = LaunchExecutableRequest {
        // TODO: Implement file serving
        file_server: false,
        path: filename,
        data: &buffer,
    };

    msg_stream.begin_write_message(stream, &file_request, Messages::LaunchExecutableRequest)?;

    dbg!();

    Ok(())
}

pub fn host_loop(opts: &Opt, _ip_address: &str) -> Result<()> {
    let mut stream = TcpStream::connect("127.0.0.1:8888")?;

    println!("connection made");

    fistbump(&mut stream)?;

    // set non-blocking mode after fistbump
    stream.set_nonblocking(true)?;

    let mut msg_stream = MessageStream::new();

    // read file to be sent

    if let Some(target) = opts.filename.as_ref() {
        send_file(&mut msg_stream, &mut stream, &target)?;
    }

    loop {
        dbg!();
        if let Some(msg) = msg_stream.update(&mut stream)? {
            handle_incoming_msg(&mut msg_stream, &mut stream, msg)?;
        }

        // don't hammer the CPU
        std::thread::sleep(std::time::Duration::from_millis(500));
    }
}

/*
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
*/
