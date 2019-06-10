use std::io;
use std::path::PathBuf;

use env_logger::Env;
use inferno::collapse::guess::Folder;
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
    /// Number of threads to use; defaults to number of logical
    /// cores on your machine
    #[structopt(short = "n", long = "nthreads", value_name = "NTHREADS")]
    nthreads: Option<usize>,

    /// Silence all log output
    #[structopt(short = "q", long = "quiet")]
    quiet: bool,

    /// Verbose logging mode (-v, -vv, -vvv)
    #[structopt(short = "v", long = "verbose", parse(from_occurrences))]
    verbose: usize,

    /// Input file, or STDIN if not specified
    #[structopt(value_name = "INFILE")]
    infile: Option<PathBuf>,
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

    let mut guess = Folder::new(opt.nthreads.unwrap_or_else(num_cpus::get));
    guess.collapse_file(opt.infile.as_ref(), io::stdout().lock())
}
