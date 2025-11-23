use anyhow::{anyhow, Context, Result};
use log::{info, trace};
use std::fs::File;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::sync::mpsc::channel;
use std::time::Duration;

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
/// Returns Ok(true) to continue, Ok(false) to exit
fn handle_incoming_msg<S: Write + Read>(
    msg_stream: &mut MessageStream,
    stream: &mut S,
    message: Messages,
) -> Result<bool> {
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
                return Ok(false);
            }
            trace!("Process finished with status: {}", reply.launch_status);
            return Ok(false);
        }

        _ => (),
    }

    trace!("Message handled, begin read again");
    msg_stream.begin_read(stream, false)?;

    Ok(true)
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

    // read file to be sent

    if let Some(target) = opts.filename.as_ref() {
        send_file(&mut msg_stream, &mut stream, target)?;
    }

    // setup ctrl-c handler
    let (tx, rx) = channel();

    ctrlc::set_handler(move || {
        if let Err(e) = tx.send(()) {
            log::error!("Failed to send Ctrl-C signal: {}", e);
        }
    })
    .context("Failed to set Ctrl-C handler")?;

    //
    loop {
        if let Some(msg) = msg_stream.update(&mut stream)
            .context("Failed to update message stream")? {
            if !handle_incoming_msg(&mut msg_stream, &mut stream, msg)
                .context("Failed to handle incoming message")? {
                info!("Remote execution completed");
                return Ok(());
            }
        }

        if rx.try_recv().is_ok() {
            trace!("Ctrl-C received, closing down");
            return close_down_exe(&mut msg_stream, &mut stream);
        }

        // don't hammer the CPU
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
}
