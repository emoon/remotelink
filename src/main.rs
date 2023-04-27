mod host;
mod log;
mod message_stream;
mod messages;
mod options;
mod target;
mod tests;
use clap::Parser;

use crate::options::Opt;
use anyhow::Result;

fn main() -> Result<()> {
    let opt = Opt::parse();

    log::set_log_level(log::LOG_ERROR);

    if opt.host {
        println!("Starting target");
        target::target_loop(&opt);
    } else {
        println!("Starting host");
        host::host_loop(&opt, opt.target.as_ref().unwrap())?;
    }

    Ok(())
}
