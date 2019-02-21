use std::fs::File;
use std::io;
use std::path::PathBuf;
use structopt::StructOpt;

use inferno::collapse::bpftrace;

const READER_CAPACITY: usize = 128 * 1024;

#[derive(Debug, StructOpt)]
#[structopt(name = "inferno-collapse-bpftrace", author = "")]
struct Opt {
    /// bpftrace script output file, or STDIN if not specified
    infile: Option<PathBuf>,
}

fn main() -> io::Result<()> {
    let opt = Opt::from_args();

    let stdout = io::stdout();
    let writer = stdout.lock();

    match opt.infile {
        Some(ref path) => {
            let file = File::open(path)?;
            let reader = io::BufReader::with_capacity(READER_CAPACITY, file);
            bpftrace::handle_file(reader, writer)
        }
        None => {
            let stdio = io::stdin();
            let stdio_guard = stdio.lock();
            let reader = io::BufReader::with_capacity(READER_CAPACITY, stdio_guard);
            bpftrace::handle_file(reader, writer)
        }
    }
}
