pub use structopt::StructOpt;

#[derive(StructOpt, Debug)]
#[structopt(name = "Remotelink")]
pub struct Opt {
    #[structopt(short, long)]
    pub debug: bool,
    #[structopt(short, long)]
    pub host: bool,
    #[structopt(short, long, default_value = "8888")]
    pub port: u16,
    #[structopt(short, long)]
    pub target: Option<String>,
    #[structopt(short, long)]
    pub filename: Option<String>,
}
