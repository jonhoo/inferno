//! Tools for collapsing the output of various profilers (e.g. [perf]) into the
//! format expected by flamegraph.
//!
//! [perf]: https://en.wikipedia.org/wiki/Perf_(Linux)

pub mod perf;

use std::fs::File;
use std::io;
use std::path::Path;

const READER_CAPACITY: usize = 128 * 1024;

/// This trait represents the ability to collapse the output of various profilers, such as
/// [perf], into the format expected by flamegraph.
///
/// [perf]: https://en.wikipedia.org/wiki/Perf_(Linux)
pub trait Frontend {
    /// Collapses the contents of the provided `reader` into the format expected by flamegraph.
    /// Writes the output to the provided `writer`.
    ///
    /// # Errors
    ///
    /// Return an [`io::Error`] if unsuccessful.
    ///
    /// [`io::Error`]: https://doc.rust-lang.org/std/io/struct.Error.html
    fn collapse<R, W>(&mut self, reader: R, writer: W) -> io::Result<()>
    where
        R: io::BufRead,
        W: io::Write;

    /// Collapses the contents of a file (or of STDIN if `infile` is `None`) into the
    /// format expected by flamegraph. Writes the output to STDOUT.
    ///
    /// # Errors
    ///
    /// Return an [`io::Error`] if unsuccessful.
    ///
    /// [`io::Error`]: https://doc.rust-lang.org/std/io/struct.Error.html
    fn collapse_file<P>(&mut self, infile: Option<P>) -> io::Result<()>
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
