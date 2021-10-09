#[macro_use]
extern crate serde_derive;

mod host;
mod log;
mod message_stream;
mod messages;
mod options;
mod target;
mod tests;

use crate::options::Opt;
use anyhow::Result;
use structopt::StructOpt;

fn main() -> Result<()> {
    let opt = Opt::from_args();

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
