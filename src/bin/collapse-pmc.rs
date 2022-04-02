use std::io;
use std::path::PathBuf;

use env_logger::Env;
use inferno::collapse::pmc::{Folder, Options};
use inferno::collapse::{Collapse, DEFAULT_NTHREADS};
use lazy_static::lazy_static;
use structopt::StructOpt;

lazy_static! {
    static ref NTHREADS: String = format!("{}", *DEFAULT_NTHREADS);
}

#[derive(Debug, StructOpt)]
#[structopt(
    name = "inferno-collapse-pmc",
    about,
    after_help = "\
[1] pmcstat must be used in callchain mode (-G).
    For example:
      To capture, use:
        pmcstat -S unhalted-cycles -O pmc.out
      To convert to callchain, you can use:
        pmcstat -R pmc.out -z16 -G pmc.graph
      Then collapse all stacks to flamegraph format
        inferno-collapse-pmc pmc.graph
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
    #[structopt(value_name = "PATH")]
    /// Pmcstat -G output file, or STDIN if not specified
    infile: Option<PathBuf>,
}

impl Opt {
    fn into_parts(self) -> (Option<PathBuf>, Options) {
        let mut options = Options::default();
        options.nthreads = self.nthreads;
        (self.infile, options)
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
        .format_timestamp(None)
        .init();
    }

    let (infile, options) = opt.into_parts();
    Folder::from(options).collapse_file_to_stdout(infile.as_ref())
}
