use anyhow::Result;
use bincode;
use serde::ser::Serialize;
use std::io::{Write, Read};

pub const REMOTELINK_MAJOR_VERSION: u8 = 0;
pub const REMOTELINK_MINOR_VERSION: u8 = 1;

#[repr(u8)]
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
    pub msg_type: u8,
    pub version_major: u8,
    pub version_minor: u8,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct FistbumpReply {
    msg_type: u8,
    version_major: u8,
    version_minor: u8,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct LaunchExecutableRequest {
    msg_type: u8,
    msg_part: u8,
    file_server: bool,
    path: String,
    data: Vec<u8>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct LaunchExecutableReplay {
    msg_type: u8,
    launch_status: i32,
    error_info: Option<String>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct StopExecutableRequest {
    msg_type: u8,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct StopExecutableReply {
    msg_type: u8,
}

pub fn send_message<T: Serialize, S: Write + Read>(stream: S, data: &T) -> Result<()> {
    let mut ser_data = Vec::with_capacity(1024);
    ser_data.push(0u8);

    bincode::serialize_into(ser_data, data)?;

    Ok(())
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
