use super::{Parse, Trace, TraceIterator};
use std::collections::VecDeque;
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
pub struct Parser<R>
where
    R: BufRead,
{
    /// Function entries on the stack in this entry thus far.
    stack: VecDeque<String>,

    /// Keep track of stack string size while we consume a stack
    stack_str_size: usize,

    /// Vector for processing java stuff
    cache_inlines: Vec<String>,

    reader: R,

    opt: Options,
}

impl<R> Parse for Parser<R>
where
    R: BufRead,
{
    fn next(&mut self) -> Option<Trace> {
        let mut line = String::new();
        loop {
            line.clear();

            if let Ok(n) = self.reader.read_line(&mut line) {
                if n == 0 {
                    // We are done
                    break;
                };
            } else {
                // Failed to read
                break;
            }

            let line = line.trim();

            if line.is_empty() {
                continue;
            } else if let Ok(count) = line.parse::<u64>() {
                return Some(Trace {
                    stack: self.finish_stack(),
                    count,
                });
            } else {
                self.on_stack_line(line);
            }
        }
        None
    }
}

impl<R> Parser<R>
where
    R: BufRead,
{
    fn finish_stack(&mut self) -> Vec<String> {
        // allocate a string that is long enough to hold the entire stack string
        let mut stack: Vec<_> = self.stack.drain(..).collect();

        if self.opt.includeoffset {
            if let Some(last) = stack.pop() {
                stack.push(Self::remove_offset(&last).3.to_owned());
            }
        }
        stack
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

    /// Creates a new trace iterator over a reader of dtrace ustack traces.
    pub fn new(opt: Options, mut reader: R) -> io::Result<TraceIterator<Self>> {
        let mut line = String::new();

        // skip header lines -- first empty line marks start of data
        loop {
            line.clear();
            if reader.read_line(&mut line)? == 0 {
                // We reached the end :( this should not happen.
                warn!("File ended while skipping headers");
                break;
            };
            if line.trim().is_empty() {
                break;
            }
        }
        Ok(TraceIterator::new(Self {
            stack: VecDeque::new(),
            cache_inlines: Vec::new(),
            stack_str_size: 0,
            reader,
            opt,
        }))
    }

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
}

#[cfg(test)]
#[test]
fn cpp_test() {
    use std::fs::File;
    use std::io::BufReader;

    let probe = "TestClass::TestClass2(const char*)[__1cJTestClass2t6Mpkc_v_]";
    assert_eq!(
        "TestClass::TestClass2",
        Parser::<BufReader<File>>::uncpp(probe)
    );

    let probe = "TestClass::TestClass2::TestClass3(const char*)[__1cJTestClass2t6Mpkc_v_]";
    assert_eq!(
        "TestClass::TestClass2::TestClass3",
        Parser::<BufReader<File>>::uncpp(probe)
    );

    let probe = "TestClass::TestClass2<blargh>(const char*)[__1cJTestClass2t6Mpkc_v_]";
    assert_eq!(
        "TestClass::TestClass2<blargh>",
        Parser::<BufReader<File>>::uncpp(probe)
    );

    let probe = "TestClass::TestClass2::TestClass3<blargh>(const char*)[__1cJTestClass2t6Mpkc_v_]";
    assert_eq!(
        "TestClass::TestClass2::TestClass3<blargh>",
        Parser::<BufReader<File>>::uncpp(probe)
    );
}
