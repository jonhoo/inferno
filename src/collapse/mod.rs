pub mod bpftrace;
pub mod perf;

use std::io;
use std::path::Path;
use std::fs::File;

const READER_CAPACITY: usize = 128 * 1024;

/// Type that knows how to collapse formats from various frontends (e.g. bpftrace and perf)
/// into output that can be subsequently ingested by `inferno-flamegraph`.
pub struct Collapser {
    frontend: Frontend,
}

impl Collapser {
    /// Constructs a new `Collapser`.
    pub fn new(frontend: Frontend) -> Self {
        Collapser { frontend }
    }

    /// Takes the provided input (either a filepath or STDIN if the `infile` argument is `None`)
    /// and collapases it, writing the output to STDOUT.
    ///
    /// # Errors
    ///
    /// Returns an `io::Error` if the operation is unsuccessful.
    pub fn handle_file<P>(&mut self, infile: Option<P>) -> io::Result<()> where P: AsRef<Path> {
        let stdout = io::stdout();
        let writer = stdout.lock();
        match infile {
            Some(ref path) => {
                let file = File::open(path)?;
                let reader = io::BufReader::with_capacity(READER_CAPACITY, file);
                match self.frontend {
                    Frontend::Perf(ref mut options) => perf::handle_file(options, reader, writer),
                    Frontend::Bpftrace => bpftrace::handle_file(reader, writer),
                }
            }
            None => {
                let stdio = io::stdin();
                let stdio_guard = stdio.lock();
                let reader = io::BufReader::with_capacity(READER_CAPACITY, stdio_guard);
                match self.frontend {
                    Frontend::Perf(ref mut options) => perf::handle_file(options, reader, writer),
                    Frontend::Bpftrace => bpftrace::handle_file(reader, writer),
                }
            }
        }
    }

    /// The `Frontend` associated with this collapser.
    pub fn frontend(&self) -> &Frontend {
        &self.frontend
    }
}

pub enum Frontend {
    /// The bpftrace frontend
    Bpftrace,
    /// The perf frontend
    Perf(perf::Options),
}
