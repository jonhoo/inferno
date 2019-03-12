use super::Collapse;
use std::collections::{HashMap, VecDeque};
use std::io;
use std::io::prelude::*;

/// Settings that change how frames are named from the incoming stack traces.
///
/// All options default to off.
#[derive(Clone, Debug, Default)]
pub struct Options {
    /// include function offset (except leafs)
    pub includeoffset: bool,
}

/// A stack collapser for the output of dtrace `ustrace()`.
///
/// To construct one, either use `dtrace::Folder::default()` or create an [`Options`] and use
/// `dtrace::Folder::from(options)`.
#[derive(Default)]
pub struct Folder {
    /// Function entries on the stack in this entry thus far.
    stack: VecDeque<String>,

    /// Number of times each call stack has been seen.
    occurrences: HashMap<String, usize>,

    /// Keep track of stack string size while we consume a stack
    stack_str_size: usize,

    /// vecotr for processing java stuff
    cache_inlines: Vec<String>,

    opt: Options,
}

impl Collapse for Folder {
    fn collapse<R, W>(&mut self, mut reader: R, writer: W) -> io::Result<()>
    where
        R: BufRead,
        W: Write,
    {
        let mut line = String::new();

        // skip header lines -- first empty line marks start of data
        loop {
            line.clear();
            if reader.read_line(&mut line)? == 0 {
                // We reached the end :( this should not happen.
                warn!("File ended while skipping headers");
                return Ok(());
            };
            if line.trim().is_empty() {
                break;
            }
        }
        loop {
            line.clear();

            if reader.read_line(&mut line)? == 0 {
                break;
            }

            let line = line.trim();

            if line.is_empty() {
                continue;
            } else if let Ok(count) = line.parse::<usize>() {
                self.on_stack_end(count);
            } else {
                self.on_stack_line(line);
            }
        }
        self.finish(writer)
    }

    fn is_applicable(&mut self, input: &str) -> Option<bool> {
        let mut found_empty_line = false;
        let mut found_stack_line = false;
        let mut found_count_line = false;
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
                if found_count_line && found_stack_line {
                    return Some(true);
                }
                found_empty_line = true;
            } else if found_empty_line {
                if line.parse::<usize>().is_ok() {
                    if found_count_line || !found_stack_line {
                        // Either multiple count lines, or a count line with no stack lines
                        return Some(false);
                    }
                    found_count_line = true;
                } else {
                    if found_count_line {
                        // Found count line before stack lines
                        return Some(false);
                    }
                    found_stack_line = true;
                }
            }
        }

        None
    }
}

impl From<Options> for Folder {
    fn from(opt: Options) -> Self {
        Self {
            stack: VecDeque::default(),
            occurrences: HashMap::default(),
            cache_inlines: Vec::new(),
            opt,
            stack_str_size: 0,
        }
    }
}

impl Folder {
    // This function approximates the Perl regex s/(::.*)[(<].*/$1/
    // from https://github.com/brendangregg/FlameGraph/blob/1b1c6deede9c33c5134c920bdb7a44cc5528e9a7/stackcollapse.pl#L88
    fn uncpp(probe: &str) -> &str {
        if let Some(scope) = probe.find("::") {
            if let Some(open) = probe[scope + 2..].rfind(|c| c == '(' || c == '<') {
                &probe[..scope + 2 + open]
            } else {
                probe
            }
        } else {
            probe
        }
    }

    fn remove_offset(line: &str) -> &str {
        let mut line = line.rsplitn(2, '+');
        // at least one element will be returned
        let second = line.next().unwrap();
        if let Some(first) = line.next() {
            first
        } else {
            second
        }
    }

    // we have a stack line that shows one stack entry from the preceeding event, like:
    //
    //     unix`tsc_gethrtimeunscaled+0x21
    //     genunix`gethrtime_unscaled+0xa
    //     genunix`syscall_mstate+0x5d
    //     unix`sys_syscall+0x10e
    //       1
    fn on_stack_line(&mut self, line: &str) {
        let line = line.trim_start();
        let frame = if self.opt.includeoffset {
            line
        } else {
            Self::remove_offset(line)
        };

        let mut frame = Self::uncpp(frame);

        if frame.is_empty() {
            frame = "-";
        };

        let mut inline = false;
        for func in frame.split("->") {
            let mut func = func.trim_start_matches('L').replace(';', ":");
            if inline {
                func.push_str("_[i]")
            };
            inline = true;
            self.stack_str_size += func.len() + 1;
            self.cache_inlines.push(func);
        }
        while let Some(func) = self.cache_inlines.pop() {
            self.stack.push_front(func);
        }
    }

    fn on_stack_end(&mut self, count: usize) {
        // allocate a string that is long enough to hold the entire stack string
        let mut stack_str = String::with_capacity(self.stack_str_size);

        let mut first = true;
        // add the other stack entries (if any)
        let last = self.stack.len() - 1;
        for (i, e) in self.stack.drain(..).enumerate() {
            if first {
                first = false
            } else {
                stack_str.push_str(";");
            }
            //trim leaf offset if these were retained:
            if self.opt.includeoffset && i == last {
                stack_str.push_str(Self::remove_offset(&e));
            } else {
                stack_str.push_str(&e);
            }
        }
        // count it!
        *self.occurrences.entry(stack_str).or_insert(0) += count;
        // reset for the next event
        self.stack_str_size = 0;
        self.stack.clear();
    }

    fn finish<W: Write>(&self, mut writer: W) -> io::Result<()> {
        let mut keys: Vec<_> = self.occurrences.keys().collect();
        keys.sort();
        for key in keys {
            writeln!(writer, "{} {}", key, self.occurrences[key])?;
        }
        Ok(())
    }
}

#[cfg(test)]
#[test]
fn cpp_test() {
    let probe = "TestClass::TestClass2(const char*)[__1cJTestClass2t6Mpkc_v_]";
    assert_eq!("TestClass::TestClass2", Folder::uncpp(probe));

    let probe = "TestClass::TestClass2::TestClass3(const char*)[__1cJTestClass2t6Mpkc_v_]";
    assert_eq!("TestClass::TestClass2::TestClass3", Folder::uncpp(probe));

    let probe = "TestClass::TestClass2<blargh>(const char*)[__1cJTestClass2t6Mpkc_v_]";
    assert_eq!("TestClass::TestClass2<blargh>", Folder::uncpp(probe));

    let probe = "TestClass::TestClass2::TestClass3<blargh>(const char*)[__1cJTestClass2t6Mpkc_v_]";
    assert_eq!(
        "TestClass::TestClass2::TestClass3<blargh>",
        Folder::uncpp(probe)
    );
}
