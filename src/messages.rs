use anyhow::Result;
use bincode;
use serde::ser::Serialize;
use serde::de::Deserialize;
use std::io::{Read, Write};
use std::mem::transmute;

pub const REMOTELINK_MAJOR_VERSION: u8 = 0;
pub const REMOTELINK_MINOR_VERSION: u8 = 1;

/// Used for read/write over the stream
const CHUNK_SIZE: usize = 64 * 1024;

#[repr(u8)]
#[derive(Copy, Clone)]
pub enum Messages {
    FistbumpRequest = 0,
    FistbumpReply = 1,
    LaunchExecutableRequest = 2,
    LaunchExecutableReply = 3,
    StopExecutableRequest = 4,
    StopExecutableReply = 5,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct FistbumpRequest {
    pub version_major: u8,
    pub version_minor: u8,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct FistbumpReply {
    version_major: u8,
    version_minor: u8,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct LaunchExecutableRequest {
    file_server: bool,
    path: String,
    data: Vec<u8>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct LaunchExecutableReplay {
    launch_status: i32,
    error_info: Option<String>,
}

#[derive(Copy, Clone)]
pub struct Header {
    pub msg_type: Messages,
    pub size: usize,
}

/// Send message over the stream
pub fn send_message<T: Serialize, S: Write + Read>(
    stream: &mut S,
    data: &T,
    msg_type: Messages,
) -> Result<()> {
    let mut header: [u8; 8] = [0; 8];

    let mut ser_data = Vec::with_capacity(1024);
    bincode::serialize_into(&mut ser_data, data)?;

    let len = ser_data.len() as u64;
    // reserve upper space for type
    assert!(len < 0xffff_ffff_ffff);
    // store type in top byte
    header[0] = msg_type as u8;
    header[1] = ((len >> 48) & 0xff) as u8;
    header[2] = ((len >> 40) & 0xff) as u8;
    header[3] = ((len >> 32) & 0xff) as u8;
    header[4] = ((len >> 24) & 0xff) as u8;
    header[5] = ((len >> 16) & 0xff) as u8;
    header[6] = ((len >> 8) & 0xff) as u8;
    header[7] = (len & 0xff) as u8;

    stream.write(&header)?;
	stream.write(&ser_data)?;

    Ok(())
}

pub fn get_header<S: Write + Read>(stream: &mut S) -> Result<Header> {
    let mut header: [u8; 8] = [0; 8];

    // read data to the header (type and size)
    stream.read_exact(&mut header)?;

    let msg_type = header[0];
    let size = ((header[1] as u64) << 48)
        | ((header[2] as u64) << 40)
        | ((header[3] as u64) << 32)
        | ((header[4] as u64) << 24)
        | ((header[5] as u64) << 16)
        | ((header[6] as u64) << 8)
        | (header[7] as u64);

	let msg_type: Messages = unsafe { transmute(msg_type) };

	Ok(Header { msg_type, size: size as usize })
}

fn read_msg_data<S: Write + Read>(stream: &mut S, header: Header) -> Result<Vec<u8>> {
    // if message is zero sized we have a basic message without any data to it
    if header.size == 0 {
        return Ok(Vec::<u8>::new());
    }

    // (large) sanity check
    assert!(header.size < 0xffff_ffff_ffff);

    let mut data = Vec::with_capacity(header.size);
	stream.read_exact(&mut data)?;

    Ok(data)
}

pub fn get_message<'a, T: Deserialize<'a>, S: Write + Read>(stream: &mut S, header: Header) -> Result<T> {
    let data = read_msg_data(stream, header)?;
    let message: T = bincode::deserialize(&data)?;
    Ok(message)
}

/*
#[derive(Serialize, Deserialize, Debug)]
pub struct OpenHandleRequest {
    msg_type: u8,
    path: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct OpenHandleReply {
    msg_type: u8,
    handle: Option<u32>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ReadRequest {
    msg_type: u8,
    handle: u32,
    size: u64,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ReadReply {
    msg_type: u8,
    data: Vec<u8>,
}

*/
