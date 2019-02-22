use std::io;
use std::path::PathBuf;
use structopt::StructOpt;

use inferno::collapse::{Collapser, Frontend};

#[derive(Debug, StructOpt)]
#[structopt(name = "inferno-collapse-bpftrace", author = "")]
struct Opt {
    /// bpftrace script output file, or STDIN if not specified
    infile: Option<PathBuf>,
}

fn main() -> io::Result<()> {
    let opt = Opt::from_args();
    Collapser::new(Frontend::Bpftrace).handle_file(opt.infile.as_ref())
}
