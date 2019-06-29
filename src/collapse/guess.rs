use std::io::prelude::*;
use std::io::{self, Cursor};

use log::{error, info};

use super::{dtrace, perf, Collapse};

const LINES_PER_ITERATION: usize = 10;

/// A collapser that tries to find an appropriate implementation of `Collapse`
/// based on the input, then delegates to that collapser if one is found.
///
/// If no applicable collapser is found, an error will be logged and
/// nothing will be written.
pub struct Folder {
    nthreads: usize,
}

impl Folder {
    /// Constructs a new `guess::Folder`.
    pub fn new(nthreads: usize) -> Self {
        Self { nthreads }
    }
}

impl Default for Folder {
    fn default() -> Self {
        Self {
            nthreads: num_cpus::get(),
        }
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
            options.nthreads = self.nthreads;
            dtrace::Folder::from(options)
        };
        let mut perf = {
            let mut options = perf::Options::default();
            options.nthreads = self.nthreads;
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
