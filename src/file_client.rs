//! Client for communicating with the remotelink file server

use crate::message_stream::{MessageStream, TransitionToRead};
use crate::messages::*;
use anyhow::{Context, Result};
use std::cell::RefCell;
use std::io::Write;
use std::net::TcpStream;
use std::path::Path;
use std::time::Duration;

/// Client for communicating with the remotelink file server
pub struct FileServerClient {
    stream: RefCell<TcpStream>,
    msg_stream: RefCell<MessageStream>,
}

impl FileServerClient {
    /// Create a new connection to the file server
    pub fn new(addr: &str) -> Result<Self> {
        let stream = TcpStream::connect(addr)
            .with_context(|| format!("Failed to connect to file server at {}", addr))?;

        stream.set_read_timeout(Some(Duration::from_secs(30)))?;
        stream.set_write_timeout(Some(Duration::from_secs(30)))?;

        let msg_stream = MessageStream::new();

        Ok(Self {
            stream: RefCell::new(stream),
            msg_stream: RefCell::new(msg_stream),
        })
    }

    /// Generic request/reply handler that eliminates boilerplate
    fn send_request_and_wait<T, R, F>(
        &self,
        request: &T,
        request_type: Messages,
        expected_reply_type: Messages,
        deserialize_and_handle: F,
    ) -> Result<R, i32>
    where
        T: serde::Serialize,
        F: FnOnce(&[u8]) -> Result<R, i32>,
    {
        let mut stream = self.stream.borrow_mut();
        let mut msg_stream = self.msg_stream.borrow_mut();

        // Send request
        msg_stream
            .begin_write_message(&mut *stream, request, request_type, TransitionToRead::Yes)
            .map_err(|_| libc::EIO)?;

        // Wait for reply
        loop {
            match msg_stream.update(&mut *stream).map_err(|_| libc::EIO)? {
                Some(msg) if msg == expected_reply_type => {
                    return deserialize_and_handle(&msg_stream.data);
                }
                Some(_) => return Err(libc::EIO),
                None => {
                    std::thread::sleep(Duration::from_millis(1));
                }
            }
        }
    }

    /// Open a file on the server
    /// Returns (handle, size) on success, or errno on error
    pub fn open(&self, path: &str) -> Result<(u32, u64), i32> {
        let request = FileOpenRequest { path };

        self.send_request_and_wait(
            &request,
            Messages::FileOpenRequest,
            Messages::FileOpenReply,
            |data| {
                let reply: FileOpenReply = bincode::deserialize(data).map_err(|_| libc::EIO)?;

                if reply.error != 0 {
                    return Err(reply.error);
                }

                Ok((reply.handle, reply.size))
            },
        )
    }

    /// Read from a file on the server
    /// Returns data on success, or errno on error
    pub fn read(&self, handle: u32, offset: u64, size: u32) -> Result<Vec<u8>, i32> {
        let request = FileReadRequest {
            handle,
            offset,
            size,
        };

        self.send_request_and_wait(
            &request,
            Messages::FileReadRequest,
            Messages::FileReadReply,
            |data| {
                let reply: FileReadReply = bincode::deserialize(data).map_err(|_| libc::EIO)?;

                if reply.error != 0 {
                    return Err(reply.error);
                }

                Ok(reply.data.to_vec())
            },
        )
    }

    /// Close a file on the server
    /// Returns () on success, or errno on error
    pub fn close(&self, handle: u32) -> Result<(), i32> {
        let request = FileCloseRequest { handle };

        self.send_request_and_wait(
            &request,
            Messages::FileCloseRequest,
            Messages::FileCloseReply,
            |data| {
                let reply: FileCloseReply = bincode::deserialize(data).map_err(|_| libc::EIO)?;

                if reply.error != 0 {
                    return Err(reply.error);
                }

                Ok(())
            },
        )
    }

    /// Get file stats from the server
    /// Returns (size, mtime) on success, or errno on error
    pub fn stat(&self, path: &str) -> Result<(u64, i64), i32> {
        let request = FileStatRequest { path };

        self.send_request_and_wait(
            &request,
            Messages::FileStatRequest,
            Messages::FileStatReply,
            |data| {
                let reply: FileStatReply = bincode::deserialize(data).map_err(|_| libc::EIO)?;

                if reply.error != 0 {
                    return Err(reply.error);
                }

                Ok((reply.size, reply.mtime))
            },
        )
    }

    /// Download a file from the server and save it locally
    pub fn download_file(&self, remote_path: &str, local_path: &Path) -> Result<(), i32> {
        // Open the remote file
        let (handle, size) = self.open(remote_path)?;

        // Read in chunks
        let mut data = Vec::with_capacity(size as usize);
        let mut offset = 0u64;
        const CHUNK_SIZE: u32 = 64 * 1024;

        while offset < size {
            let to_read = std::cmp::min(CHUNK_SIZE, (size - offset) as u32);
            let chunk = self.read(handle, offset, to_read)?;
            offset += chunk.len() as u64;
            data.extend(chunk);
        }

        // Close remote file
        let _ = self.close(handle);

        // Write locally
        let mut file = std::fs::File::create(local_path).map_err(|_| libc::EIO)?;
        file.write_all(&data).map_err(|_| libc::EIO)?;
        file.sync_all().map_err(|_| libc::EIO)?;

        // Make executable on Unix
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(local_path, std::fs::Permissions::from_mode(0o755))
                .map_err(|_| libc::EIO)?;
        }

        Ok(())
    }
}
