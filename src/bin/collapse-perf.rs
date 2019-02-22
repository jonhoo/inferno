use std::io;
use std::path::PathBuf;
use structopt::StructOpt;

use inferno::collapse::{Collapser, Frontend};
use inferno::collapse::perf::Options;

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
    /// include PID with process names [1]
    #[structopt(long = "pid")]
    include_pid: bool,

    /// include TID and PID with process names [1]
    #[structopt(long = "tid")]
    include_tid: bool,

    /// include raw addresses where symbols can't be found
    #[structopt(long = "addrs")]
    include_addrs: bool,

    /// annotate jit functions with a _[j]
    #[structopt(long = "jit")]
    annotate_jit: bool,

    /// annotate kernel functions with a _[k]
    #[structopt(long = "kernel")]
    annotate_kernel: bool,

    /// all annotations (--kernel --jit)
    #[structopt(long = "all")]
    annotate_all: bool,

    /// un-inline using addr2line
    #[structopt(name = "inline", long = "inline")]
    show_inline: bool,

    /// adds source context to --inline
    #[structopt(long = "context", requires = "inline")]
    show_context: bool,

    /// event name filter, defaults to first encountered event
    #[structopt(long = "event-filter", value_name = "EVENT")]
    event_filter: Option<String>,

    /// perf script output file, or STDIN if not specified
    infile: Option<PathBuf>,
}

impl Into<Options> for Opt {
    fn into(self) -> Options {
        Options {
            include_pid: self.include_pid,
            include_tid: self.include_tid,
            include_addrs: self.include_addrs,
            annotate_jit: self.annotate_jit || self.annotate_all,
            annotate_kernel: self.annotate_kernel || self.annotate_all,
            show_inline: self.show_inline,
            show_context: self.show_context,
            event_filter: self.event_filter,
        }
    }
}

fn main() -> io::Result<()> {
    let opt = Opt::from_args();
    let infile = opt.infile.clone();
    let options = opt.into();
    Collapser::new(Frontend::Perf(options)).handle_file(infile.as_ref())
}
