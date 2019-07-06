use std::io::prelude::*;
use std::io::{self, Cursor};

use log::{error, info};

use crate::collapse::{dtrace, perf, Collapse, DEFAULT_NSTACKS, DEFAULT_NTHREADS};

const LINES_PER_ITERATION: usize = 10;

/// Folder configuration options.
#[derive(Clone, Debug)]
pub struct Options {
    /// The number of stacks in each job sent to the threadpool (if using multiple threads).
    /// Default is 20.
    pub nstacks_per_job: usize,

    /// The number of threads to use. Default is the number of logical cores on your machine.
    pub nthreads: usize,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            nstacks_per_job: DEFAULT_NSTACKS,
            nthreads: *DEFAULT_NTHREADS,
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
            options.nstacks_per_job = self.opt.nstacks_per_job;
            options.nthreads = self.opt.nthreads;
            dtrace::Folder::from(options)
        };
        let mut perf = {
            let mut options = perf::Options::default();
            options.nstacks_per_job = self.opt.nstacks_per_job;
            options.nthreads = self.opt.nthreads;
            perf::Folder::from(options)
        };

        // Each Collapse impl gets its own flag in this array.
        // It gets set to true when the impl has been ruled out.
        let mut not_applicable = [false; 2];

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
}
