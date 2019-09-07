use std::io;
use std::path::PathBuf;

use env_logger::Env;
use inferno::collapse::guess::{Folder, Options};
use inferno::collapse::{Collapse, DEFAULT_NTHREADS};
use lazy_static::lazy_static;
use structopt::StructOpt;

lazy_static! {
    static ref NTHREADS: String = format!("{}", *DEFAULT_NTHREADS);
}

#[derive(Debug, StructOpt)]
#[structopt(
    name = "inferno-collapse-guess",
    about,
    after_help = "\
[1] Attempts to find an appropriate collapser to use based on the input.
                  "
)]
struct Opt {
    // ************* //
    // *** FLAGS *** //
    // ************* //
    /// Silence all log output
    #[structopt(short = "q", long = "quiet")]
    quiet: bool,

    /// Verbose logging mode (-v, -vv, -vvv)
    #[structopt(short = "v", long = "verbose", parse(from_occurrences))]
    verbose: usize,

    // *************** //
    // *** OPTIONS *** //
    // *************** //
    /// Number of threads to use
    #[structopt(
        short = "n",
        long = "nthreads",
        default_value = &NTHREADS,
        value_name = "UINT"
    )]
    nthreads: usize,

    // ************ //
    // *** ARGS *** //
    // ************ //
    /// Input file, or STDIN if not specified
    #[structopt(value_name = "PATH")]
    infile: Option<PathBuf>,
}

impl Opt {
    fn into_parts(self) -> (Option<PathBuf>, Options) {
        (
            self.infile,
            Options {
                nthreads: self.nthreads,
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

    let (infile, options) = opt.into_parts();
    Folder::from(options).collapse_file(infile.as_ref(), io::stdout().lock())
}
