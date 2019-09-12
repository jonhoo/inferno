#[macro_use]
pub(crate) mod common;

/// Stack collapsing for the output of [`dtrace`](https://www.joyent.com/dtrace).
///
/// See the [crate-level documentation] for details.
///
///   [crate-level documentation]: ../../index.html
pub mod dtrace;

/// Attempts to use whichever Collapse implementation is appropriate for a given input
pub mod guess;

/// Stack collapsing for the output of [`perf script`](https://linux.die.net/man/1/perf-script).
///
/// See the [crate-level documentation] for details.
///
///   [crate-level documentation]: ../../index.html
pub mod perf;

/// Stack collapsing for the output of [`sample`](https://gist.github.com/loderunner/36724cc9ee8db66db305#profiling-with-sample) on macOS.
///
/// See the [crate-level documentation] for details.
///
///   [crate-level documentation]: ../../index.html
pub mod sample;

/// Stack collapsing for the output of [`VTune`](https://software.intel.com/en-us/vtune-amplifier-help-command-line-interface).
///
/// See the [crate-level documentation] for details.
///
///   [crate-level documentation]: ../../index.html
pub mod vtune;

// DEFAULT_NTHREADS is public because we use it in the help text of the binaries,
// but it doesn't need to be exposed to library users, hence #[doc(hidden)].
#[doc(hidden)]
pub use self::common::DEFAULT_NTHREADS;

use std::fs::File;
use std::io;
use std::path::Path;

use self::common::{CollapsePrivate, CAPACITY_READER};

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

    /// Collapses the contents of the provided file (or of STDIN if `infile` is `None`) and
    /// writes folded stack lines to provided `writer`.
    fn collapse_file<P, W>(&mut self, infile: Option<P>, writer: W) -> io::Result<()>
    where
        P: AsRef<Path>,
        W: io::Write,
    {
        match infile {
            Some(ref path) => {
                let file = File::open(path)?;
                let reader = io::BufReader::with_capacity(CAPACITY_READER, file);
                self.collapse(reader, writer)
            }
            None => {
                let stdio = io::stdin();
                let stdio_guard = stdio.lock();
                let reader = io::BufReader::with_capacity(CAPACITY_READER, stdio_guard);
                self.collapse(reader, writer)
            }
        }
    }

    /// Returns whether this implementation is appropriate for the given input.
    ///
    /// - `None` means "not sure -- need more input"
    /// - `Some(true)` means "yes, this implementation should work with this string"
    /// - `Some(false)` means "no, this implementation definitely won't work"
    fn is_applicable(&mut self, input: &str) -> Option<bool>;
}

impl<T> Collapse for T
where
    T: CollapsePrivate,
{
    fn collapse<R, W>(&mut self, reader: R, writer: W) -> io::Result<()>
    where
        R: io::BufRead,
        W: io::Write,
    {
        <Self as CollapsePrivate>::collapse(self, reader, writer)
    }

    fn is_applicable(&mut self, input: &str) -> Option<bool> {
        <Self as CollapsePrivate>::is_applicable(self, input)
    }
}
