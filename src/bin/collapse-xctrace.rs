use std::io;
use std::path::PathBuf;

use clap::{ArgAction, Parser};
use env_logger::Env;
use inferno::collapse::xctrace::Folder;
use inferno::collapse::Collapse;

#[derive(Debug, Parser)]
#[clap(
    name = "inferno-collapse-xctrace",
    about,
    after_help = r#"\
[1] This processes the result of the xctrace with `Timer Profiler` profile as run with:
        `xctrace record --template 'Time Profiler' --launch <executable> --output tmp.trace`
    or
        `xctrace record --template 'Time Profiler' --attach <pid|proc_name> --output tmp.trace`
    then
        xctrace export --input tmp.trace --xpath '/trace-toc/*/data/table[@schema="time-profile"]' > tmp.xml
    "#
)]
struct Opt {
    /// Silence all log output
    #[clap(short = 'q', long = "quiet")]
    quiet: bool,

    /// Verbose logging mode (-v, -vv, -vvv)
    #[clap(short = 'v', long = "verbose", action = ArgAction::Count)]
    verbose: u8,

    // ************ //
    // *** ARGS *** //
    // ************ //
    /// xctrace output file, or STDIN if not specified
    #[clap(value_name = "PATH")]
    infile: Option<PathBuf>,
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

    Folder::default().collapse_file_to_stdout(opt.infile.as_ref())
}
