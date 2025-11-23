use anyhow::{anyhow, Context, Result};
use log::{debug, info, trace, warn};
use std::fs::File;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::Path;
use std::sync::mpsc::channel;
use std::time::Duration;

use crate::file_watcher::FileWatcher;
use crate::message_stream::{MessageStream, TransitionToRead};
use crate::messages::*;
use crate::options::Opt;

fn handshake<T: Write + Read>(stream: &mut T) -> Result<()> {
    let handshake_request = HandshakeRequest {
        version_major: REMOTELINK_MAJOR_VERSION,
        version_minor: REMOTELINK_MINOR_VERSION,
    };

    let mut msg_stream = MessageStream::new();

    // as socket is in blocking mode at this point we expect this to return with the correct data directly
    if !msg_stream.begin_write_message(
        stream,
        &handshake_request,
        Messages::HandshakeRequest,
        TransitionToRead::No,
    )? {
        return Err(anyhow!(
            "Message write wasn't finished, should have completed directly"
        ));
    }

    match msg_stream.begin_read(stream, true)? {
        Some(msg) => {
            if msg == Messages::HandshakeReply {
                let _message: HandshakeReply = bincode::deserialize(&msg_stream.data)?;
            // TODO: validate that versions match
            } else {
                return Err(anyhow!(
                    "Incorrect message returned for HandshakeRequest {:?}",
                    msg
                ));
            }
        }

        None => {
            return Err(anyhow!(
                "Incorrect data from message reader, should have been message"
            ))
        }
    }

    Ok(())
}

/// Handles incoming messages and sends back reply (if needed)
/// Returns Ok(Some(true)) to continue with process running
/// Returns Ok(Some(false)) to continue with process not running (exited)
/// Returns Ok(None) to exit the host loop entirely
fn handle_incoming_msg<S: Write + Read>(
    msg_stream: &mut MessageStream,
    stream: &mut S,
    message: Messages,
    watch_mode: bool,
) -> Result<Option<bool>> {
    trace!("Message received: {:?}", message);

    match message {
        Messages::StdoutOutput => {
            let msg: TextMessage = bincode::deserialize(&msg_stream.data)?;
            let text = std::str::from_utf8(msg.data)?;
            print!("{}", text);
            std::io::stdout().flush()?;
        }

        Messages::StderrOutput => {
            let msg: TextMessage = bincode::deserialize(&msg_stream.data)?;
            let text = std::str::from_utf8(msg.data)?;
            eprint!("{}", text);
            std::io::stderr().flush()?;
        }

        Messages::LaunchExecutableReply => {
            let reply: LaunchExecutableReply = bincode::deserialize(&msg_stream.data)?;
            if reply.launch_status != 0 {
                if let Some(error) = reply.error_info {
                    log::error!("Executable failed: {}", error);
                }
                log::error!("Process exited with status: {}", reply.launch_status);
            } else {
                trace!("Process finished with status: {}", reply.launch_status);
            }

            // In watch mode, process exit just means we're ready for next version
            // In normal mode, we exit the host loop
            if watch_mode {
                info!("Process exited in watch mode, waiting for next file change");
                msg_stream.begin_read(stream, false)?;
                return Ok(Some(false)); // Process not running, but continue watching
            } else {
                return Ok(None); // Exit host loop
            }
        }

        Messages::StopExecutableReply => {
            trace!("Stop acknowledged by runner");
            // Process is now stopped, ready for relaunch
        }

        _ => (),
    }

    trace!("Message handled, begin read again");
    msg_stream.begin_read(stream, false)?;

    Ok(Some(true)) // Continue with process running
}

fn send_file<S: Write + Read>(
    msg_stream: &mut MessageStream,
    stream: &mut S,
    filename: &str,
) -> Result<()> {
    let mut buffer = Vec::new();
    let mut f = File::open(filename)?;
    f.read_to_end(&mut buffer)?;

    let file_request = LaunchExecutableRequest {
        // TODO: Implement file serving
        file_server: false,
        path: filename,
        data: &buffer,
    };

    msg_stream.begin_write_message(
        stream,
        &file_request,
        Messages::LaunchExecutableRequest,
        TransitionToRead::Yes,
    )?;

    Ok(())
}

fn close_down_exe<S: Write + Read>(msg_stream: &mut MessageStream, stream: &mut S) -> Result<()> {
    let stop_request = StopExecutableRequest::default();
    msg_stream.begin_write_message(
        stream,
        &stop_request,
        Messages::StopExecutableRequest,
        TransitionToRead::Yes,
    )?;

    // wait 30 ms for the reply, then just the client

    for _ in 0..30 {
        if let Some(msg) = msg_stream.update(stream)? {
            if msg == Messages::StopExecutableReply {
                trace!("StopExecutableReply received, closing down");
                return Ok(());
            }
        }

        std::thread::sleep(std::time::Duration::from_millis(1));
    }

    trace!("No reply from client, closing down anyway");

    Ok(())
}

/// Setup file watcher if --watch flag is enabled
fn setup_file_watcher(opts: &Opt) -> Option<FileWatcher> {
    if !opts.watch {
        return None;
    }

    let filename = match opts.filename.as_ref() {
        Some(f) => f,
        None => {
            warn!("--watch flag provided but no filename specified, watch mode disabled");
            return None;
        }
    };

    let path = Path::new(filename);
    match FileWatcher::new(path) {
        Ok(watcher) => {
            info!("Watch mode enabled - will automatically restart on file changes");
            Some(watcher)
        }
        Err(e) => {
            warn!("Failed to create file watcher: {}", e);
            warn!("Continuing without watch mode");
            None
        }
    }
}

/// Process incoming messages and update process running state
/// Returns Ok(Some(Some(new_state))) if state changed, Ok(Some(None)) if no change, Ok(None) to exit loop
fn process_messages<S: Write + Read>(
    msg_stream: &mut MessageStream,
    stream: &mut S,
    watch_mode: bool,
) -> Result<Option<Option<bool>>> {
    if let Some(msg) = msg_stream.update(stream).context("Failed to update message stream")? {
        match handle_incoming_msg(msg_stream, stream, msg, watch_mode)
            .context("Failed to handle incoming message")?
        {
            Some(running) => Ok(Some(Some(running))),
            None => {
                info!("Remote execution completed");
                Ok(None)
            }
        }
    } else {
        Ok(Some(None)) // No message, no state change
    }
}

/// Handle file change detection and restart if needed
/// Returns updated process_running state and updated watcher (None if disabled)
fn handle_file_change<S: Write + Read>(
    watcher: &mut FileWatcher,
    msg_stream: &mut MessageStream,
    stream: &mut S,
    process_running: bool,
) -> Result<(bool, bool)> {
    match watcher.check_for_stable_change() {
        Ok(true) => {
            // File has changed and is stable - restart
            match restart_executable(msg_stream, stream, watcher.path(), process_running) {
                Ok(running) => Ok((running, true)),
                Err(e) => {
                    log::error!("Failed to restart executable: {}", e);
                    log::error!("Will continue watching for next change");
                    Ok((false, true))
                }
            }
        }
        Ok(false) => {
            // No change or not stable yet
            Ok((process_running, true))
        }
        Err(e) => {
            log::error!("File watcher error: {}", e);
            log::error!("Disabling watch mode");
            Ok((process_running, false))
        }
    }
}

/// Stop the current executable and restart with a new version
/// Returns true if process is now running, false otherwise
fn restart_executable<S: Write + Read>(
    msg_stream: &mut MessageStream,
    stream: &mut S,
    filename: &Path,
    process_running: bool,
) -> Result<bool> {
    info!("Restarting executable with new version: {}", filename.display());

    // If process is running, stop it first
    if process_running {
        debug!("Stopping currently running process");
        let stop_request = StopExecutableRequest::default();
        msg_stream.begin_write_message(
            stream,
            &stop_request,
            Messages::StopExecutableRequest,
            TransitionToRead::Yes,
        )?;

        // Wait up to 5 seconds for stop confirmation
        let timeout_ms = 5000;
        let mut waited_ms = 0;
        let mut got_reply = false;

        while waited_ms < timeout_ms {
            if let Some(msg) = msg_stream.update(stream)? {
                if msg == Messages::StopExecutableReply {
                    debug!("Stop confirmed by runner");
                    got_reply = true;
                    break;
                }
            }
            std::thread::sleep(Duration::from_millis(10));
            waited_ms += 10;
        }

        if !got_reply {
            warn!("Timeout waiting for stop confirmation, proceeding anyway");
        }
    }

    // Send new executable
    debug!("Sending new executable file");
    send_file(msg_stream, stream, filename.to_str().unwrap())?;

    // File sent successfully, process should be starting
    Ok(true)
}

pub fn host_loop(opts: &Opt, ip_address: &str) -> Result<()> {
    let ip_adress: std::net::IpAddr = ip_address.parse()?;
    let address = SocketAddr::new(ip_adress, opts.port);

    info!("Connecting to {} with timeout of {}s", address, opts.connect_timeout_secs);

    let mut stream = TcpStream::connect_timeout(
        &address,
        Duration::from_secs(opts.connect_timeout_secs)
    ).context("Failed to connect to remote runner")?;

    info!("Connected to {}", address);

    // Configure timeouts before handshake
    crate::configure_stream_timeouts(
        &mut stream,
        Duration::from_secs(opts.read_timeout_secs),
        Duration::from_secs(opts.write_timeout_secs),
        Duration::from_secs(opts.keepalive_secs),
    ).context("Failed to configure stream timeouts")?;

    handshake(&mut stream)?;

    // set non-blocking mode after handshake
    stream.set_nonblocking(true)?;

    let mut msg_stream = MessageStream::new();

    // Track whether remote process is currently running
    let mut process_running = false;

    // read file to be sent
    if let Some(target) = opts.filename.as_ref() {
        send_file(&mut msg_stream, &mut stream, target)?;
        process_running = true;
    }

    // Setup file watcher if --watch flag is enabled
    let mut file_watcher = setup_file_watcher(opts);

    // setup ctrl-c handler
    let (tx, rx) = channel();

    ctrlc::set_handler(move || {
        if let Err(e) = tx.send(()) {
            log::error!("Failed to send Ctrl-C signal: {}", e);
        }
    })
    .context("Failed to set Ctrl-C handler")?;

    // Main loop
    loop {
        // Handle incoming messages from remote runner
        match process_messages(&mut msg_stream, &mut stream, opts.watch)? {
            Some(Some(running)) => process_running = running,
            Some(None) => {} // No message, no state change
            None => return Ok(()),
        }

        // Check for Ctrl-C
        if rx.try_recv().is_ok() {
            trace!("Ctrl-C received, closing down");
            return close_down_exe(&mut msg_stream, &mut stream);
        }

        // Check for file changes (if watching)
        if let Some(ref mut watcher) = file_watcher {
            let (new_running, keep_watching) =
                handle_file_change(watcher, &mut msg_stream, &mut stream, process_running)?;
            process_running = new_running;
            if !keep_watching {
                file_watcher = None;
            }
        }

        // don't hammer the CPU
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
}
