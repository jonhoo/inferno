use std::collections::VecDeque;
use std::io::{self, prelude::*};

use log::warn;

use super::{Collapse, Input, Occurrences, CAPACITY_INPUT_BUFFER, CAPACITY_LINE_BUFFER};

///////////////////////////////////////////////////////////////////////////////

/// Settings that change how frames are named from the incoming stack traces.
///
/// All options default to off, expect nthreads, which defaults to the number
/// of logical cores on your machine.
#[derive(Copy, Clone, Debug)]
pub struct Options {
    /// Include function offset (except leafs)
    pub includeoffset: bool,

    /// The number of threads to use.
    pub nthreads: usize,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            includeoffset: false,
            nthreads: num_cpus::get(),
        }
    }
}

///////////////////////////////////////////////////////////////////////////////

/// A stack collapser for the output of dtrace `ustrace()`.
///
/// To construct one, either use `dtrace::Folder::default()` or create an [`Options`] and use
/// `dtrace::Folder::from(options)`.
pub struct Folder {
    /// Vector for processing java stuff
    cache_inlines: Vec<String>,

    /// Number of times each call stack has been seen.
    occurrences: Occurrences,

    /// Function entries on the stack in this entry thus far.
    stack: VecDeque<String>,

    /// Keep track of stack string size while we consume a stack
    stack_str_size: usize,

    opt: Options,
}

impl From<Options> for Folder {
    fn from(opt: Options) -> Self {
        Self {
            cache_inlines: Vec::new(),
            occurrences: Occurrences::new(opt.nthreads),
            stack: VecDeque::default(),
            stack_str_size: 0,
            opt,
        }
    }
}

impl Default for Folder {
    fn default() -> Self {
        let options = Options::default();
        Folder::from(options)
    }
}

impl Collapse for Folder {
    fn collapse<R, W>(&mut self, reader: R, writer: W) -> io::Result<()>
    where
        R: io::BufRead,
        W: io::Write,
    {
        if self.opt.nthreads <= 1 {
            self.collapse_single_threaded(reader)?;
        } else {
            self.collapse_multi_threaded(reader)?;
        }
        self.occurrences.write(writer)
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

impl Folder {
    fn collapse_single_threaded<R>(&mut self, mut reader: R) -> io::Result<()>
    where
        R: io::BufRead,
    {
        let mut line = String::with_capacity(CAPACITY_LINE_BUFFER);

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
        Ok(())
    }

    fn collapse_multi_threaded<R>(&mut self, mut reader: R) -> io::Result<()>
    where
        R: io::BufRead,
    {
        debug_assert!(self.occurrences.is_concurrent());
        debug_assert!(self.opt.nthreads > 1);

        let mut buf = Vec::with_capacity(CAPACITY_INPUT_BUFFER);
        reader.read_to_end(&mut buf)?;

        let mut input = Input::new(buf, self.opt.nthreads, Self::identify_stack_locations)?;

        crossbeam::thread::scope(|scope| {
            let mut handles = Vec::with_capacity(input.nthreads());
            let (sender, receiver) = crossbeam::channel::bounded(input.nthreads());
            for chunk in input.chunks() {
                let occurrences = self.occurrences.clone();
                let opt = self.opt;
                let sender = sender.clone();

                let handle = scope.spawn(move |_| {
                    let mut folder = Folder {
                        // state
                        cache_inlines: Vec::new(),
                        occurrences,
                        stack: VecDeque::default(),
                        stack_str_size: 0,

                        // options
                        opt,
                    };
                    let result = folder.collapse_single_threaded(chunk);
                    sender.send(result).unwrap();
                });
                handles.push(handle);
            }
            for handle in handles {
                receiver.recv().unwrap()?;
                handle.join().unwrap();
            }
            Ok::<_, io::Error>(())
        })
        .unwrap()?;

        Ok(())
    }

    // When collapsing using multiple threads, this function is run once
    // upfront in order to determine the locations (byte indices) of each
    // stack in the input data so that the input data can later be broken up
    // into pieces in order to be allocated across multiple threads.
    // See `crate::collapse::Input::new`.
    fn identify_stack_locations(mut reader: io::BufReader<&[u8]>) -> io::Result<Vec<usize>> {
        let mut byte_index = 0;
        let mut line = String::with_capacity(CAPACITY_LINE_BUFFER);
        let mut stack_indices = vec![0];
        loop {
            line.clear();
            let n = reader.read_line(&mut line).unwrap();
            if n == 0 {
                return Ok(stack_indices);
            }
            byte_index += n;
            if line.trim().is_empty() {
                break;
            }
        }
        loop {
            line.clear();
            let n = reader.read_line(&mut line).unwrap();
            if n == 0 {
                return Ok(stack_indices);
            }
            byte_index += n;
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            if line.parse::<usize>().is_ok() {
                stack_indices.push(byte_index);
            }
        }
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
        self.occurrences.add(stack_str, count);

        // reset for the next event
        self.stack_str_size = 0;
        self.stack.clear();
    }
}

///////////////////////////////////////////////////////////////////////////////

#[cfg(test)]
mod tests {
    use std::fs::File;
    use std::path::{Path, PathBuf};

    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn test_input_indices() -> io::Result<()> {
        let input = [
            "flamegraph-bug.txt",
            "hex-addresses.txt",
            "java.txt",
            "only-header-lines.txt",
            "scope_with_no_argument_list.txt",
        ]
        .into_iter()
        .map(|s| Path::new("./tests/data/collapse-dtrace").join(s))
        .collect::<Vec<PathBuf>>();

        for path in input.iter() {
            let mut infile = File::open(path)?;
            let mut expected = Vec::new();
            infile.read_to_end(&mut expected)?;

            for n in 0..16 {
                let input = Input::new(expected.clone(), n, Folder::identify_stack_locations)?;
                let mut actual: Vec<u8> = Vec::new();
                for chunk in input.chunks() {
                    actual.extend(chunk);
                }
                assert_eq!(actual, expected);
            }
        }
        Ok(())
    }

    #[test]
    fn cpp_test() {
        let probe = "TestClass::TestClass2(const char*)[__1cJTestClass2t6Mpkc_v_]";
        assert_eq!("TestClass::TestClass2", Folder::uncpp(probe));

        let probe = "TestClass::TestClass2::TestClass3(const char*)[__1cJTestClass2t6Mpkc_v_]";
        assert_eq!("TestClass::TestClass2::TestClass3", Folder::uncpp(probe));

        let probe = "TestClass::TestClass2<blargh>(const char*)[__1cJTestClass2t6Mpkc_v_]";
        assert_eq!("TestClass::TestClass2<blargh>", Folder::uncpp(probe));

        let probe =
            "TestClass::TestClass2::TestClass3<blargh>(const char*)[__1cJTestClass2t6Mpkc_v_]";
        assert_eq!(
            "TestClass::TestClass2::TestClass3<blargh>",
            Folder::uncpp(probe)
        );
    }
}
