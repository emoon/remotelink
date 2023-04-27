use serde::{Deserialize, Serialize};

pub const REMOTELINK_MAJOR_VERSION: u8 = 0;
pub const REMOTELINK_MINOR_VERSION: u8 = 1;

/// Used for read/write over the stream
//const CHUNK_SIZE: usize = 64 * 1024;

#[repr(u8)]
#[derive(Copy, Clone, PartialEq, Debug)]
pub enum Messages {
    HandshakeRequest = 0,
    HandshakeReply = 1,
    LaunchExecutableRequest = 2,
    LaunchExecutableReply = 3,
    StopExecutableRequest = 4,
    StopExecutableReply = 5,
    StdoutOutput = 6,
    NoMessage = 8,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct HandshakeRequest {
    pub version_major: u8,
    pub version_minor: u8,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct HandshakeReply {
    pub version_major: u8,
    pub version_minor: u8,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct LaunchExecutableRequest<'a> {
    pub file_server: bool,
    pub path: &'a str,
    pub data: &'a [u8],
}

#[derive(Serialize, Deserialize, Debug)]
pub struct TextMessage<'a> {
    pub data: &'a [u8],
}

#[derive(Serialize, Deserialize, Debug)]
pub struct LaunchExecutableReply<'a> {
    pub launch_status: i32,
    pub error_info: Option<&'a str>,
}

#[derive(Serialize, Deserialize, Debug, Default)]
pub struct StopExecutableRequest {
    dummy: u32,
}

#[derive(Serialize, Deserialize, Debug, Default)]
pub struct StopExecutableReply {
    dummy: u32,
}

#[derive(Copy, Clone)]
pub struct Header {
    pub msg_type: Messages,
    pub size: usize,
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
