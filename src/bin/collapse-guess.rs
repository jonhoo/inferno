use std::io;
use std::path::PathBuf;

use env_logger::Env;
use inferno::collapse::guess::{Folder, Options};
use inferno::collapse::Collapse;
use structopt::StructOpt;

#[derive(Debug, StructOpt)]
#[structopt(
    name = "inferno-collapse-guess",
    author = "",
    after_help = "\
[1] Attempts to find an appropriate collapser to use based on the input.
                  "
)]
struct Opt {
    /// Silence all log output
    #[structopt(short = "q", long = "quiet")]
    quiet: bool,

    /// Verbose logging mode (-v, -vv, -vvv)
    #[structopt(short = "v", long = "verbose", parse(from_occurrences))]
    verbose: usize,

    /// Number of stacks per job sent to threadpool (only used if nthreads > 1)
    #[structopt(long = "nstacks", default_value = "10", value_name = "UINT")]
    nstacks: usize,

    /// Number of threads to use [default: number of logical cores on your machine]
    #[structopt(short = "n", long = "nthreads", value_name = "UINT")]
    nthreads: Option<usize>,

    /// Input file, or STDIN if not specified
    #[structopt(value_name = "PATH")]
    infile: Option<PathBuf>,
}

impl Opt {
    fn into_parts(self) -> (Option<PathBuf>, Options) {
        (
            self.infile,
            Options {
                nstacks_per_job: self.nstacks,
                nthreads: self.nthreads.unwrap_or_else(num_cpus::get),
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
