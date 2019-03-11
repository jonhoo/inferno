/// Stack collapsing for the output of [`dtrace`](https://www.joyent.com/dtrace).
///
/// See the [crate-level documentation] for details.
///
///   [crate-level documentation]: ../../index.html
pub mod dtrace;

/// Stack collapsing for the output of [`perf script`](https://linux.die.net/man/1/perf-script).
///
/// See the [crate-level documentation] for details.
///
///   [crate-level documentation]: ../../index.html
pub mod perf;

use std::fs::File;
use std::io::{self, Write};
use std::path::Path;

const READER_CAPACITY: usize = 128 * 1024;

/// The abstract behavior of stack collapsing.
///
/// Implementors of this trait are providing a way to take the stack traces produced by a
/// particular profiler's output (like `perf script`) and produce lines in the folded stack format
/// expected by [`crate::flamegraph::from_sorted_lines`].
///
/// See also the [crate-level documentation] for details.
///
///   [crate-level documentation]: ../index.html
// https://github.com/rust-lang/rust/issues/45040
// #[doc(spotlight)]
pub trait Collapse {
    /// Collapses the contents of the provided `reader` and writes folded stack lines to the
    /// provided `writer`.
    fn collapse<R, W>(&mut self, reader: R, writer: W) -> io::Result<()>
    where
        R: io::BufRead,
        W: io::Write;

    /// Collapses the contents of a file (or of STDIN if `infile` is `None`) and writes folded
    /// stack lines to provided `writer`.
    fn collapse_file<P, W>(&mut self, infile: Option<P>, writer: W) -> io::Result<()>
    where
        P: AsRef<Path>,
        W: Write,
    {
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
