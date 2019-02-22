use std::io;
use std::path::PathBuf;
use structopt::StructOpt;

use inferno::collapse::bpftrace::Bpftrace;
use inferno::collapse::Frontend;

#[derive(Debug, StructOpt)]
#[structopt(name = "inferno-collapse-bpftrace", author = "")]
struct Opt {
    /// bpftrace script output file, or STDIN if not specified
    infile: Option<PathBuf>,
}

fn main() -> io::Result<()> {
    let opt = Opt::from_args();
    Bpftrace::default().collapse_file(opt.infile.as_ref())
}
