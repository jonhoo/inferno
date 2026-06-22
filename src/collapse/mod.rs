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

/// Internal string match helper functions for perf
pub(crate) mod matcher;

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

/// Collapse direct recursive backtraces.
///
/// Post-process a stack list and merge direct recursive calls.
///
/// For example, collapses
/// ```text
/// main;recursive;recursive;recursive;helper 1
/// ```
/// into
/// ```text
/// main;recursive;helper 1
/// ```
///
/// See the [crate-level documentation] for details.
///
///   [crate-level documentation]: ../../index.html
pub mod recursive;

/// Stack collapsing for the output of the [Visual Studio built-in profiler](https://docs.microsoft.com/en-us/visualstudio/profiling/profiling-feature-tour?view=vs-2019).
///
/// See the [crate-level documentation] for details.
///
///   [crate-level documentation]: ../../index.html
pub mod vsprof;

/// Stack collapsing for the output of the [xctrace](https://developer.apple.com/xcode/features/).
///
/// See the [crate-level documentation] for details.
///
///   [crate-level documentation]: ../../index.html
pub mod xctrace;

/// Stack collapsing for the output of the [GHC's built-in profiler](https://downloads.haskell.org/ghc/latest/docs/users_guide/profiling.html).
///
/// See the [crate-level documentation] for details.
///
///   [crate-level documentation]: ../../index.html
pub mod ghcprof;

// DEFAULT_NTHREADS is public because we use it in the help text of the binaries,
// but it doesn't need to be exposed to library users, hence #[doc(hidden)].
#[doc(hidden)]
pub use self::common::DEFAULT_NTHREADS;

use std::fs::File;
use std::io::{self, IsTerminal};
use std::path::Path;

use self::common::{CollapsePrivate, CAPACITY_READER};
use crate::flamegraph;

/// Collapsed stack data, ready for flamegraph rendering.
///
/// Produced by [`Collapse::collapse_to_stacks`]. Can be passed to
/// [`flamegraph::from_sorted_stacks`] via [`samples()`](Self::samples), or rendered directly via
/// [`flame_graph()`](Self::flame_graph).
///
/// This wrapper performs no sorting or sort validation. When constructing this type, take care to
/// order the stack entries correctly (usually: sorted) such that `samples` and `flame_graph`
/// uphold the expected sorting.
#[derive(Debug, Clone, Default)]
pub struct FoldedStacks {
    /// `(stack_string, count, delta)` tuples.
    ///
    /// Stacks use `;` as the frame delimiter internally (kept for storage efficiency).
    ///
    /// The delta is `Some` for differential flamegraph data.
    entries: Vec<(String, u64, Option<isize>)>,
}

impl FoldedStacks {
    /// Iterate over the stack entries as [`flamegraph::Sample`] values.
    pub fn samples(
        &self,
    ) -> impl ExactSizeIterator<Item = flamegraph::Sample<std::str::Split<'_, char>>> + '_ {
        self.entries.iter().map(|(stack, count, delta)| {
            let mut sample = flamegraph::Sample::new(stack.split(';'), *count);
            sample.delta = *delta;
            sample
        })
    }

    /// Render the collapsed stacks directly to SVG.
    ///
    /// Note that this function assumes that the stack entries are in sorted order.
    pub fn flame_graph<W: io::Write>(
        &self,
        opt: &flamegraph::Options,
        palette_map: Option<&mut flamegraph::color::PaletteMap>,
        writer: W,
    ) -> io::Result<()> {
        flamegraph::from_sorted_stacks(opt, palette_map, self.samples(), writer)
    }

    /// Returns the number of distinct stacks.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Returns `true` if there are no stacks.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// Construct `FoldedStacks` from an iterator of `(stack, count, delta)` tuples.
///
/// The caller is responsible for providing entries in sorted order (by stack
/// string). No runtime sort is performed.
impl FromIterator<(String, u64, Option<isize>)> for FoldedStacks {
    fn from_iter<I: IntoIterator<Item = (String, u64, Option<isize>)>>(iter: I) -> Self {
        FoldedStacks {
            entries: iter.into_iter().collect(),
        }
    }
}

/// Extend `FoldedStacks` with additional `(stack, count, delta)` tuples.
///
/// The caller is responsible for maintaining sorted order across the
/// combined entries. No runtime re-sort is performed.
impl Extend<(String, u64, Option<isize>)> for FoldedStacks {
    fn extend<I: IntoIterator<Item = (String, u64, Option<isize>)>>(&mut self, iter: I) {
        self.entries.extend(iter);
    }
}

/// The abstract behavior of stack collapsing.
///
/// Implementors of this trait are providing a way to take the stack traces produced by a
/// particular profiler's output (like `perf script`) and produce lines in the folded stack format
/// expected by [`crate::flamegraph::from_lines`].
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
                let stdin = io::stdin();
                let stdin_guard = stdin.lock();
                let reader = io::BufReader::with_capacity(CAPACITY_READER, stdin_guard);
                self.collapse(reader, writer)
            }
        }
    }

    /// Collapses the contents of the provided file (or of STDIN if `infile` is `None`) and
    /// writes folded stack lines to STDOUT.
    fn collapse_file_to_stdout<P>(&mut self, infile: Option<P>) -> io::Result<()>
    where
        P: AsRef<Path>,
    {
        if std::io::stdout().is_terminal() {
            self.collapse_file(infile, io::stdout().lock())
        } else {
            self.collapse_file(infile, io::BufWriter::new(io::stdout().lock()))
        }
    }

    /// Returns whether this implementation is appropriate for the given input.
    ///
    /// - `None` means "not sure -- need more input"
    /// - `Some(true)` means "yes, this implementation should work with this string"
    /// - `Some(false)` means "no, this implementation definitely won't work"
    #[allow(clippy::wrong_self_convention)]
    fn is_applicable(&mut self, input: &str) -> Option<bool>;

    /// Collapses the contents of the provided `reader` into structured [`FoldedStacks`], bypassing
    /// text serialization.
    ///
    /// The returned `FoldedStacks` can be rendered directly via [`FoldedStacks::flame_graph`] or
    /// iterated as typed [`flamegraph::Sample`] values via [`FoldedStacks::samples`].
    ///
    /// The default implementation collapses to an intermediate text buffer and then parses it.
    /// This is useful for collapser types that implement `Collapse` directly without going through
    /// `CollapsePrivate`. Implementations that use `CollapsePrivate` get a more efficient override
    /// via the blanket impl.
    fn collapse_to_stacks<R>(&mut self, reader: R) -> io::Result<FoldedStacks>
    where
        R: io::BufRead,
    {
        // Collapse to a text buffer using the standard `collapse` path, then
        // parse the "stack count" lines back into sorted `FoldedStacks` entries.
        //
        // Each line is either:
        //   "<stack> <count>"                  (normal)
        //   "<stack> <original_count> <count>" (differential)
        let mut buf = Vec::new();
        self.collapse(reader, &mut buf)?;
        let text =
            String::from_utf8(buf).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        let mut entries: Vec<_> = text
            .lines()
            .filter_map(|line| {
                let (stack, last_num) = line.trim_end().rsplit_once(' ')?;
                let last_num = last_num.parse::<u64>().ok()?;

                // Check for a second trailing number (differential format).
                if let Some((stack, penul_num)) = stack.trim_end().rsplit_once(' ') {
                    if let Ok(penul_num) = penul_num.parse::<u64>() {
                        let delta = last_num as i64 - penul_num as i64;
                        return Some((stack.to_owned(), last_num, Some(delta as isize)));
                    }
                }

                Some((stack.to_owned(), last_num, None))
            })
            .collect();
        entries.sort_unstable();
        Ok(FoldedStacks { entries })
    }
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

    fn collapse_to_stacks<R>(&mut self, reader: R) -> io::Result<FoldedStacks>
    where
        R: io::BufRead,
    {
        let mut occurrences = <Self as CollapsePrivate>::collapse_to_occurrences(self, reader)?;
        let entries = occurrences.drain_sorted_entries();
        Ok(FoldedStacks { entries })
    }
}
