use crate::message_stream::{MessageStream, TransitionToRead};
use crate::messages;
use crate::messages::*;
use crate::options::*;
use anyhow::{anyhow, Context as AnyhowContext, Result};
use core::result::Result::Ok;
use log::{trace, error, info};
use std::{
    fs::File,
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    path::PathBuf,
    process::{Child, Command, Stdio},
    sync::mpsc::{Receiver, Sender, channel},
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
                trace!("StopExecutableRequest");

                if let Some(proc) = self.proc.as_mut() {
                    proc.kill()?;
                }

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
                                let exe_launch = LaunchExecutableReply {
                                    launch_status: 0,
                                    error_info: None,
                                };

                                msg_stream.begin_write_message(
                                    stream,
                                    &exe_launch,
                                    Messages::LaunchExecutableReply,
                                    TransitionToRead::Yes,
                                )?;
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
        if let Err(e) = thread::Builder::new()
            .name("child_stream_to_vec".into())
            .spawn(move || loop {
                let mut buf = [0u8; 2];
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

        // Spawn the executable
        let mut p = Command::new(&exe_path)
            .stderr(Stdio::piped())
            .stdout(Stdio::piped())
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

        if let Some(stdout) = context.stdout.as_mut() {
            if let Ok(data) = stdout.try_recv() {
                if !data.is_empty() {
                    let text_message = TextMessage { data: &data };

                    msg_stream.begin_write_message(
                        stream,
                        &text_message,
                        Messages::StdoutOutput,
                        TransitionToRead::Yes,
                    )?;
                }
            }
        } else {
            // If there isn't much going on we sleep for 1 ms to not hammer the CPU
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
    }
}

pub fn update(opts: &Opt) -> Result<()> {
    let listener = TcpListener::bind("0.0.0.0:8888")
        .with_context(|| "Failed to bind to 0.0.0.0:8888")?;
    info!("Waiting for incoming host connection");
    for stream in listener.incoming() {
        match stream {
            Err(e) => error!("Failed to accept incoming connection: {}", e),
            Ok(mut stream) => {
                let opts_clone = opts.clone();
                thread::spawn(move || {
                    handle_client(&mut stream, &opts_clone).unwrap_or_else(|error| error!("{:?}", error));
                });
            }
        }
    }
    Ok(())
}
