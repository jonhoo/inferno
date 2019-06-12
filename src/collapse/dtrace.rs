use std::collections::VecDeque;
use std::io;
use std::io::prelude::*;

use fnv::FnvHashMap;
use log::warn;

use super::Collapse;

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
    /// Vector for processing java stuff
    cache_inlines: Vec<String>,

    /// Number of times each call stack has been seen.
    occurrences: FnvHashMap<String, usize>,

    /// Function entries on the stack in this entry thus far.
    stack: VecDeque<String>,

    /// Keep track of stack string size while we consume a stack
    stack_str_size: usize,

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
        Self {
            cache_inlines: Vec::new(),
            occurrences: FnvHashMap::with_capacity_and_hasher(512, fnv::FnvBuildHasher::default()),
            stack: VecDeque::default(),
            stack_str_size: 0,
            opt,
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

    fn remove_offset(line: &str) -> (bool, bool, bool, &str) {
        let mut has_inlines = false;
        let mut could_be_cpp = false;
        let mut has_semicolon = false;
        let mut last_offset = line.len();
        // This seems risly but dtrace stacks are c-strings as can be seen in the function
        // responsible for printing them:
        // https://github.com/opendtrace/opendtrace/blob/1a03ea5576a9219a43f28b4f159ff8a4b1f9a9fd/lib/libdtrace/common/dt_consume.c#L1331
        let bytes = line.as_bytes();
        for offset in 0..bytes.len() {
            match bytes[offset] {
                b'>' if offset > 0 && bytes[offset - 1] == b'-' => has_inlines = true,
                b':' if offset > 0 && bytes[offset - 1] == b':' => could_be_cpp = true,
                b';' => has_semicolon = true,
                b'+' => last_offset = offset,
                _ => (),
            }
        }
        (
            has_inlines,
            could_be_cpp,
            has_semicolon,
            &line[..last_offset],
        )
    }

    // we have a stack line that shows one stack entry from the preceeding event, like:
    //
    //     unix`tsc_gethrtimeunscaled+0x21
    //     genunix`gethrtime_unscaled+0xa
    //     genunix`syscall_mstate+0x5d
    //     unix`sys_syscall+0x10e
    //       1
    fn on_stack_line(&mut self, line: &str) {
        let (has_inlines, could_be_cpp, has_semicolon, mut frame) = if self.opt.includeoffset {
            (true, true, true, line)
        } else {
            Self::remove_offset(line)
        };

        if could_be_cpp {
            frame = Self::uncpp(frame);
        }

        if frame.is_empty() {
            frame = "-";
        };

        if has_inlines {
            let mut inline = false;
            for func in frame.split("->") {
                let mut func = if has_semicolon {
                    func.trim_start_matches('L').replace(';', ":")
                } else {
                    func.trim_start_matches('L').to_owned()
                };
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
        } else if has_semicolon {
            self.stack.push_front(frame.replace(';', ":"))
        } else {
            self.stack.push_front(frame.to_owned())
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
                stack_str.push(';');
            }
            //trim leaf offset if these were retained:
            if self.opt.includeoffset && i == last {
                stack_str.push_str(Self::remove_offset(&e).3);
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
        let mut keys: Vec<_> = self.occurrences.iter().collect();
        keys.sort();
        for (key, count) in keys {
            writeln!(writer, "{} {}", key, count)?;
        }
        Ok(())
    }
}

#[cfg(test)]
use pretty_assertions::assert_eq;
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
