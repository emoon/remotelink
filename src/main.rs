mod file_server;
mod file_watcher;
mod host;
mod message_stream;
mod messages;
mod options;
mod remote_runner;
mod tests;
use clap::Parser;

use crate::options::Opt;
use anyhow::{Context, Result};
use log::LevelFilter;
use simple_logger::SimpleLogger;
use std::net::TcpStream;
use std::time::Duration;

/// Configure timeouts and TCP options on a stream
pub fn configure_stream_timeouts(
    stream: &mut TcpStream,
    read_timeout: Duration,
    write_timeout: Duration,
    _keepalive: Duration,
) -> Result<()> {
    stream.set_read_timeout(Some(read_timeout))
        .context("Failed to set read timeout")?;

    stream.set_write_timeout(Some(write_timeout))
        .context("Failed to set write timeout")?;

    // Note: TCP keepalive is a socket option that requires platform-specific APIs.
    // On Unix systems, it's set via setsockopt with SO_KEEPALIVE.
    // For cross-platform support, we use the socket2 crate approach.
    #[cfg(unix)]
    {
        use std::os::fd::AsRawFd;
        let fd = stream.as_raw_fd();
        // Enable TCP keepalive
        unsafe {
            let optval: libc::c_int = 1;
            if libc::setsockopt(
                fd,
                libc::SOL_SOCKET,
                libc::SO_KEEPALIVE,
                &optval as *const _ as *const libc::c_void,
                std::mem::size_of_val(&optval) as libc::socklen_t,
            ) < 0 {
                return Err(std::io::Error::last_os_error())
                    .context("Failed to set SO_KEEPALIVE");
            }
        }
    }

    // Disable Nagle's algorithm for lower latency
    stream.set_nodelay(true)
        .context("Failed to set TCP_NODELAY")?;

    Ok(())
}

fn main() -> Result<()> {
    let opt = Opt::parse();

    let level = match opt.log_level.as_str() {
        "error" => LevelFilter::Error,
        "warn" => LevelFilter::Warn,
        "info" => LevelFilter::Info,
        "debug" => LevelFilter::Debug,
        "trace" => LevelFilter::Trace,
        _ => LevelFilter::Warn,
    };

    SimpleLogger::new()
        .with_level(level)
        .with_colors(true)
        .init()?;

    if opt.remote_runner {
        println!("Starting remote-runner");
        remote_runner::update(&opt)?;
    } else {
        println!("Starting host");
        let target = opt.target.as_ref()
            .ok_or_else(|| anyhow::anyhow!("Target IP address is required when not in remote-runner mode"))?;
        host::host_loop(&opt, target)?;
    }

    Ok(())
}
