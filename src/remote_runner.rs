use crate::message_stream::{MessageStream, TransitionToRead};
use crate::messages;
use crate::messages::*;
use crate::options::*;
use anyhow::*;
use core::result::Result::Ok;
use std::fs::File;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::os::unix::fs::PermissionsExt;
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;

type IoOut = Arc<Mutex<Vec<u8>>>;

#[derive(Default)]
struct Context {
    /// Used for tracking running executable.
    stdout: Option<IoOut>,
    /// Used for tracking running executable.
    stderr: Option<IoOut>,
    /// Used for tracking running executable.
    proc: Option<Child>,
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
                let file: bincode::Result<messages::LaunchExecutableRequest> =
                    bincode::deserialize(&msg_stream.data);

                match file {
                    Ok(f) => {
                        self.start_executable(&f);

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
                        panic!("{}", e);
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
    fn child_stream_to_vec<R>(mut stream: R) -> IoOut
    where
        R: Read + Send + 'static,
    {
        let out = Arc::new(Mutex::new(Vec::new()));
        let vec = out.clone();
        thread::Builder::new()
            .name("child_stream_to_vec".into())
            .spawn(move || loop {
                let mut buf = [0];
                match stream.read(&mut buf) {
                    Err(err) => {
                        println!("{}] Error reading from stream: {}", line!(), err);
                        break;
                    }
                    Ok(got) => {
                        if got == 0 {
                            break;
                        } else if got == 1 {
                            vec.lock().expect("!lock").push(buf[0])
                        } else {
                            println!("{}] Unexpected number of bytes: {}", line!(), got);
                            break;
                        }
                    }
                }
            })
            .expect("!thread");
        out
    }

    fn start_executable(&mut self, f: &messages::LaunchExecutableRequest) {
        println!("Want to launch {} size {}", f.path, f.data.len());

        {
            let mut file = File::create("test").unwrap();
            file.write_all(f.data).unwrap();
        }

        // make exe executable
        std::fs::set_permissions("test", std::fs::Permissions::from_mode(0o700)).unwrap();

        let mut p = Command::new("./test")
            .stderr(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .expect("failed to execute child");

        self.stdout = Some(Self::child_stream_to_vec(p.stdout.take().expect("!stdout")));
        self.stderr = Some(Self::child_stream_to_vec(p.stderr.take().expect("!stderr")));
        self.proc = Some(p);
    }
}

fn handle_client(stream: &mut TcpStream) -> Result<()> {
    println!("Incoming connection from: {}", stream.peer_addr()?);

    stream.set_nonblocking(true)?;

    let mut msg_stream = MessageStream::new();

    msg_stream.begin_read(stream, false)?;

    // Setup a context so we can keep track of a running process and such
    let mut context = Context::default();

    loop {
        let msg = msg_stream.update(stream)?;

        match msg {
            Some(msg) => {
                if !context.handle_incoming_msg(&mut msg_stream, stream, msg)? {
                    println!("exit client");
                    return Ok(());
                }
            }
            _ => (),
        }

        if let Some(stdout) = context.stdout.as_mut() {
            let mut data = stdout.lock().unwrap();

            if !data.is_empty() {
                let text_message = TextMessage { data: &data };

                msg_stream.begin_write_message(
                    stream,
                    &text_message,
                    Messages::StdoutOutput,
                    TransitionToRead::Yes,
                )?;

                data.clear();
            }
        }

        std::thread::sleep(std::time::Duration::from_millis(1));
    }
}

pub fn update(_opts: &Opt) {
    let listener = TcpListener::bind("0.0.0.0:8888").expect("Could not bind");
    println!("Wating incoming host");
    for stream in listener.incoming() {
        match stream {
            Err(e) => eprintln!("failed: {}", e),
            Ok(mut stream) => {
                thread::spawn(move || {
                    handle_client(&mut stream).unwrap_or_else(|error| eprintln!("{:?}", error));
                });
            }
        }
    }
}
