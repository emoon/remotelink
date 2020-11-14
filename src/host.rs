//use std::fs::File;
//use std::io::{BufReader, Error, Read, Write};
use anyhow::Result;
use std::io::{Read, Write};

use crate::messages::*;
use crate::options::Opt;

fn fistbump<T: Write + Read>(stream: T) -> Result<()> {
    let fistbump_request = FistbumpRequest {
        msg_type: Messages::FistbumpRequest as u8,
        version_major: REMOTELINK_MAJOR_VERSION,
        version_minor: REMOTELINK_MINOR_VERSION,
    };

    send_message(stream, &fistbump_request)?;

    Ok(())
}

pub fn host_loop(opts: &Opt, ip_address: &str) -> Result<()> {
    let mut stream = TcpStream::connect(ip_address)?;

    fistbump(stream)?;

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
