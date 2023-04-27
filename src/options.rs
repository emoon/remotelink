pub use clap::Parser;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
pub struct Opt {
    #[arg(short, long, default_value = "warn")]
    /// Sets the level of verbosity.
    /// Supported levels: error, warn, info, debug, trace
    pub log_level: String,
    #[arg(short, long)]
    /// Run as remote runner. This is the machine where the executable(s) will be run.
    pub remote_runner: bool,
    #[arg(short, long, default_value = "8888")]
    /// Select a TCP port to talk over. Has to be same on both sides.
    pub port: u16,
    #[arg(short, long)]
    /// The remote runner to connect to.
    pub target: Option<String>,
    #[arg(short, long)]
    /// The executable to run.
    pub filename: Option<String>,
}
