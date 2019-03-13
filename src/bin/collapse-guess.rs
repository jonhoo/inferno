use env_logger::Env;
use std::io;
use std::path::PathBuf;
use structopt::StructOpt;

use inferno::collapse::guess::Folder;
use inferno::collapse::Collapse;

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

    /// Input file, or STDIN if not specified
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

    let mut guess = Folder {};
    guess.collapse_file(opt.infile.as_ref(), io::stdout().lock())
}
