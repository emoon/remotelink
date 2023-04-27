pub use clap::Parser;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
pub struct Opt {
    #[arg(short, long)]
    pub debug: bool,
    #[arg(short, long)]
    pub remote_runner: bool,
    #[arg(short, long, default_value = "8888")]
    pub port: u16,
    #[arg(short, long)]
    pub target: Option<String>,
    #[arg(short, long)]
    pub filename: Option<String>,
}
