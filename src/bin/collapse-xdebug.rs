use std::io;
use std::path::PathBuf;

use clap::Parser;
use env_logger::Env;
use inferno::collapse::xdebug::{Folder, Options};
use inferno::collapse::{Collapse, DEFAULT_NTHREADS};
use once_cell::sync::Lazy;

static NTHREADS: Lazy<String> = Lazy::new(|| format!("{}", *DEFAULT_NTHREADS));

#[derive(Debug, Parser)]
#[clap(name = "inferno-collapse-xdebug")]
struct Opt {
    /// Silence all log output
    #[clap(short = 'q', long = "quiet")]
    quiet: bool,

    /// Verbose logging mode (-v, -vv, -vvv)
    #[clap(short = 'v', long = "verbose", parse(from_occurrences))]
    verbose: usize,

    // *************** //
    // *** OPTIONS *** //
    // *************** //
    /// Number of threads to use.
    #[clap(
        short = 'n',
        long = "nthreads",
        default_value = &NTHREADS,
        value_name = "UINT"
    )]
    nthreads: usize,

    // ************ //
    // *** ARGS *** //
    // ************ //
    #[clap(value_name = "PATH")]
    /// Xdebug script output file, or STDIN if not specified
    infile: Option<PathBuf>,

    #[clap(short = 'f')]
    include_filenames: bool,
}

impl Opt {
    fn into_parts(self) -> (Option<PathBuf>, Options) {
        (
            self.infile,
            Options {
                nthreads: self.nthreads,
                include_filenames: self.include_filenames,
            },
        )
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
    Folder::from(options).collapse_file(infile.as_ref(), io::stdout().lock())
}
