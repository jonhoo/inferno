use env_logger::Env;
use std::io;
use std::path::PathBuf;
use structopt::StructOpt;

use inferno::diff_folded::{self, Options};

#[derive(Debug, StructOpt)]
#[structopt(name = "inferno-diff-folded", author = "")]
struct Opt {
    /// Normalize sample counts
    #[structopt(short = "n", long = "normalize")]
    normalize: bool,

    /// Strip hex numbers (addresses)
    #[structopt(short = "s", long = "--strip-hex")]
    strip_hex: bool,

    /// Silence all log output
    #[structopt(short = "q", long = "quiet")]
    quiet: bool,

    /// Verbose logging mode (-v, -vv, -vvv)
    #[structopt(short = "v", long = "verbose", parse(from_occurrences))]
    verbose: usize,

    /// Folded stack profile 1
    infile1: PathBuf,

    /// Folded stack profile 2
    infile2: PathBuf,
}

impl Opt {
    fn into_parts(self) -> (PathBuf, PathBuf, Options) {
        (
            self.infile1,
            self.infile2,
            Options {
                normalize: self.normalize,
                strip_hex: self.strip_hex,
            },
        )
    }
}

fn main() -> io::Result<()> {
    let opt = Opt::from_args();

    // Initialize logger
    if !opt.quiet {
        env_logger::Builder::from_env(Env::default().default_filter_or(match opt.verbose {
            0 => "warn",
            1 => "info",
            2 => "debug",
            _ => "trace",
        }))
        .default_format_timestamp(false)
        .init();
    }

    let (folded1, folded2, options) = opt.into_parts();
    diff_folded::from_files(&options, folded1, folded2, io::stdout().lock())
}
