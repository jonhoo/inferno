mod bpftrace;
mod perf;

pub use bpftrace::Bpftrace;
pub use perf::{Perf, PerfOptions};

use std::fs::File;
use std::io;
use std::path::Path;

const READER_CAPACITY: usize = 128 * 1024;

pub trait Frontend {
    fn collapse<R, W>(&mut self, reader: R, writer: W) -> io::Result<()>
    where
        R: io::BufRead,
        W: io::Write;

    fn collapse_with<P>(&mut self, infile: Option<P>) -> io::Result<()>
    where
        P: AsRef<Path>,
    {
        let stdout = io::stdout();
        let writer = stdout.lock();
        match infile {
            Some(ref path) => {
                let file = File::open(path)?;
                let reader = io::BufReader::with_capacity(READER_CAPACITY, file);
                self.collapse(reader, writer)
            }
            None => {
                let stdio = io::stdin();
                let stdio_guard = stdio.lock();
                let reader = io::BufReader::with_capacity(READER_CAPACITY, stdio_guard);
                self.collapse(reader, writer)
            }
        }
    }
}
