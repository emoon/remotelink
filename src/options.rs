pub use structopt::StructOpt;

#[derive(StructOpt, Debug)]
#[structopt(name = "Remotelink")]
pub struct Opt {
    #[structopt(short, long)]
    debug: bool,
    #[structopt(short, long)]
    host: bool,
    #[structopt(short, long, default_value = "8888")]
    port: u16,
    #[structopt(short, long)]
    target: Option<String>,
    #[structopt(short, long)]
    filename: Option<String>,
}

