use env_logger::Env;
use std::io;
use std::path::PathBuf;
use structopt::StructOpt;

use inferno::collapse::dtrace::{Folder, Options};
use inferno::collapse::Collapse;

#[derive(Debug, StructOpt)]
#[structopt(
    name = "inferno-collapse-dtrace",
    author = "",
    after_help = "\
[1] This processes the result of the dtrace ustack() as run with:
        dtrace -x ustackframes=100 -n 'profile-97 /pid == 12345 && arg1/ { @[ustack()] = count(); } tick-60s { exit(0); }'
    or including kernel time:
        dtrace -x ustackframes=100 -n 'profile-97 /pid == 12345/ { @[ustack()] = count(); } tick-60s { exit(0); }'
    "
)]
struct Opt {
    /// Silence all log output
    #[structopt(short = "q", long = "quiet")]
    quiet: bool,

    /// Verbose logging mode (-v, -vv, -vvv)
    #[structopt(short = "v", long = "verbose", parse(from_occurrences))]
    verbose: usize,

    /// Include offsets
    #[structopt(long = "includeoffset")]
    includeoffset: bool,

    /// Demangle function names
    #[structopt(long = "demangle")]
    demangle: bool,

    /// perf script output file, or STDIN if not specified
    infile: Option<PathBuf>,
}

impl Opt {
    fn into_parts(self) -> (Option<PathBuf>, Options) {
        (
            self.infile,
            Options {
                includeoffset: self.includeoffset,
                demangle: self.demangle,
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
