use std::io;
use std::path::PathBuf;

use env_logger::Env;
use inferno::collapse::sample::{Folder, Options};
use inferno::collapse::Collapse;
use structopt::StructOpt;

#[derive(Debug, StructOpt)]
#[structopt(
    name = "inferno-collapse-sample",
    about,
    after_help = "\
[1] This processes the result of the sample command on macOS:
            sample 1234 -file out.sample_stacks"
)]
struct Opt {
    // ************* //
    // *** FLAGS *** //
    // ************* //
    /// Don't include modules with function names
    #[structopt(long = "no-modules")]
    no_modules: bool,

    /// Silence all log output
    #[structopt(short = "q", long = "quiet")]
    quiet: bool,

    /// Verbose logging mode (-v, -vv, -vvv)
    #[structopt(short = "v", long = "verbose", parse(from_occurrences))]
    verbose: usize,

    // ************ //
    // *** ARGS *** //
    // ************ //
    /// sample output file, or STDIN if not specified
    #[structopt(value_name = "PATH")]
    infile: Option<PathBuf>,
}

impl Opt {
    fn into_parts(self) -> (Option<PathBuf>, Options) {
        (
            self.infile,
            Options {
                no_modules: self.no_modules,
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
