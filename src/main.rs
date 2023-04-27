mod host;
mod log;
mod message_stream;
mod messages;
mod options;
mod remote_runner;
mod tests;
use clap::Parser;

use crate::options::Opt;
use anyhow::Result;

fn main() -> Result<()> {
    let opt = Opt::parse();

    log::set_log_level(log::LOG_ERROR);

    if opt.remote_runner {
        println!("Starting remote-runner");
        remote_runner::update(&opt);
    } else {
        println!("Starting host");
        host::host_loop(&opt, opt.target.as_ref().unwrap())?;
    }

    Ok(())
}
