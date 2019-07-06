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
    // Flags...
    /// Include raw addresses where symbols can't be found
    #[structopt(long = "addrs")]
    _addrs: bool,

    /// All annotations (--kernel --jit)
    #[structopt(long = "all")]
    _all: bool,

    /// Annotate jit functions with a _[j]
    #[structopt(long = "jit")]
    _jit: bool,

    /// Annotate kernel functions with a _[k]
    #[structopt(long = "kernel")]
    _kernel: bool,

    /// Include PID with process names
    #[structopt(long = "pid")]
    _pid: bool,

    /// Include TID and PID with process names
    #[structopt(long = "tid")]
    _tid: bool,

    /// Silence all log output
    #[structopt(short = "q", long = "quiet")]
    quiet: bool,

    /// Verbose logging mode (-v, -vv, -vvv)
    #[structopt(short = "v", long = "verbose", parse(from_occurrences))]
    verbose: usize,

    // Options...
    /// Event filter [default: first encountered event]
    #[structopt(long = "event-filter", value_name = "STRING")]
    event_filter: Option<String>,

    /// Number of stacks per job sent to threadpool (only used if nthreads > 1)
    #[structopt(long = "nstacks", default_value = "20", value_name = "UINT")]
    nstacks: usize,

    /// Number of threads to use [default: number of logical cores on your machine]
    #[structopt(short = "n", long = "nthreads", value_name = "UINT")]
    nthreads: Option<usize>,

    // Args...
    /// Perf script output file, or STDIN if not specified
    #[structopt(value_name = "PATH")]
    infile: Option<PathBuf>,
}

impl Opt {
    fn into_parts(self) -> (Option<PathBuf>, Options) {
        (
            self.infile,
            Options {
                include_pid: self._pid,
                include_tid: self._tid,
                include_addrs: self._addrs,
                annotate_jit: self._jit || self._all,
                annotate_kernel: self._kernel || self._all,
                event_filter: self.event_filter,
                nthreads: self.nthreads.unwrap_or_else(num_cpus::get),
                nstacks_per_job: self.nstacks,
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
