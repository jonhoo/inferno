use std::io;
use std::path::PathBuf;

use env_logger::Env;
use inferno::collapse::perf::{Folder, Options};
use inferno::collapse::Collapse;
use structopt::StructOpt;

#[derive(Debug, StructOpt)]
#[structopt(
    name = "inferno-collapse-perf",
    author = "",
    after_help = "\
[1] perf script must emit both PID and TIDs for these to work; eg, Linux < 4.1:
        perf script -f comm,pid,tid,cpu,time,event,ip,sym,dso,trace
    for Linux >= 4.1:
        perf script -F comm,pid,tid,cpu,time,event,ip,sym,dso,trace
    If you save this output add --header on Linux >= 3.14 to include perf info."
)]
struct Opt {
    /// All annotations (--kernel --jit)
    #[structopt(long = "all")]
    annotate_all: bool,

    /// Annotate jit functions with a _[j]
    #[structopt(long = "jit")]
    annotate_jit: bool,

    /// Annotate kernel functions with a _[k]
    #[structopt(long = "kernel")]
    annotate_kernel: bool,

    /// Event name filter; defaults to first encountered event
    #[structopt(long = "event-filter", value_name = "EVENT")]
    event_filter: Option<String>,

    /// Include raw addresses where symbols can't be found
    #[structopt(long = "addrs")]
    include_addrs: bool,

    /// Include PID with process names [1]
    #[structopt(long = "pid")]
    include_pid: bool,

    /// Include TID and PID with process names [1]
    #[structopt(long = "tid")]
    include_tid: bool,

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

    /// Perf script output file, or STDIN if not specified
    #[structopt(value_name = "INFILE")]
    infile: Option<PathBuf>,
}

impl Opt {
    fn into_parts(self) -> (Option<PathBuf>, Options) {
        (
            self.infile,
            Options {
                include_pid: self.include_pid,
                include_tid: self.include_tid,
                include_addrs: self.include_addrs,
                annotate_jit: self.annotate_jit || self.annotate_all,
                annotate_kernel: self.annotate_kernel || self.annotate_all,
                event_filter: self.event_filter,
                nthreads: self.nthreads.unwrap_or_else(|| num_cpus::get()),
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
