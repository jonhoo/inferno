use std::io::prelude::*;
use std::io::{self, Cursor};

use log::{error, info};

use crate::collapse::{self, dtrace, perf, sample, Collapse};

const LINES_PER_ITERATION: usize = 10;

/// Folder configuration options.
#[derive(Clone, Debug)]
pub struct Options {
    /// The number of threads to use. Default is the number of logical cores on your machine.
    pub nthreads: usize,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            nthreads: *collapse::DEFAULT_NTHREADS,
        }
    }
}

/// A collapser that tries to find an appropriate implementation of `Collapse`
/// based on the input, then delegates to that collapser if one is found.
///
/// If no applicable collapser is found, an error will be logged and
/// nothing will be written.
pub struct Folder {
    opt: Options,
}

impl From<Options> for Folder {
    fn from(opt: Options) -> Self {
        Self { opt }
    }
}

impl Default for Folder {
    fn default() -> Self {
        Options::default().into()
    }
}

impl Collapse for Folder {
    fn collapse<R, W>(&mut self, mut reader: R, writer: W) -> io::Result<()>
    where
        R: io::BufRead,
        W: io::Write,
    {
        let mut dtrace = {
            let mut options = dtrace::Options::default();
            options.nthreads = self.opt.nthreads;
            dtrace::Folder::from(options)
        };
        let mut perf = {
            let mut options = perf::Options::default();
            options.nthreads = self.opt.nthreads;
            perf::Folder::from(options)
        };
        let mut sample = sample::Folder::from(sample::Options::default());

        // Each Collapse impl gets its own flag in this array.
        // It gets set to true when the impl has been ruled out.
        let mut not_applicable = [false; 3];

        let mut buffer = String::new();
        loop {
            let mut eof = false;
            for _ in 0..LINES_PER_ITERATION {
                if reader.read_line(&mut buffer)? == 0 {
                    eof = true;
                }
            }

            macro_rules! try_collapse_impl {
                ($collapse:ident, $index:expr) => {
                    if !not_applicable[$index] {
                        match $collapse.is_applicable(&buffer) {
                            Some(false) => {
                                // We can rule this collapser out.
                                not_applicable[$index] = true;
                            }
                            Some(true) => {
                                // We found a collapser that works! Let's use it.
                                info!("Using {} collapser", stringify!($collapse));
                                let cursor = Cursor::new(buffer).chain(reader);
                                return $collapse.collapse(cursor, writer);
                            }
                            None => (), // We're not yet sure if this collapser is appropriate
                        }
                    }
                };
            }
            try_collapse_impl!(perf, 0);
            try_collapse_impl!(dtrace, 1);
            try_collapse_impl!(sample, 2);

            if eof {
                break;
            }
        }

        error!("No applicable collapse implementation found for input");

        Ok(())
    }

    fn is_applicable(&mut self, _line: &str) -> Option<bool> {
        unreachable!()
    }

    #[cfg(test)]
    fn set_nstacks_per_job(&mut self, _: usize) {}

    #[doc(hidden)]
    fn set_nthreads(&mut self, n: usize) {
        self.opt.nthreads = n;
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use lazy_static::lazy_static;

    use super::*;
    use crate::collapse::tests_common;

    lazy_static! {
        static ref INPUT: Vec<PathBuf> = {
            [
                // perf
                "./flamegraph/example-perf-stacks.txt.gz",
                "./flamegraph/test/perf-cycles-instructions-01.txt",
                "./flamegraph/test/perf-dd-stacks-01.txt",
                "./flamegraph/test/perf-funcab-cmd-01.txt",
                "./flamegraph/test/perf-funcab-pid-01.txt",
                "./flamegraph/test/perf-iperf-stacks-pidtid-01.txt",
                "./flamegraph/test/perf-java-faults-01.txt",
                "./flamegraph/test/perf-java-stacks-01.txt",
                "./flamegraph/test/perf-java-stacks-02.txt",
                "./flamegraph/test/perf-js-stacks-01.txt",
                "./flamegraph/test/perf-mirageos-stacks-01.txt",
                "./flamegraph/test/perf-numa-stacks-01.txt",
                "./flamegraph/test/perf-rust-Yamakaky-dcpu.txt",
                "./flamegraph/test/perf-vertx-stacks-01.txt",
                "./tests/data/collapse-perf/empty-line.txt",
                "./tests/data/collapse-perf/go-stacks.txt",
                "./tests/data/collapse-perf/java-inline.txt",
                "./tests/data/collapse-perf/weird-stack-line.txt",
                // dtrace
                "./flamegraph/example-dtrace-stacks.txt",
                "./tests/data/collapse-dtrace/flamegraph-bug.txt",
                "./tests/data/collapse-dtrace/hex-addresses.txt",
                "./tests/data/collapse-dtrace/java.txt",
                "./tests/data/collapse-dtrace/only-header-lines.txt",
                "./tests/data/collapse-dtrace/scope_with_no_argument_list.txt",

            ]
            .into_iter()
            .map(PathBuf::from)
            .collect::<Vec<_>>()
        };
    }

    #[test]
    fn test_collapse_multi_guess() -> io::Result<()> {
        let mut folder = Folder::default();
        tests_common::test_collapse_multi(&mut folder, &INPUT)
    }
}
