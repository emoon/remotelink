use anyhow::{anyhow, Context, Result};
use log::{debug, error, info, trace, warn};
use std::collections::HashMap;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::net::TcpStream;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, SystemTime};

use crate::message_stream::{MessageStream, TransitionToRead};
use crate::messages::*;

const MAX_READ_SIZE: u32 = 4 * 1024 * 1024; // 4MB max per read
const MAX_OPEN_FILES: usize = 256;
const FILE_SERVER_PORT: u16 = 8889;

/// Represents an open file on the file server
struct OpenFile {
    file: File,
    path: PathBuf,
    size: u64,
}

/// File server state shared across connections
struct FileServerState {
    base_dir: PathBuf,
    next_handle: u32,
    open_files: HashMap<u32, OpenFile>,
}

impl FileServerState {
    fn new(base_dir: PathBuf) -> Self {
        Self {
            base_dir,
            next_handle: 1,
            open_files: HashMap::new(),
        }
    }

    /// Validates and canonicalizes a path relative to base_dir
    /// Returns error if path contains ".." or escapes base_dir
    fn validate_path(&self, rel_path: &str) -> Result<PathBuf> {
        // Reject paths with ".." to prevent traversal attacks
        if rel_path.contains("..") {
            return Err(anyhow!("Path traversal not allowed"));
        }

        // Build full path
        let full_path = self.base_dir.join(rel_path);

        // Canonicalize to resolve symlinks and check if within base_dir
        let canonical = full_path
            .canonicalize()
            .with_context(|| format!("Failed to canonicalize path: {:?}", full_path))?;

        // Ensure canonical path is still within base_dir
        if !canonical.starts_with(&self.base_dir) {
            return Err(anyhow!("Path escapes base directory"));
        }

        Ok(canonical)
    }

    /// Opens a file and returns a handle
    fn open_file(&mut self, rel_path: &str) -> Result<(u32, u64)> {
        if self.open_files.len() >= MAX_OPEN_FILES {
            return Err(anyhow!("Too many open files"));
        }

        let path = self.validate_path(rel_path)?;

        // Check if path is a directory - don't allow opening directories as files
        if path.is_dir() {
            return Err(anyhow!("Cannot open directory as file: {:?}", path));
        }

        let file = File::open(&path).with_context(|| format!("Failed to open file: {:?}", path))?;

        let size = file
            .metadata()
            .with_context(|| format!("Failed to get file metadata: {:?}", path))?
            .len();

        let handle = self.next_handle;
        self.next_handle = self.next_handle.wrapping_add(1);
        if self.next_handle == 0 {
            self.next_handle = 1; // Skip 0 as it indicates error
        }

        self.open_files
            .insert(handle, OpenFile { file, path, size });

        debug!(
            "Opened file handle {} for {:?} (size: {} bytes)",
            handle, rel_path, size
        );

        Ok((handle, size))
    }

    /// Reads data from an open file
    fn read_file(&mut self, handle: u32, offset: u64, size: u32) -> Result<Vec<u8>> {
        if size > MAX_READ_SIZE {
            return Err(anyhow!("Read size exceeds maximum ({})", MAX_READ_SIZE));
        }

        let open_file = self
            .open_files
            .get_mut(&handle)
            .ok_or_else(|| anyhow!("Invalid file handle"))?;

        if offset >= open_file.size {
            // Reading past end of file returns empty data
            return Ok(Vec::new());
        }

        // Clamp read size to file size
        let bytes_available = open_file.size - offset;
        let bytes_to_read = std::cmp::min(size as u64, bytes_available) as usize;

        // Seek to offset
        open_file
            .file
            .seek(SeekFrom::Start(offset))
            .with_context(|| {
                format!(
                    "Failed to seek to offset {} in {:?}",
                    offset, open_file.path
                )
            })?;

        // Read data
        let mut buffer = vec![0u8; bytes_to_read];
        open_file.file.read_exact(&mut buffer).with_context(|| {
            format!(
                "Failed to read {} bytes from {:?}",
                bytes_to_read, open_file.path
            )
        })?;

        trace!(
            "Read {} bytes from handle {} at offset {}",
            buffer.len(),
            handle,
            offset
        );

        Ok(buffer)
    }

    /// Closes an open file
    fn close_file(&mut self, handle: u32) -> Result<()> {
        self.open_files
            .remove(&handle)
            .ok_or_else(|| anyhow!("Invalid file handle"))?;

        debug!("Closed file handle {}", handle);

        Ok(())
    }

    /// Gets file statistics
    /// Returns (size, mtime, is_dir)
    fn stat_file(&self, rel_path: &str) -> Result<(u64, i64, bool)> {
        let path = self.validate_path(rel_path)?;

        let metadata =
            std::fs::metadata(&path).with_context(|| format!("Failed to stat file: {:?}", path))?;

        let size = metadata.len();
        let mtime = metadata
            .modified()
            .ok()
            .and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        let is_dir = metadata.is_dir();

        Ok((size, mtime, is_dir))
    }

    /// Reads directory entries
    fn read_dir(&self, rel_path: &str) -> Result<Vec<crate::messages::DirEntry>> {
        let path = self.validate_path(rel_path)?;

        let mut entries = Vec::new();
        for entry in std::fs::read_dir(&path)
            .with_context(|| format!("Failed to read directory: {:?}", path))?
        {
            let entry = entry?;
            let name = entry.file_name().to_string_lossy().to_string();
            let is_dir = entry.file_type()?.is_dir();
            entries.push(crate::messages::DirEntry { name, is_dir });
        }

        Ok(entries)
    }
}

/// Handles a single file server client connection
fn handle_file_client(mut stream: TcpStream, state: Arc<Mutex<FileServerState>>) -> Result<()> {
    let peer_addr = stream
        .peer_addr()
        .unwrap_or_else(|_| "unknown:0".parse().unwrap());

    info!("File server: connection from {}", peer_addr);

    // Set timeouts and disable Nagle for faster responses
    stream.set_read_timeout(Some(Duration::from_secs(30)))?;
    stream.set_write_timeout(Some(Duration::from_secs(30)))?;
    stream.set_nodelay(true)?;
    stream.set_nonblocking(true)?;

    let mut msg_stream = MessageStream::new();
    msg_stream.begin_read(&mut stream, false)?;

    loop {
        let msg = match msg_stream.update(&mut stream) {
            Ok(Some(msg)) => msg,
            Ok(None) => {
                // No message yet, sleep briefly
                thread::sleep(Duration::from_millis(1));
                continue;
            }
            Err(e) => {
                let err_str = e.to_string();
                // WouldBlock is normal for non-blocking sockets - just continue
                if err_str.contains("WouldBlock") {
                    thread::sleep(Duration::from_millis(1));
                    continue;
                }
                // Check if it's a clean disconnect
                if err_str.contains("UnexpectedEof") || err_str.contains("Connection reset") {
                    debug!(
                        "File server: client {} disconnected: {}",
                        peer_addr, err_str
                    );
                    return Ok(());
                }
                warn!("File server: error reading from {}: {}", peer_addr, e);
                return Err(e).context("Failed to read message");
            }
        };

        info!("File server: received message {:?} from {}", msg, peer_addr);

        match msg {
            Messages::FileOpenRequest => {
                let request: FileOpenRequest = bincode::deserialize(&msg_stream.data)?;
                info!("File server: open request for path: {:?}", request.path);

                let reply = match state.lock().unwrap().open_file(request.path) {
                    Ok((handle, size)) => FileOpenReply {
                        handle,
                        size,
                        error: 0,
                    },
                    Err(e) => {
                        debug!("File server: file not found {:?}: {}", request.path, e);
                        FileOpenReply {
                            handle: 0,
                            size: 0,
                            error: libc::ENOENT, // File not found
                        }
                    }
                };

                msg_stream.begin_write_message(
                    &mut stream,
                    &reply,
                    Messages::FileOpenReply,
                    TransitionToRead::Yes,
                )?;
            }

            Messages::FileReadRequest => {
                let request: FileReadRequest = bincode::deserialize(&msg_stream.data)?;

                let reply_data: Vec<u8>;
                let reply = match state.lock().unwrap().read_file(
                    request.handle,
                    request.offset,
                    request.size,
                ) {
                    Ok(data) => {
                        reply_data = data;
                        FileReadReply {
                            data: &reply_data,
                            error: 0,
                        }
                    }
                    Err(e) => {
                        warn!(
                            "File server: failed to read handle {}: {}",
                            request.handle, e
                        );
                        reply_data = Vec::new();
                        FileReadReply {
                            data: &reply_data,
                            error: libc::EIO, // I/O error
                        }
                    }
                };

                msg_stream.begin_write_message(
                    &mut stream,
                    &reply,
                    Messages::FileReadReply,
                    TransitionToRead::Yes,
                )?;
            }

            Messages::FileCloseRequest => {
                let request: FileCloseRequest = bincode::deserialize(&msg_stream.data)?;

                let reply = match state.lock().unwrap().close_file(request.handle) {
                    Ok(()) => FileCloseReply { error: 0 },
                    Err(e) => {
                        warn!(
                            "File server: failed to close handle {}: {}",
                            request.handle, e
                        );
                        FileCloseReply {
                            error: libc::EBADF, // Bad file descriptor
                        }
                    }
                };

                msg_stream.begin_write_message(
                    &mut stream,
                    &reply,
                    Messages::FileCloseReply,
                    TransitionToRead::Yes,
                )?;
            }

            Messages::FileStatRequest => {
                let request: FileStatRequest = bincode::deserialize(&msg_stream.data)?;

                let reply = match state.lock().unwrap().stat_file(request.path) {
                    Ok((size, mtime, is_dir)) => FileStatReply {
                        size,
                        mtime,
                        is_dir,
                        error: 0,
                    },
                    Err(e) => {
                        debug!("File server: stat failed {:?}: {}", request.path, e);
                        FileStatReply {
                            size: 0,
                            mtime: 0,
                            is_dir: false,
                            error: libc::ENOENT, // File not found
                        }
                    }
                };

                msg_stream.begin_write_message(
                    &mut stream,
                    &reply,
                    Messages::FileStatReply,
                    TransitionToRead::Yes,
                )?;
            }

            Messages::FileReaddirRequest => {
                let request: FileReaddirRequest = bincode::deserialize(&msg_stream.data)?;
                debug!("File server: readdir request for path: {:?}", request.path);

                let reply = match state.lock().unwrap().read_dir(request.path) {
                    Ok(entries) => FileReaddirReply { entries, error: 0 },
                    Err(e) => {
                        debug!("File server: readdir failed {:?}: {}", request.path, e);
                        FileReaddirReply {
                            entries: Vec::new(),
                            error: libc::ENOENT,
                        }
                    }
                };

                msg_stream.begin_write_message(
                    &mut stream,
                    &reply,
                    Messages::FileReaddirReply,
                    TransitionToRead::Yes,
                )?;
            }

            _ => {
                warn!(
                    "File server: unexpected message type {:?} from {}",
                    msg, peer_addr
                );
                return Err(anyhow!("Unexpected message type"));
            }
        }
    }
}

/// Starts the file server in a background thread
pub fn start_file_server(base_dir: String) -> Result<thread::JoinHandle<()>> {
    start_file_server_on_port(base_dir, FILE_SERVER_PORT)
}

/// Starts the file server on a specific port (for testing)
pub fn start_file_server_on_port(base_dir: String, port: u16) -> Result<thread::JoinHandle<()>> {
    let base_path = PathBuf::from(&base_dir);

    // Validate base directory exists
    if !base_path.exists() {
        return Err(anyhow!(
            "File server directory does not exist: {:?}",
            base_path
        ));
    }

    if !base_path.is_dir() {
        return Err(anyhow!(
            "File server path is not a directory: {:?}",
            base_path
        ));
    }

    // Canonicalize to get absolute path
    let canonical_base = base_path
        .canonicalize()
        .with_context(|| format!("Failed to canonicalize base directory: {:?}", base_path))?;

    info!(
        "Starting file server on port {} serving {:?}",
        port, canonical_base
    );

    let state = Arc::new(Mutex::new(FileServerState::new(canonical_base)));

    let handle = thread::Builder::new()
        .name("file_server".to_string())
        .spawn(move || {
            let bind_addr = format!("0.0.0.0:{}", port);

            // Create socket manually to set SO_REUSEADDR before binding
            let listener = {
                use std::net::{Ipv4Addr, SocketAddr};
                use std::os::unix::io::FromRawFd;

                unsafe {
                    // Create socket
                    let fd = libc::socket(libc::AF_INET, libc::SOCK_STREAM, 0);
                    if fd < 0 {
                        error!("File server failed to create socket");
                        return;
                    }

                    // Set SO_REUSEADDR
                    let optval: libc::c_int = 1;
                    if libc::setsockopt(
                        fd,
                        libc::SOL_SOCKET,
                        libc::SO_REUSEADDR,
                        &optval as *const _ as *const libc::c_void,
                        std::mem::size_of_val(&optval) as libc::socklen_t,
                    ) < 0
                    {
                        error!("File server failed to set SO_REUSEADDR");
                        libc::close(fd);
                        return;
                    }

                    // Bind
                    let addr =
                        SocketAddr::new(std::net::IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)), port);
                    let sock_addr = match addr {
                        SocketAddr::V4(addr) => {
                            let mut storage: libc::sockaddr_in = std::mem::zeroed();
                            storage.sin_family = libc::AF_INET as libc::sa_family_t;
                            storage.sin_port = addr.port().to_be();
                            storage.sin_addr = libc::in_addr {
                                s_addr: u32::from(*addr.ip()).to_be(),
                            };
                            storage
                        }
                        _ => {
                            error!("File server only supports IPv4");
                            libc::close(fd);
                            return;
                        }
                    };

                    if libc::bind(
                        fd,
                        &sock_addr as *const _ as *const libc::sockaddr,
                        std::mem::size_of_val(&sock_addr) as libc::socklen_t,
                    ) < 0
                    {
                        error!("File server failed to bind to {}", bind_addr);
                        libc::close(fd);
                        return;
                    }

                    // Listen
                    if libc::listen(fd, 128) < 0 {
                        error!("File server failed to listen");
                        libc::close(fd);
                        return;
                    }

                    // Convert to TcpListener
                    std::net::TcpListener::from_raw_fd(fd)
                }
            };

            info!("File server listening on {}", bind_addr);

            for stream in listener.incoming() {
                match stream {
                    Ok(stream) => {
                        let state_clone = Arc::clone(&state);
                        thread::spawn(move || {
                            if let Err(e) = handle_file_client(stream, state_clone) {
                                error!("File server client error: {}", e);
                            }
                        });
                    }
                    Err(e) => {
                        error!("File server failed to accept connection: {}", e);
                        break; // Break on error (allows clean shutdown)
                    }
                }
            }

            info!("File server shutting down");
        })
        .context("Failed to spawn file server thread")?;

    Ok(handle)
}
