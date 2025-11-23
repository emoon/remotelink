use crate::message_stream::{MessageStream, TransitionToRead};
use crate::messages;
use crate::messages::*;
use crate::options::*;
use anyhow::{anyhow, Context as AnyhowContext, Result};
use core::result::Result::Ok;
use log::{trace, error, info, warn, debug};
use std::{
    fs::File,
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    path::PathBuf,
    process::{Child, Command, Stdio},
    sync::{
        mpsc::{Receiver, Sender, channel},
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    thread,
    time::Duration,
};
use uuid::Uuid;

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

type IoOut = Receiver<Vec<u8>>;

#[derive(Default)]
struct Context {
    /// Used for tracking running executable.
    stdout: Option<IoOut>,
    /// Used for tracking running executable.
    stderr: Option<IoOut>,
    /// Used for tracking running executable.
    proc: Option<Child>,
    /// Path to the temporary executable file for cleanup
    temp_exe_path: Option<PathBuf>,
}

impl Context {
    /// Clean up all resources associated with this context
    fn cleanup(&mut self) {
        trace!("Starting cleanup");

        // Kill and reap the process if still running
        if let Some(mut proc) = self.proc.take() {
            match proc.try_wait() {
                Ok(Some(status)) => {
                    info!("Process already exited with status: {}", status);
                }
                Ok(None) => {
                    // Process still running, kill it
                    warn!("Process still running during cleanup, killing...");
                    if let Err(e) = proc.kill() {
                        error!("Failed to kill process: {}", e);
                    }
                    // Wait to reap zombie
                    match proc.wait() {
                        Ok(status) => info!("Process killed, exit status: {}", status),
                        Err(e) => error!("Failed to wait on killed process: {}", e),
                    }
                }
                Err(e) => {
                    error!("Error checking process status: {}", e);
                    // Try to kill anyway
                    let _ = proc.kill();
                    let _ = proc.wait();
                }
            }
        }

        // Close stdout/stderr channels
        if self.stdout.take().is_some() {
            trace!("Closed stdout channel");
        }
        if self.stderr.take().is_some() {
            trace!("Closed stderr channel");
        }

        // Delete temporary executable file
        if let Some(path) = self.temp_exe_path.take() {
            match std::fs::remove_file(&path) {
                Ok(()) => info!("Cleaned up temp file: {:?}", path),
                Err(e) => error!("Failed to remove temp file {:?}: {}", path, e),
            }
        }

        trace!("Cleanup complete");
    }

    /// Log current resource status for debugging
    #[allow(dead_code)]
    fn log_resource_status(&self) {
        debug!(
            "Context resources - Process: {}, Stdout: {}, Stderr: {}, Temp file: {}",
            self.proc.is_some(),
            self.stdout.is_some(),
            self.stderr.is_some(),
            self.temp_exe_path.is_some()
        );
    }

    /// Handles incoming messages and sends back reply (if needed) if returns false it means we
    /// should exit the update
    pub fn handle_incoming_msg<S: Write + Read>(
        &mut self,
        msg_stream: &mut MessageStream,
        stream: &mut S,
        message: Messages,
    ) -> Result<bool> {
        match message {
            Messages::HandshakeRequest => {
                let msg: HandshakeRequest = bincode::deserialize(&msg_stream.data)?;

                if msg.version_major != messages::REMOTELINK_MAJOR_VERSION {
                    return Err(anyhow!(
                        "Major version miss-match (target {} host {})",
                        messages::REMOTELINK_MAJOR_VERSION,
                        msg.version_major
                    ));
                }

                if msg.version_minor != messages::REMOTELINK_MINOR_VERSION {
                    println!("Minor version miss-matching, but continuing");
                }

                let handshake_reply = HandshakeReply {
                    version_major: messages::REMOTELINK_MAJOR_VERSION,
                    version_minor: messages::REMOTELINK_MINOR_VERSION,
                };

                msg_stream.begin_write_message(
                    stream,
                    &handshake_reply,
                    Messages::HandshakeReply,
                    TransitionToRead::Yes,
                )?;
            }

            Messages::StopExecutableRequest => {
                info!("Received stop request");

                if let Some(mut proc) = self.proc.take() {
                    match proc.kill() {
                        Ok(()) => {
                            // Wait to reap the process
                            match proc.wait() {
                                Ok(status) => {
                                    info!("Process stopped, exit status: {}", status);
                                }
                                Err(e) => {
                                    error!("Failed to wait on stopped process: {}", e);
                                }
                            }
                        }
                        Err(e) => {
                            error!("Failed to kill process: {}", e);
                        }
                    }
                } else {
                    warn!("Stop request but no process running");
                };

                // Clean up all resources
                self.cleanup();

                // Send reply
                let stop_reply = StopExecutableReply::default();

                msg_stream.begin_write_message(
                    stream,
                    &stop_reply,
                    Messages::StopExecutableReply,
                    TransitionToRead::Yes,
                )?;

                return Ok(false);
            }

            Messages::LaunchExecutableRequest => {
                trace!("LaunchExecutableRequest");

                let file: bincode::Result<messages::LaunchExecutableRequest> =
                    bincode::deserialize(&msg_stream.data);

                match file {
                    Ok(f) => {
                        match self.start_executable(&f) {
                            Ok(()) => {
                                // Launch successful - don't send reply yet
                                // Will send LaunchExecutableReply when process exits
                                trace!("Executable launched successfully");
                            }
                            Err(e) => {
                                error!("Failed to start executable: {}", e);

                                let exe_launch = LaunchExecutableReply {
                                    launch_status: -1,
                                    error_info: Some("Failed to launch executable"),
                                };

                                msg_stream.begin_write_message(
                                    stream,
                                    &exe_launch,
                                    Messages::LaunchExecutableReply,
                                    TransitionToRead::Yes,
                                )?;
                            }
                        }
                    }

                    Err(e) => {
                        error!("Failed to deserialize LaunchExecutableRequest: {}", e);

                        let exe_launch = LaunchExecutableReply {
                            launch_status: -1,
                            error_info: Some("Invalid message format"),
                        };

                        msg_stream.begin_write_message(
                            stream,
                            &exe_launch,
                            Messages::LaunchExecutableReply,
                            TransitionToRead::Yes,
                        )?;
                    }
                }
            }

            _ => {
                // if we didn't handle the message switch over to waiting for new data
                dbg!(message);
            }
        }

        Ok(true)
    }

    /// Pipe streams are blocking, we need separate threads to monitor them without blocking the primary thread.
    fn child_stream_to_vec<R>(mut stream: R, out: Sender<Vec<u8>>)
    where
        R: Read + Send + 'static,
    {
        const IO_BUFFER_SIZE: usize = 4096;  // Standard page size for optimal performance
        if let Err(e) = thread::Builder::new()
            .name("child_stream_to_vec".into())
            .spawn(move || loop {
                let mut buf = [0u8; IO_BUFFER_SIZE];
                match stream.read(&mut buf) {
                    Err(err) => {
                        error!("{}] Error reading from stream: {}", line!(), err);
                        break;
                    }
                    Ok(got) => {
                        if got == 0 {
                            break;
                        }

                        let mut vec = Vec::with_capacity(got);
                        vec.extend_from_slice(&buf[..got]);
                        // TODO: Fix this
                        let _ = out.send(vec);
                    }
                }
            })
        {
            error!("Failed to spawn child_stream_to_vec thread: {}", e);
        }
    }

    fn start_executable(&mut self, f: &messages::LaunchExecutableRequest) -> Result<()> {
        trace!("Want to launch {} size {}", f.path, f.data.len());

        // Generate unique temp file
        let temp_dir = std::env::temp_dir();
        let unique_name = format!("remotelink-{}", Uuid::new_v4());

        #[cfg(unix)]
        let exe_path = temp_dir.join(unique_name);

        #[cfg(windows)]
        let exe_path = temp_dir.join(format!("{}.exe", unique_name));

        info!("Creating temp executable: {:?}", exe_path);

        // Write executable data
        let mut file = File::create(&exe_path)
            .with_context(|| format!("Failed to create temp executable at {:?}", exe_path))?;

        file.write_all(f.data)
            .with_context(|| "Failed to write executable data")?;

        // Ensure all data is written to disk
        file.sync_all()
            .with_context(|| "Failed to sync executable to disk")?;

        drop(file);  // Close file before executing

        // Make executable (Unix only)
        #[cfg(unix)]
        {
            std::fs::set_permissions(&exe_path, std::fs::Permissions::from_mode(0o700))
                .with_context(|| format!("Failed to set executable permissions on {:?}", exe_path))?;
        }

        // Build command with environment variables
        let mut cmd = Command::new(&exe_path);
        cmd.stderr(Stdio::piped()).stdout(Stdio::piped());

        // Set REMOTELINK_FILE_SERVER and LD_PRELOAD environment variables if file server is enabled
        if f.file_server {
            // Get the host address from the environment or use localhost
            // The host will be running the file server on port 8889
            let file_server_addr = std::env::var("REMOTELINK_HOST")
                .unwrap_or_else(|_| "127.0.0.1".to_string());
            let file_server = format!("{}:8889", file_server_addr);
            cmd.env("REMOTELINK_FILE_SERVER", &file_server);
            info!("Setting REMOTELINK_FILE_SERVER={}", file_server);

            // Set LD_PRELOAD to enable file interception
            // Try common locations for the preload library
            let preload_paths = vec![
                "/usr/local/lib/libremotelink_preload.so",
                "/usr/lib/libremotelink_preload.so",
                "./target/release/libremotelink_preload.so",
                "./target/debug/libremotelink_preload.so",
            ];

            for path in &preload_paths {
                if std::path::Path::new(path).exists() {
                    cmd.env("LD_PRELOAD", path);
                    info!("Setting LD_PRELOAD={}", path);
                    break;
                }
            }
        }

        // Spawn the executable
        let mut p = cmd
            .spawn()
            .with_context(|| format!("Failed to spawn executable: {:?}", exe_path))?;

        info!("Spawned process with PID: {:?}", p.id());

        let (stdout_tx, stdout_rx) = channel();
        let (stderr_tx, stderr_rx) = channel();

        let stdout = p.stdout.take()
            .ok_or_else(|| anyhow::anyhow!("Failed to capture stdout"))?;
        let stderr = p.stderr.take()
            .ok_or_else(|| anyhow::anyhow!("Failed to capture stderr"))?;

        Self::child_stream_to_vec(stdout, stdout_tx);
        Self::child_stream_to_vec(stderr, stderr_tx);

        self.stdout = Some(stdout_rx);
        self.stderr = Some(stderr_rx);
        self.proc = Some(p);
        self.temp_exe_path = Some(exe_path);  // Store for cleanup

        Ok(())
    }
}

/// Generic helper to send data from a receiver if available
fn send_output<S: Write + Read>(
    receiver: Option<&mut IoOut>,
    msg_stream: &mut MessageStream,
    stream: &mut S,
    message_type: Messages,
) -> Result<()> {
    let receiver = match receiver {
        Some(r) => r,
        None => return Ok(()),
    };

    let data = match receiver.try_recv() {
        Ok(data) => data,
        Err(_) => return Ok(()),
    };

    if data.is_empty() {
        return Ok(());
    }

    let text_message = TextMessage { data: &data };
    msg_stream.begin_write_message(
        stream,
        &text_message,
        message_type,
        TransitionToRead::Yes,
    )?;

    Ok(())
}

/// Helper to send stdout data if available
fn send_stdout<S: Write + Read>(
    context: &mut Context,
    msg_stream: &mut MessageStream,
    stream: &mut S,
) -> Result<()> {
    send_output(context.stdout.as_mut(), msg_stream, stream, Messages::StdoutOutput)
}

/// Helper to send stderr data if available
fn send_stderr<S: Write + Read>(
    context: &mut Context,
    msg_stream: &mut MessageStream,
    stream: &mut S,
) -> Result<()> {
    send_output(context.stderr.as_mut(), msg_stream, stream, Messages::StderrOutput)
}

impl Drop for Context {
    fn drop(&mut self) {
        // Ensure cleanup happens even if not explicitly called
        // This handles cases where the connection drops or errors occur
        self.cleanup();
    }
}

fn handle_client(stream: &mut TcpStream, opts: &Opt) -> Result<()> {
    let peer_addr = stream.peer_addr()
        .unwrap_or_else(|_| "unknown:0".parse().unwrap());

    info!("Incoming connection from: {}", peer_addr);

    // Configure timeouts before any operations
    if let Err(e) = crate::configure_stream_timeouts(
        stream,
        Duration::from_secs(opts.read_timeout_secs),
        Duration::from_secs(opts.write_timeout_secs),
        Duration::from_secs(opts.keepalive_secs),
    ) {
        error!("Failed to configure stream timeouts for {}: {}", peer_addr, e);
        return Err(e);
    }

    stream.set_nonblocking(true)?;

    let mut msg_stream = MessageStream::new();

    msg_stream.begin_read(stream, false)?;

    // Setup a context so we can keep track of a running process and such
    let mut context = Context::default();

    loop {
        let msg = msg_stream.update(stream)?;

        if let Some(msg) = msg {
            if !context.handle_incoming_msg(&mut msg_stream, stream, msg)? {
                info!("exit client");
                return Ok(());
            }
        }

        // Send stdout and stderr data if available
        send_stdout(&mut context, &mut msg_stream, stream)?;
        send_stderr(&mut context, &mut msg_stream, stream)?;

        // If there isn't much going on we sleep for 1 ms to not hammer the CPU
        if context.stdout.is_none() && context.stderr.is_none() {
            std::thread::sleep(std::time::Duration::from_millis(1));
        }

        // Check if process has exited
        if let Some(proc) = context.proc.as_mut() {
            match proc.try_wait() {
                Ok(Some(exit_status)) => {
                    let exit_code = exit_status.code().unwrap_or(-1);
                    info!("Process exited with code: {}", exit_code);

                    // Send exit notification to client
                    let exit_message = messages::LaunchExecutableReply {
                        launch_status: exit_code,
                        error_info: None,
                    };

                    msg_stream.begin_write_message(
                        stream,
                        &exit_message,
                        Messages::LaunchExecutableReply,
                        TransitionToRead::Yes,
                    )?;

                    // Clean up resources
                    context.cleanup();

                    // Continue accepting commands (or could return Ok(()) to close connection)
                }
                Ok(None) => {
                    // Process still running, continue
                }
                Err(e) => {
                    error!("Error checking process status: {}", e);
                    // Clean up on error
                    context.cleanup();
                    return Err(e.into());
                }
            }
        }
    }
}

pub fn update(opts: &Opt) -> Result<()> {
    let bind_addr = format!("{}:{}", opts.bind_address, opts.port);
    let listener = TcpListener::bind(&bind_addr)
        .with_context(|| format!("Failed to bind to {}", bind_addr))?;

    info!("Remote runner listening on {}", bind_addr);

    // Warn if binding to all interfaces
    if opts.bind_address == "0.0.0.0" {
        warn!("Binding to 0.0.0.0 - accessible from all network interfaces!");
        warn!("Consider using 127.0.0.1 for local-only access");
    }

    let active_connections = Arc::new(AtomicUsize::new(0));
    let max_connections = opts.max_connections;

    info!("Waiting for incoming host connection");
    info!("Connection limit set to {}", max_connections);

    for stream in listener.incoming() {
        match stream {
            Ok(mut stream) => {
                // Check connection limit
                let current_connections = active_connections.load(Ordering::SeqCst);

                if current_connections >= max_connections {
                    warn!(
                        "Connection limit reached ({}/{}), rejecting connection from {:?}",
                        current_connections,
                        max_connections,
                        stream.peer_addr()
                    );
                    // Connection will be dropped, closing it
                    continue;
                }

                // Accept connection
                let peer_addr = stream.peer_addr()
                    .unwrap_or_else(|_| "unknown:0".parse().unwrap());

                // Increment counter
                active_connections.fetch_add(1, Ordering::SeqCst);
                let current = active_connections.load(Ordering::SeqCst);
                info!("Connection accepted from {} ({}/{} active)",
                      peer_addr, current, max_connections);

                // Clone counter for thread
                let conn_counter = Arc::clone(&active_connections);
                let opts_clone = opts.clone();

                // Spawn handler thread
                thread::spawn(move || {
                    if let Err(e) = handle_client(&mut stream, &opts_clone) {
                        error!("Client handler error for {}: {}", peer_addr, e);
                    }

                    // Decrement counter when done
                    let remaining = conn_counter.fetch_sub(1, Ordering::SeqCst) - 1;
                    info!("Connection closed for {} ({} active)", peer_addr, remaining);
                });
            }
            Err(e) => {
                error!("Failed to accept connection: {}", e);
            }
        }
    }
    Ok(())
}
