use super::Collapse;
use super::{dtrace, perf};
use std::io::prelude::*;
use std::io::{self, Cursor};

/// A collapser that tries to find an appropriate implementation of `Collapse`
/// based on the input, then delegates to that collapser if one is found.
///
/// If no applicable collapser is found, and error will be logged and
/// nothing will be written.
#[derive(Default)]
pub struct Folder {}

impl Collapse for Folder {
    fn collapse<R, W>(&mut self, mut reader: R, writer: W) -> io::Result<()>
    where
        R: io::BufRead,
        W: io::Write,
    {
        let mut perf = perf::Folder::from(perf::Options::default());
        let mut dtrace = dtrace::Folder::from(dtrace::Options::default());

        // Each Collapse impl gets its own flag in this array.
        // It gets set to true when the impl has been ruled out.
        let mut not_applicable = [false; 2];

        let mut line_start = 0;
        let mut buffer = String::new();
        loop {
            let bytes = reader.read_line(&mut buffer)?;
            let line = &buffer[line_start..];

            macro_rules! try_collapse_impl {
                ($collapse:ident, $index:expr) => {
                    if !not_applicable[$index] {
                        match $collapse.is_applicable(line) {
                            Some(false) => {
                                // We can rule this collapser out.
                                not_applicable[$index] = true;
                            }
                            Some(true) => {
                                // We found a collapser that works! Let's use it.
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

            // Only break at end of input AFTER testing each implementation one last time.
            if bytes == 0 {
                break;
            }

            line_start += bytes;
        }

        error!("No applicable collapse implementation found for input");

        Ok(())
    }

    fn is_applicable(&mut self, _line: &str) -> Option<bool> {
        None
    }
}
