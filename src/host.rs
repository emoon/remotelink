//use std::fs::File;
//use std::io::{BufReader, Error, Read, Write};
use anyhow::*;
use std::io::{Read, Write};
use std::net::TcpStream;

use crate::messages::*;
use crate::messages;
use crate::options::Opt;

fn fistbump<T: Write + Read>(stream: &mut T) -> Result<()> {
    let fistbump_request = FistbumpRequest {
        version_major: REMOTELINK_MAJOR_VERSION,
        version_minor: REMOTELINK_MINOR_VERSION,
    };

    println!("host: sending message");

    send_message(stream, &fistbump_request, Messages::FistbumpRequest)?;

    // expect reply message here directly

    println!("host: reading message header");
    let header = messages::get_header(stream)?;
    println!("host: reading data");
    let data = messages::get_data(stream, header)?;

    if header.msg_type == Messages::FistbumpReply {
        let _msg: FistbumpReply = messages::get_message(&data)?;
        // TODO: Handle miss-matching version here
    } else {
        return Err(anyhow!("Incorrect message returned for FistbumpRequest {:?}", header.msg_type));
    }

    Ok(())
}

pub fn host_loop(_opts: &Opt, _ip_address: &str) -> Result<()> {
    let mut stream = TcpStream::connect("127.0.0.1:8888")?;
    stream.set_nonblocking(true)?;

    println!("connection made");

    fistbump(&mut stream)?;

    Ok(())

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
}
