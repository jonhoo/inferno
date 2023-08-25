use std::io::{self, BufRead};

use log::warn;

use crate::collapse::common::Occurrences;
use crate::collapse::Collapse;

// If source is included traces start after this line (ignoring spaces)
static START_LINE: &[&str] = &[
    "COST", "CENTRE", "MODULE", "SRC", "no.", "entries", "%time", "%alloc", "%time", "%alloc",
];

/// `ghcprof` folder configuration options.
#[derive(Clone, Debug, Default)]
#[non_exhaustive]
pub struct Options {
    /// Column to source associated value from, default is `Source::PercentTime`.
    pub source: Source,
}

/// `ghcprof` folder configuration options.
#[derive(Clone, Debug, Default)]
#[non_exhaustive]
pub enum Source {
    #[default]
    /// The %time column
    PercentTime,
    /// The ticks column
    Ticks,
    /// The bytes column
    Bytes,
}

/// A stack collapser for the output of `ghc`'s prof files.
///
/// To construct one, either use `ghcprof::Folder::default()` or create an [`Options`] and use
/// `ghcprof::Folder::from(options)`.
#[derive(Clone, Default)]
pub struct Folder {
    /// Cost for the current stack frame.
    current_cost: usize,

    /// Function on the stack in this entry thus far.
    stack: Vec<String>,

    opt: Options,
}

// the first character and last + 1
#[derive(Debug)]
struct Cols {
    cost_centre: usize,
    module: usize,
    source: usize,
}

impl Collapse for Folder {
    fn collapse<R, W>(&mut self, mut reader: R, writer: W) -> io::Result<()>
    where
        R: io::BufRead,
        W: io::Write,
    {
        // Consume the header...
        let mut line = Vec::new();
        let cols = loop {
            line.clear();
            if reader.read_until(b'\n', &mut line)? == 0 {
                warn!("File ended before start of call graph");
                return Ok(());
            };
            let l = String::from_utf8_lossy(&line);

            if l.split_whitespace()
                .take(START_LINE.len())
                .eq(START_LINE.iter().cloned())
            {
                let cost_centre = 0;
                let module = l.find(START_LINE[2]).unwrap_or(0);
                // Pick out these fixed columns, first two are individual only
                // "%time %alloc   %time %alloc"
                // `ticks` and `bytes` columns might appear on the end
                // ticks header is right aligned
                // bytes header tries to be right aligned but has a max width
                // "%time %alloc   %time %alloc  ticks  bytes"
                let source = match self.opt.source {
                    Source::PercentTime => l.find("%time").unwrap(),
                    // ticks and bytes columns are weirdly aligned so find the end of the col before
                    Source::Ticks => l.rfind("%alloc").unwrap() + 6,
                    Source::Bytes => l.rfind("ticks").unwrap() + 5,
                };
                break Cols {
                    cost_centre,
                    module,
                    source,
                };
            }
        };
        // Skip one line
        reader.read_until(b'\n', &mut line)?;

        // Process the data...
        let mut occurrences = Occurrences::new(1);
        loop {
            line.clear();
            if reader.read_until(b'\n', &mut line)? == 0 {
                break;
            }
            let l = String::from_utf8_lossy(&line);
            let line = l.trim_end();
            if line.is_empty() {
                break;
            } else {
                self.on_line(line, &mut occurrences, &cols)?;
            }
        }

        // Write the results...
        occurrences.write_and_clear(writer)?;

        // Reset the state...
        self.current_cost = 0;
        self.stack.clear();
        Ok(())
    }

    /// Check for start line of a call graph.
    fn is_applicable(&mut self, input: &str) -> Option<bool> {
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

            if line
                .split_whitespace()
                .take(START_LINE.len())
                .eq(START_LINE.iter().cloned())
            {
                return Some(true);
            }
        }
        None
    }
}

impl From<Options> for Folder {
    fn from(opt: Options) -> Self {
        Folder {
            opt,
            ..Default::default()
        }
    }
}

impl Folder {
    // Handle call graph lines of the form:
    //
    // MAIN           MAIN ...
    //  CAF           Options.Applicative.Builder ...
    //   defaultPrefs Options.Applicative.Builder ...
    //    idm         Options.Applicative.Builder ...
    //    prefs       Options.Applicative.Builder ...
    //   fullDesc     Options.Applicative.Builder ...
    //   hidden       Options.Applicative.Builder ...
    //   option       Options.Applicative.Builder ...
    //    metavar     Options.Applicative.Builder ...
    //  CAF           Options.Applicative.Builder.Internal ...
    //   internal     Options.Applicative.Builder.Internal ...
    //   noGlobal     Options.Applicative.Builder.Internal ...
    //   optionMod    Options.Applicative.Builder.Internal ...

    fn on_line(
        &mut self,
        line: &str,
        occurrences: &mut Occurrences,
        cols: &Cols,
    ) -> io::Result<()> {
        if let Some(indent_chars) = line.find(|c| c != ' ') {
            let prev_len = self.stack.len();
            let depth = indent_chars;

            if depth < prev_len {
                // If the line is not a child, pop stack to the stack before the new depth
                self.stack.truncate(depth);
            } else if depth != prev_len {
                return invalid_data_error!("Skipped indentation level at line:\n{}", line);
            }
            // There can be non-ascii names so take care to char offset not byte offset
            let string_range = |col_start: usize| {
                line.chars()
                    .skip(col_start)
                    .skip_while(|c| c.is_whitespace())
                    .take_while(|c| !c.is_whitespace())
                    .collect::<String>()
            };
            let cost = string_range(cols.source);
            if let Ok(cost) = cost.trim().parse::<f64>() {
                let func = string_range(cols.cost_centre);
                let module = string_range(cols.module);
                // Costs include self + calls (at least in what we parse)
                self.current_cost = match self.opt.source {
                    Source::PercentTime => cost * 10.0, // Do not lose the 1 decimal place
                    Source::Ticks => cost,
                    Source::Bytes => cost,
                } as usize;
                self.stack.push(format!("{}.{}", module.trim(), func.trim()));
                // identical stacks from other threads can appear so need to insert or add
                occurrences.insert_or_add(self.stack.join(";"), self.current_cost);
            } else {
                return invalid_data_error!("Invalid cost field: \"{}\"", cost);
            }
        }

        Ok(())
    }
}
