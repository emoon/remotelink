use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};

pub const REMOTELINK_MAJOR_VERSION: u8 = 0;
pub const REMOTELINK_MINOR_VERSION: u8 = 1;

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
    StderrOutput = 7,
    NoMessage = 8,
}

impl Messages {
    /// Safely converts a u8 to a Messages enum variant.
    /// Returns an error if the value doesn't correspond to a valid message type.
    pub fn from_u8(value: u8) -> Result<Self> {
        match value {
            0 => Ok(Messages::HandshakeRequest),
            1 => Ok(Messages::HandshakeReply),
            2 => Ok(Messages::LaunchExecutableRequest),
            3 => Ok(Messages::LaunchExecutableReply),
            4 => Ok(Messages::StopExecutableRequest),
            5 => Ok(Messages::StopExecutableReply),
            6 => Ok(Messages::StdoutOutput),
            7 => Ok(Messages::StderrOutput),
            8 => Ok(Messages::NoMessage),
            _ => Err(anyhow!("Invalid message type: {}", value)),
        }
    }
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

#[allow(dead_code)]
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_from_u8_valid_messages() {
        assert!(matches!(Messages::from_u8(0).unwrap(), Messages::HandshakeRequest));
        assert!(matches!(Messages::from_u8(1).unwrap(), Messages::HandshakeReply));
        assert!(matches!(Messages::from_u8(2).unwrap(), Messages::LaunchExecutableRequest));
        assert!(matches!(Messages::from_u8(3).unwrap(), Messages::LaunchExecutableReply));
        assert!(matches!(Messages::from_u8(4).unwrap(), Messages::StopExecutableRequest));
        assert!(matches!(Messages::from_u8(5).unwrap(), Messages::StopExecutableReply));
        assert!(matches!(Messages::from_u8(6).unwrap(), Messages::StdoutOutput));
        assert!(matches!(Messages::from_u8(7).unwrap(), Messages::StderrOutput));
        assert!(matches!(Messages::from_u8(8).unwrap(), Messages::NoMessage));
    }

    #[test]
    fn test_from_u8_invalid_messages() {
        // Test various invalid values
        assert!(Messages::from_u8(9).is_err());
        assert!(Messages::from_u8(10).is_err());
        assert!(Messages::from_u8(255).is_err());
    }

    #[test]
    fn test_from_u8_error_message() {
        // Verify the error message is descriptive
        let result = Messages::from_u8(42);
        assert!(result.is_err());
        let err_msg = format!("{}", result.unwrap_err());
        assert!(err_msg.contains("Invalid message type"));
        assert!(err_msg.contains("42"));
    }
}
