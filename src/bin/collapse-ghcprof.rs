use std::io;
use std::path::PathBuf;

use clap::{ArgAction, ArgGroup, Parser};
use env_logger::Env;
use inferno::collapse::ghcprof::{Folder, Options, Source};
use inferno::collapse::Collapse;

#[derive(Debug, Parser)]
#[clap(
    name = "inferno-collapse-ghcprof",
    about,
    after_help = "\
[1] This processes the .prof output of GHC (Glorious Haskell Compiler)
    "
)]
#[command(group(
    ArgGroup::new("source")
        .required(false)
        .args(["bytes", "ticks"]),
))]
struct Opt {
    // ************* //
    // *** FLAGS *** //
    // ************* //
    /// Count bytes
    #[clap(long = "bytes")]
    bytes: bool,
    /// Count ticks
    #[clap(long = "ticks")]
    ticks: bool,

    /// Silence all log output
    #[clap(short = 'q', long = "quiet")]
    quiet: bool,

    /// Verbose logging mode (-v, -vv, -vvv)
    #[clap(short = 'v', long = "verbose", action = ArgAction::Count)]
    verbose: u8,

    // ************ //
    // *** ARGS *** //
    // ************ //
    /// ghcprof CSV output file, or STDIN if not specified
    #[clap(value_name = "PATH")]
    infile: Option<PathBuf>,
}

impl Opt {
    fn into_parts(self) -> (Option<PathBuf>, Options) {
        let mut options = Options::default();
        options.source =
            if self.ticks {
                Source::Ticks
        } else if self.bytes {
            Source::Bytes
        } else {
            Source::PercentTime
        };
        (self.infile, options)
    }
}

fn main() -> io::Result<()> {
    let opt = Opt::parse();

    // Initialize logger
    if !opt.quiet {
        env_logger::Builder::from_env(Env::default().default_filter_or(match opt.verbose {
            0 => "warn",
            1 => "info",
            2 => "debug",
            _ => "trace",
        }))
        .format_timestamp(None)
        .init();
    }

    let (infile, options) = opt.into_parts();
    Folder::from(options).collapse_file_to_stdout(infile.as_ref())
}
