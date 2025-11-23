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
    FileOpenRequest = 9,
    FileOpenReply = 10,
    FileReadRequest = 11,
    FileReadReply = 12,
    FileCloseRequest = 13,
    FileCloseReply = 14,
    FileStatRequest = 15,
    FileStatReply = 16,
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
            9 => Ok(Messages::FileOpenRequest),
            10 => Ok(Messages::FileOpenReply),
            11 => Ok(Messages::FileReadRequest),
            12 => Ok(Messages::FileReadReply),
            13 => Ok(Messages::FileCloseRequest),
            14 => Ok(Messages::FileCloseReply),
            15 => Ok(Messages::FileStatRequest),
            16 => Ok(Messages::FileStatReply),
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

// File server protocol messages

#[derive(Serialize, Deserialize, Debug)]
pub struct FileOpenRequest<'a> {
    /// Path relative to the file server's base directory
    pub path: &'a str,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct FileOpenReply {
    /// File handle (0 means error)
    pub handle: u32,
    /// File size in bytes
    pub size: u64,
    /// Error code (0 = success, errno values for errors)
    pub error: i32,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct FileReadRequest {
    /// File handle from FileOpenReply
    pub handle: u32,
    /// Offset in file to read from
    pub offset: u64,
    /// Number of bytes to read (max 4MB)
    pub size: u32,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct FileReadReply<'a> {
    /// Data read from file
    pub data: &'a [u8],
    /// Error code (0 = success, errno values for errors)
    pub error: i32,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct FileCloseRequest {
    /// File handle to close
    pub handle: u32,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct FileCloseReply {
    /// Error code (0 = success, errno values for errors)
    pub error: i32,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct FileStatRequest<'a> {
    /// Path relative to the file server's base directory
    pub path: &'a str,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct FileStatReply {
    /// File size in bytes
    pub size: u64,
    /// Last modification time (Unix timestamp)
    pub mtime: i64,
    /// Error code (0 = success, errno values for errors)
    pub error: i32,
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
        assert!(matches!(Messages::from_u8(9).unwrap(), Messages::FileOpenRequest));
        assert!(matches!(Messages::from_u8(10).unwrap(), Messages::FileOpenReply));
        assert!(matches!(Messages::from_u8(11).unwrap(), Messages::FileReadRequest));
        assert!(matches!(Messages::from_u8(12).unwrap(), Messages::FileReadReply));
        assert!(matches!(Messages::from_u8(13).unwrap(), Messages::FileCloseRequest));
        assert!(matches!(Messages::from_u8(14).unwrap(), Messages::FileCloseReply));
        assert!(matches!(Messages::from_u8(15).unwrap(), Messages::FileStatRequest));
        assert!(matches!(Messages::from_u8(16).unwrap(), Messages::FileStatReply));
    }

    #[test]
    fn test_from_u8_invalid_messages() {
        // Test various invalid values
        assert!(Messages::from_u8(17).is_err());
        assert!(Messages::from_u8(100).is_err());
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

    #[test]
    fn test_file_open_request_serialization() {
        let request = FileOpenRequest { path: "test/file.txt" };
        let serialized = bincode::serialize(&request).unwrap();
        let deserialized: FileOpenRequest = bincode::deserialize(&serialized).unwrap();
        assert_eq!(deserialized.path, "test/file.txt");
    }

    #[test]
    fn test_file_open_reply_serialization() {
        let reply = FileOpenReply {
            handle: 42,
            size: 1024,
            error: 0,
        };
        let serialized = bincode::serialize(&reply).unwrap();
        let deserialized: FileOpenReply = bincode::deserialize(&serialized).unwrap();
        assert_eq!(deserialized.handle, 42);
        assert_eq!(deserialized.size, 1024);
        assert_eq!(deserialized.error, 0);
    }

    #[test]
    fn test_file_read_request_serialization() {
        let request = FileReadRequest {
            handle: 42,
            offset: 1024,
            size: 512,
        };
        let serialized = bincode::serialize(&request).unwrap();
        let deserialized: FileReadRequest = bincode::deserialize(&serialized).unwrap();
        assert_eq!(deserialized.handle, 42);
        assert_eq!(deserialized.offset, 1024);
        assert_eq!(deserialized.size, 512);
    }

    #[test]
    fn test_file_read_reply_serialization() {
        let data = b"Hello, World!";
        let reply = FileReadReply {
            data,
            error: 0,
        };
        let serialized = bincode::serialize(&reply).unwrap();
        let deserialized: FileReadReply = bincode::deserialize(&serialized).unwrap();
        assert_eq!(deserialized.data, data);
        assert_eq!(deserialized.error, 0);
    }

    #[test]
    fn test_file_close_request_serialization() {
        let request = FileCloseRequest { handle: 42 };
        let serialized = bincode::serialize(&request).unwrap();
        let deserialized: FileCloseRequest = bincode::deserialize(&serialized).unwrap();
        assert_eq!(deserialized.handle, 42);
    }

    #[test]
    fn test_file_close_reply_serialization() {
        let reply = FileCloseReply { error: 0 };
        let serialized = bincode::serialize(&reply).unwrap();
        let deserialized: FileCloseReply = bincode::deserialize(&serialized).unwrap();
        assert_eq!(deserialized.error, 0);
    }

    #[test]
    fn test_file_stat_request_serialization() {
        let request = FileStatRequest { path: "test/file.txt" };
        let serialized = bincode::serialize(&request).unwrap();
        let deserialized: FileStatRequest = bincode::deserialize(&serialized).unwrap();
        assert_eq!(deserialized.path, "test/file.txt");
    }

    #[test]
    fn test_file_stat_reply_serialization() {
        let reply = FileStatReply {
            size: 2048,
            mtime: 1234567890,
            error: 0,
        };
        let serialized = bincode::serialize(&reply).unwrap();
        let deserialized: FileStatReply = bincode::deserialize(&serialized).unwrap();
        assert_eq!(deserialized.size, 2048);
        assert_eq!(deserialized.mtime, 1234567890);
        assert_eq!(deserialized.error, 0);
    }
}
