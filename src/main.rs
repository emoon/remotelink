#[macro_use]
extern crate serde_derive;

mod host;
mod messages;
mod options;
mod target;
mod tests;

use crate::options::Opt;
use structopt::StructOpt;

fn main() {
    let opt = Opt::from_args();

    /*
    if opt.server {
        server_loop(&opt);
    } else if opt.target.is_some() {
        client_loop(&opt, opt.target.as_ref().unwrap());
    } else {
        println!("Must pass --server or --client");
    }
    */
}
