use super::Collapse;
use crate::parse::dtrace::{self, Parser};
use crate::parse::TraceIterator;
use hashbrown::HashMap;
use std::io;
use std::io::prelude::*;
/// Configuration for the dtrace parser.
pub type Options = dtrace::Options;

/// A stack collapser for the output of dtrace `ustrace()`.
///
/// To construct one, either use `dtrace::Folder::default()` or create an [`Options`] and use
/// `dtrace::Folder::from(options)`.
#[derive(Default)]
pub struct Folder {
    opt: Options,
}

impl Collapse for Folder {
    fn collapse<R, W>(&mut self, reader: R, writer: W) -> io::Result<()>
    where
        R: BufRead,
        W: Write,
    {
        let parser = Parser::new(self.opt.clone(), reader)?;
        self.finish(parser, writer)
    }

    fn is_applicable(&mut self, input: &str) -> Option<bool> {
        let mut found_empty_line = false;
        let mut found_stack_line = false;
        let mut input = input.as_bytes();
        let mut line = String::new();
        loop {
            line.clear();
            if let Ok(n) = input.read_line(&mut line) {
                if n == 0 {
                    break;
                }
            } else {
                return Some(false);
            }

            let line = line.trim();
            if line.is_empty() {
                found_empty_line = true;
            } else if found_empty_line {
                if line.parse::<usize>().is_ok() {
                    return Some(found_stack_line);
                } else if line.contains('`')
                    || (line.starts_with("0x") && usize::from_str_radix(&line[2..], 16).is_ok())
                {
                    found_stack_line = true;
                } else {
                    // This is not a stack or count line
                    return Some(false);
                }
            }
        }
        None
    }
}

impl From<Options> for Folder {
    fn from(opt: Options) -> Self {
        Self { opt }
    }
}

impl Folder {
    fn finish<W: Write, R: BufRead>(
        &self,
        stacks: TraceIterator<Parser<R>>,
        mut writer: W,
    ) -> io::Result<()> {
        let mut occurrences = HashMap::with_capacity(512);
        for s in stacks {
            *occurrences.entry(s.stack.join(";")).or_insert(0) += s.count;
        }
        let mut keys: Vec<_> = occurrences.iter().collect();
        keys.sort();
        for (key, count) in keys {
            writeln!(writer, "{} {}", key, count)?;
        }
        Ok(())
    }
}
