mod host;
mod message_stream;
mod messages;
mod options;
mod remote_runner;
mod tests;
use clap::Parser;

use crate::options::Opt;
use anyhow::Result;
use log::LevelFilter;
use simple_logger::SimpleLogger;

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
