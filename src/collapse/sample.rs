use super::util::fix_partially_demangled_rust_symbol;
use super::Collapse;
use log::{error, warn};
use std::io;
use std::io::prelude::*;

// The set of symbols to ignore for 'waiting' threads, for ease of use.
// This will hide waiting threads from the view, making it easier to
// see what is actually running in the sample.
static IGNORE_SYMBOLS: &[&str] = &[
    "__psynch_cvwait",
    "__select",
    "__semwait_signal",
    "__ulock_wait",
    "__wait4",
    "__workq_kernreturn",
    "kevent",
    "mach_msg_trap",
    "read",
    "semaphore_wait_trap",
];

// The call graph begins after this line.
static START_LINE: &str = "Call graph:";

// The section after the call graph begins with this.
// We know we're done when we get to this line.
static END_LINE: &str = "Total number in stack";

/// Settings that change how frames are named from the incoming stack traces.
///
/// All options default to off.
#[derive(Clone, Debug, Default)]
pub struct Options {
    /// Don't include modules with function names
    pub no_modules: bool,
}

/// A stack collapser for the output of `sample` on macOS.
///
/// To construct one, either use `sample::Folder::default()` or create an [`Options`] and use
/// `sample::Folder::from(options)`.
#[derive(Default)]
pub struct Folder {
    /// Function on the stack in this entry thus far.
    stack: Vec<String>,

    /// Number of samples for the current stack frame.
    current_samples: usize,
    opt: Options,
}

impl Collapse for Folder {
    fn collapse<R, W>(&mut self, mut reader: R, mut writer: W) -> io::Result<()>
    where
        R: BufRead,
        W: Write,
    {
        let mut line = String::new();

        // Skip everything until we find the call graph.
        loop {
            line.clear();
            if reader.read_line(&mut line)? == 0 {
                warn!("File ended before start of call graph");
                return Ok(());
            };
            if line.starts_with(START_LINE) {
                break;
            }
        }

        loop {
            line.clear();

            if reader.read_line(&mut line)? == 0 {
                warn!("File ended before end of call graph");
                return self.write_stack(&mut writer);
            }

            let line = line.trim_end();

            if line.is_empty() {
                continue;
            } else if line.starts_with("    ") {
                self.on_line(line, &mut writer)?;
            } else if line.starts_with(END_LINE) {
                return self.write_stack(&mut writer);
            } else {
                error!("Stack line doesn't start with 4 spaces:\n{}", line);
            }
        }
    }

    /// Check for start and end lines of a call graph.
    fn is_applicable(&mut self, input: &str) -> Option<bool> {
        let mut found_start = false;
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

            if line.starts_with(START_LINE) {
                found_start = true;
                continue;
            } else if line.starts_with(END_LINE) {
                return Some(found_start);
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
    fn line_parts<'a>(&self, line: &'a str) -> Option<(&'a str, &'a str, &'a str)> {
        let mut line = line.trim_start().splitn(2, ' ');
        let time = line.next()?.trim_end();
        let line = line.next()?;

        let func = match line.find('(') {
            Some(open) => &line[..open],
            None => line,
        }
        .trim_end();

        let mut module = "";
        if !self.opt.no_modules {
            // Modules are shown with "(in libfoo.dylib)" or "(in AppKit)".
            // We've arleady split on "(in " above.
            let mut line = line.rsplitn(2, "(in ");
            if let Some(line) = line.next() {
                if let Some(close) = line.find(')') {
                    module = &line[..close];
                }

                // Remove ".dylib", since it adds no value.
                if module.ends_with(".dylib") {
                    module = &module[..module.len() - 6]
                }
            }
        }

        Some((time, func, module))
    }

    fn is_indent_char(c: char) -> bool {
        c == ' ' || c == '+' || c == '|' || c == ':' || c == '!'
    }

    // Handle call graph lines of the form:
    //
    // 5130 Thread_8749954
    //    + 5130 start_wqthread  (in libsystem_pthread.dylib) ...
    //    +   4282 _pthread_wqthread  (in libsystem_pthread.dylib) ...
    //    +   ! 4282 __doworkq_kernreturn  (in libsystem_kernel.dylib) ...
    //    +   848 _pthread_wqthread  (in libsystem_pthread.dylib) ...
    //    +     848 __doworkq_kernreturn  (in libsystem_kernel.dylib) ...
    fn on_line<W: Write>(&mut self, line: &str, writer: &mut W) -> io::Result<()> {
        if let Some(indent_chars) = line[4..].find(|c| !Self::is_indent_char(c)) {
            // Each indent is two characters
            if indent_chars % 2 != 0 {
                error!("Odd number of indentation characters for line:\n{}", line);
            }

            let prev_depth = self.stack.len();
            let depth = indent_chars / 2 + 1;

            if depth <= prev_depth {
                self.write_stack(writer)?;
                for _ in 0..=prev_depth - depth {
                    self.stack.pop();
                }
            } else if depth > prev_depth + 1 {
                error!("Skipped indentation level at line:\n{}", line);
            }

            if let Some((samples, func, module)) = self.line_parts(&line[4 + indent_chars..]) {
                if let Ok(samples) = samples.parse::<usize>() {
                    self.current_samples = samples;
                    // sample doesn't properly demangle Rust symbols, so fix those.
                    let func = fix_partially_demangled_rust_symbol(func);
                    if module.is_empty() {
                        self.stack.push(func.to_string());
                    } else {
                        self.stack.push(format!("{}`{}", module, func));
                    }
                } else {
                    error!("Invalid samples field: {}", samples);
                }
            } else {
                error!("Unable to parse stack line:\n{}", line);
            }
        } else {
            error!("Found stack line with only indent characters:\n{}", line);
        }

        Ok(())
    }

    fn write_stack<W: Write>(&self, writer: &mut W) -> io::Result<()> {
        if let Some(func) = self.stack.last() {
            for symbol in IGNORE_SYMBOLS {
                if func.ends_with(symbol) {
                    // Don't write out stacks with ignored symbols
                    return Ok(());
                }
            }
        }

        for (i, frame) in self.stack.iter().enumerate() {
            if i > 0 {
                write!(writer, ";")?;
            }
            write!(writer, "{}", frame)?;
        }
        writeln!(writer, " {}", self.current_samples)
    }
}
