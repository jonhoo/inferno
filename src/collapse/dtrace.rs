use std::collections::VecDeque;
use std::io::{self, prelude::*};
use std::mem;

use crossbeam::channel;
use log::warn;

use crate::collapse::{self, Collapse, Occurrences};

/// Dtrace folder configuration options.
#[derive(Clone, Debug)]
pub struct Options {
    /// Include function offset (except leafs). Default is `false`.
    pub includeoffset: bool,

    /// The number of threads to use. Default is the number of logical cores on your machine.
    pub nthreads: usize,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            includeoffset: false,
            nthreads: *collapse::DEFAULT_NTHREADS,
        }
    }
}

/// A stack collapser for the output of dtrace `ustrace()`.
///
/// To construct one, either use `dtrace::Folder::default()` or create an [`Options`] and use
/// `dtrace::Folder::from(options)`.
pub struct Folder {
    /// Vector for processing java stuff
    cache_inlines: Vec<String>,

    /// Number of stacks in each job sent to the threadpool.
    nstacks_per_job: usize,

    /// Number of times each call stack has been seen.
    occurrences: Occurrences,

    /// Function entries on the stack in this entry thus far.
    stack: VecDeque<String>,

    /// Keep track of stack string size while we consume a stack
    stack_str_size: usize,

    opt: Options,
}

impl From<Options> for Folder {
    fn from(mut opt: Options) -> Self {
        if opt.nthreads == 0 {
            opt.nthreads = 1;
        }
        Self {
            cache_inlines: Vec::new(),
            nstacks_per_job: collapse::NSTACKS_PER_JOB,
            occurrences: Occurrences::new(opt.nthreads),
            stack: VecDeque::default(),
            stack_str_size: 0,
            opt,
        }
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
        // skip header lines -- first empty line marks start of data
        let mut line = String::new();
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

        // Do collapsing...
        if self.occurrences.is_concurrent() {
            self.collapse_multi_threaded(reader)?;
        } else {
            self.collapse_single_threaded(reader)?;
        }

        // Write results...
        self.occurrences.write_and_clear(writer)?;

        // Reset state...
        self.stack.clear();
        self.stack_str_size = 0;

        Ok(())
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

    #[cfg(test)]
    fn set_nstacks_per_job(&mut self, n: usize) {
        self.nstacks_per_job = n;
    }

    #[cfg(test)]
    fn set_nthreads(&mut self, n: usize) {
        self.opt.nthreads = n;
        self.occurrences = Occurrences::new(n);
    }
}

impl Folder {
    fn collapse_single_threaded<R>(&mut self, mut reader: R) -> io::Result<()>
    where
        R: io::BufRead,
    {
        let mut line = String::new();
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
        assert_ne!(self.nstacks_per_job, 0);
        assert!(self.occurrences.is_concurrent());
        assert!(self.opt.nthreads > 1);

        crossbeam::thread::scope(|scope| {
            // Spin up the threadpool / worker threads.
            let (tx_input, rx_input) = channel::bounded::<Option<Vec<u8>>>(2 * self.opt.nthreads);
            let (tx_output, rx_output) = channel::bounded::<io::Result<()>>(1);
            let mut handles = Vec::with_capacity(self.opt.nthreads);
            for _ in 0..self.opt.nthreads {
                let rx_input = rx_input.clone();
                let tx_output = tx_output.clone();

                let nstacks_per_job = self.nstacks_per_job;
                let occurrences = self.occurrences.clone();
                let opt = self.opt.clone();

                let handle = scope.spawn(move |_| {
                    let mut folder = Folder {
                        cache_inlines: Vec::default(),
                        nstacks_per_job,
                        occurrences,
                        stack: VecDeque::default(),
                        stack_str_size: 0,
                        opt,
                    };
                    while let Some(data) = rx_input.recv().unwrap() {
                        if let Err(e) = folder.collapse_single_threaded(&data[..]) {
                            // Use `try_send` here because we only need to send one
                            // error back to the main thread for propagation (even if
                            // multiple threads fail). If multiple threads fail,
                            // the first one to do so will fill up our output channel
                            // (which only has one slot); so plain `send` would block
                            // here, which can't happen because we need each thread to
                            // continuously pull values off the input channel or else
                            // it may fill up and clog. This is also why we continue
                            // pulling data off the input channel even after we try
                            // to send an error.
                            let _ = tx_output.try_send(Err(e));
                            loop {
                                if rx_input.recv().unwrap().is_none() {
                                    break;
                                }
                            }
                        }
                        folder.stack.clear();
                        folder.stack_str_size = 0;
                    }
                });
                handles.push(handle);
            }

            // State for the loop...
            let buf_capacity =
                usize::next_power_of_two(collapse::NBYTES_PER_STACK_GUESS * self.nstacks_per_job);
            let mut buf = Vec::with_capacity(buf_capacity);
            let (mut index, mut nstacks) = (0, 0);

            // The loop...
            loop {
                // First, read some data into the `utils::Buffer`.
                let n = reader.read_until(b'\n', &mut buf)?;

                // If we're at the end of the data...
                if n == 0 {
                    // Send the last slice and exit the loop.
                    tx_input.send(Some(buf)).unwrap();
                    break;
                }

                let line = &buf[index..index + n];
                index += n;

                if is_end_of_stack(line) {
                    // It's the end of a stack; count it
                    nstacks += 1;
                    // If we've seen enough stacks to make up a slice...
                    if nstacks == self.nstacks_per_job {
                        // Send it.
                        let buf_capacity = buf.capacity();
                        let chunk = mem::replace(&mut buf, Vec::with_capacity(buf_capacity));
                        tx_input.send(Some(chunk)).unwrap();
                        // Reset the state; mark the beginning of the next slice.
                        index = 0;
                        nstacks = 0;
                    }
                }
            }

            // Send shutdown signal.
            for _ in &handles {
                tx_input.send(None).unwrap();
            }

            // Retrieve error, if any.
            drop(tx_output);
            if let Ok(result) = rx_output.recv() {
                result?;
            }

            // Join threads.
            for handle in handles {
                handle.join().unwrap();
            }

            Ok(())
        })
        .unwrap()
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

// This function should do the same thing as:
// ```
// fn is_end_of_stack(line: &[u8]) -> bool {
//     match std::str::from_utf8(line) {
//         Ok(line) => {
//             let line = line.trim();
//             match line.parse::<usize>() {
//                 Ok(_) => true,
//                 Err(_) => false,
//             }
//         }
//         Err(_) => false,
//     }
// }
// ```
// But it is much faster since it works directly on bytes and because all we're interested in is
// whether the provided bytes **can** be parsed into a `usize`, not which `usize` they might
// actually parse into. We don't need to validate that the input is utf8 and, again, we don't need
// to keep track of what the number we're parsing actually is.
//
// Benchmarking results for the two methods:
// * Using the method above: 281 MiB/s
// * Using the method below: 437 MiB/s
//
fn is_end_of_stack(line: &[u8]) -> bool {
    // In order to return `true`, as we iterate over the provided bytes, we need to progress
    // through each of the follow states, in order; if we can't, immediately return `false`.
    enum State {
        StartOfLine,  // Accept any number of whitespace characters
        MiddleOfLine, // Accept any number of ascii digits
        EndOfLine,    // Accept any number of whitespace characters
    }
    let mut state = State::StartOfLine;
    for b in line {
        let b = *b;
        let c = b as char;
        match state {
            State::StartOfLine => {
                if c.is_whitespace() {
                    continue;
                // The check below determines if the byte is an ascii digits, as digits lie
                // between 47 and 58 in the ascii table. For an unknown reason, doing the check
                // this way is much faster on my machine than calling the `is_ascii_digit` method
                // on `char`. Oddly, the reverse is true for checking if the byte is whitespace.
                // On my machine the `is_whitespace` method on `char` is faster than doing a
                // manual check of the value of the byte itself.
                } else if b > 47 && b < 58 {
                    state = State::MiddleOfLine;
                    continue;
                } else {
                    return false;
                }
            }
            State::MiddleOfLine => {
                // See comment above.
                if b > 47 && b < 58 {
                    continue;
                } else if c.is_whitespace() {
                    state = State::EndOfLine;
                } else {
                    return false;
                }
            }
            State::EndOfLine => {
                if c.is_whitespace() {
                    continue;
                } else {
                    return false;
                }
            }
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;

    use lazy_static::lazy_static;
    use pretty_assertions::assert_eq;
    use rand::{Rng, SeedableRng};

    use super::*;
    use crate::collapse::tests_common;

    lazy_static! {
        static ref INPUT: Vec<PathBuf> = {
            [
                "./flamegraph/example-dtrace-stacks.txt",
                "./tests/data/collapse-dtrace/flamegraph-bug.txt",
                "./tests/data/collapse-dtrace/hex-addresses.txt",
                "./tests/data/collapse-dtrace/java.txt",
                "./tests/data/collapse-dtrace/only-header-lines.txt",
                "./tests/data/collapse-dtrace/scope_with_no_argument_list.txt",
            ]
            .into_iter()
            .map(PathBuf::from)
            .collect::<Vec<_>>()
        };
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

    #[test]
    fn test_collapse_multi_dtrace() -> io::Result<()> {
        let mut folder = Folder::default();
        tests_common::test_collapse_multi(&mut folder, &INPUT)
    }

    #[test]
    #[ignore]
    fn test_collapse_multi_dtrace_simple() -> io::Result<()> {
        let path = "./flamegraph/example-dtrace-stacks.txt";
        let mut file = fs::File::open(path)?;
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes)?;
        let mut folder = Folder::default();
        folder.collapse(&bytes[..], io::sink())
    }

    /// Varies the nstacks_per_job parameter and outputs the 10 fastests configurations by file.
    ///
    /// Command: `cargo test bench_nstacks_dtrace --release -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn bench_nstacks_dtrace() -> io::Result<()> {
        let mut folder = Folder::default();
        tests_common::bench_nstacks(&mut folder, &INPUT)
    }

    #[test]
    #[ignore]
    /// Fuzz test the multithreaded collapser.
    ///
    /// Command: `cargo test fuzz_collapse_dtrace --release -- --ignored --nocapture`
    fn fuzz_collapse_dtrace() -> io::Result<()> {
        let seed = rand::thread_rng().gen::<u64>();
        println!("Random seed: {}", seed);
        let mut rng = rand::rngs::SmallRng::seed_from_u64(seed);

        let mut buf_actual = Vec::new();
        let mut buf_expected = Vec::new();
        let mut count = 0;

        let inputs = tests_common::read_inputs(&INPUT)?;

        loop {
            let nstacks_per_job = rng.gen_range(1, 500 + 1);
            let options = Options {
                includeoffset: rng.gen(),
                nthreads: rng.gen_range(2, 32 + 1),
            };

            for (path, input) in inputs.iter() {
                buf_actual.clear();
                buf_expected.clear();

                let mut folder = {
                    let mut options = options.clone();
                    options.nthreads = 1;
                    Folder::from(options)
                };
                folder.nstacks_per_job = nstacks_per_job;
                folder.collapse(&input[..], &mut buf_expected)?;
                let expected = std::str::from_utf8(&buf_expected[..]).unwrap();

                let mut folder = Folder::from(options.clone());
                folder.nstacks_per_job = nstacks_per_job;
                folder.collapse(&input[..], &mut buf_actual)?;
                let actual = std::str::from_utf8(&buf_actual[..]).unwrap();

                if actual != expected {
                    eprintln!(
                        "Failed on file: {}\noptions: {:#?}\n",
                        path.display(),
                        options
                    );
                    assert_eq!(actual, expected);
                }
            }

            count += 1;
            if count % 10 == 0 {
                println!("Successfully ran {} fuzz tests.", count);
            }
        }
    }

}
