use std::collections::VecDeque;
use std::io::{self, BufRead};

use crate::collapse::common::{self, CollapsePrivate, Occurrences};

mod logging {
    use log::warn;

    pub(super) fn weird_stack_line(line: &str) {
        warn!("Weird stack line: {}", line);
    }
}

/// `pmcstat` folder configuration options.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct Options {
    /// The number of threads to use.
    ///
    /// Default is the number of logical cores on your machine.
    pub nthreads: usize,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            nthreads: *common::DEFAULT_NTHREADS,
        }
    }
}

/// A stack collapser for the output of `pmcstat -G` (callchain mode).
///
/// To construct one, either use `pmc::Folder::default()` or create an [`Options`] and use
/// `pmc::Folder::from(options)`.
pub struct Folder {
    // State...

    /// The number of stacks per job to send to the threadpool.
    nstacks_per_job: usize,

    /// Function entries on the stack in this entry thus far.
    stack: VecDeque<String>,

    /// Number of leading spaces (i.e. stack depth) found on last stack line
    indent: Option<usize>,

    /// The called count found on last processed stack line
    count: Option<usize>,

    // Options...
    opt: Options,
}

impl From<Options> for Folder {
    fn from(mut opt: Options) -> Self {
        if opt.nthreads == 0 {
            opt.nthreads = 1;
        }
        Self {
            nstacks_per_job: common::DEFAULT_NSTACKS_PER_JOB,
            stack: VecDeque::default(),
            indent: None,
            count: None,
            opt,
        }
    }
}

impl Default for Folder {
    fn default() -> Self {
        Options::default().into()
    }
}

impl CollapsePrivate for Folder {
    fn pre_process<R>(&mut self, _reader: &mut R, _occurrences: &mut Occurrences) -> io::Result<()>
    where
        R: io::BufRead,
    {
        // We do not need any special pre-processing
        Ok(())
    }

    fn collapse_single_threaded<R>(
        &mut self,
        mut reader: R,
        occurrences: &mut Occurrences,
    ) -> io::Result<()>
    where
        R: io::BufRead,
    {
        // While there are still stacks left to process, process them...
        let mut line_buffer = Vec::new();
        while !self.process_single_stack(&mut line_buffer, &mut reader, occurrences)? {}

        // Reset state...
        self.stack.clear();
        self.indent = None;
        self.count = None;
        Ok(())
    }

    /// Determine if this format corresponds to the input data.
    fn is_applicable(&mut self, input: &str) -> Option<bool> {
        // First line is always of the form
        // @ <event name> [<number] samples]
        // Ex:
        // @ CLOCK.HARD [302186 samples]

        let mut input = input.as_bytes();
        let mut line = String::new();

        // read fist line
        if let Ok(n) = input.read_line(&mut line) {
            if n == 0 {
                return None;
            }
        } else {
            return Some(false);
        }

        let line = line.trim();

        // minimal check
        if !(line.starts_with("@ ") && line.ends_with(" samples]")) {
            return Some(false);
        }

        Some(true)
    }

    fn would_end_stack(&mut self, line: &[u8]) -> bool {
        line.is_empty()
    }

    fn clone_and_reset_stack_context(&self) -> Self {
        Self {
            nstacks_per_job: self.nstacks_per_job,
            stack: VecDeque::default(),
            indent: None,
            count: None,
            opt: self.opt.clone(),
        }
    }

    fn nstacks_per_job(&self) -> usize {
        self.nstacks_per_job
    }

    fn set_nstacks_per_job(&mut self, n: usize) {
        self.nstacks_per_job = n;
    }

    fn nthreads(&self) -> usize {
        self.opt.nthreads
    }

    fn set_nthreads(&mut self, n: usize) {
        self.opt.nthreads = n;
    }
}

impl Folder {
    /// Processes a stack. On success, returns `true` if at end of data; `false` otherwise.
    fn process_single_stack<R>(
        &mut self,
        line_buffer: &mut Vec<u8>,
        reader: &mut R,
        occurrences: &mut Occurrences,
    ) -> io::Result<bool>
    where
        R: io::BufRead,
    {
        loop {
            line_buffer.clear();
            if reader.read_until(0x0A, line_buffer)? == 0 {
                if !self.stack.is_empty() {
                    self.after_stack(occurrences);
                }
                return Ok(true);
            }
            let line = String::from_utf8_lossy(line_buffer);
            if line.starts_with('@') {
                continue;
            }
            let line = line.trim_end();
            if line.is_empty() {
                self.after_stack(occurrences);
                return Ok(false);
            } else {
                self.on_stack_line(line, occurrences);
            }
        }
    }

    /// Parse a stack line and extract and validate some fields
    // We extract:
    // - the size of the leading spaces
    // - the percentage value (not used for now but just in case it is needed later)
    // - the count between [ ]
    // - the function name
    //
    // We ignore the module part "@ xxx" which is optionnaly present at the end of the line
    //
    // Ex:
    // 08.91%  [1318]     acpi_cpu_c1 @ /boot/kernel/kernel
    // 100.0%  [1318]      acpi_cpu_idle
    //  100.0%  [1318]       cpu_idle_acpi
    fn stack_line_parts(line: &str) -> Option<(usize, &str, usize, &str)> {
        // count leading spaces
        let indent = line.chars().position(|c| !c.is_whitespace()).unwrap_or(0);

        // Ex: "  54.00%  [27]         kern_clock_nanosleep"
        let mut words = line[indent..].split_whitespace(); // TODO check performance vs multiple splitn()

        let percent = words.next();
        let count = words.next();
        let function = words.next();

        match (percent, count, function) {
            (Some(percent), Some(count), Some(function)) => {
                // minimal validation '0%' -> 'x.y%' -> '100.0%'
                let lpercent = percent.len();
                if lpercent >= 2 && percent.ends_with('%') {
                    // minimal validation '[numbers]'
                    let lcount = count.len();
                    if lcount >= 3 && count.starts_with('[') && count.ends_with(']') {
                        if let Ok(count) = count[1..lcount - 1].parse() {
                            return Some((indent, &percent[..lpercent - 1], count, function));
                        }
                    }
                }
            }
            (_, _, _) => {
                println!("not enough words, ignoring line");
            }
        }

        None
    }

    // we have a stack line that shows one stack entry from the preceeding event, like:
    //
    // 08.91%  [1318]     acpi_cpu_c1 @ /boot/kernel/kernel
    //  100.0%  [1318]      acpi_cpu_idle
    //   100.0%  [1318]       cpu_idle_acpi
    //    100.0%  [1318]        cpu_idle
    //     100.0%  [1318]         sched_idletd
    //      100.0%  [1318]          fork_exit
    fn on_stack_line(&mut self, line: &str, occurrences: &mut Occurrences) {
        let parts = Self::stack_line_parts(line);
        match parts {
            Some((indent, _, count, function)) => {
                // detect shared stacks, i.e. stacks that share some elements
                // for example:
                //
                // 01.17%  [173]      randomdev_encrypt @ /boot/kernel/kernel
                //  95.95%  [166]       random_fortuna_read
                //   100.0%  [166]        read_random_uio
                //    100.0%  [166]         devfs_read_f
                //     100.0%  [166]          kern_readv
                //      100.0%  [166]           sys_read
                //       100.0%  [166]            amd64_syscall
                //  04.05%  [7]         read_random_uio
                //   100.0%  [7]          devfs_read_f
                //    100.0%  [7]           kern_readv
                //     100.0%  [7]            sys_read
                //      100.0%  [7]             amd64_syscall
                //
                // Or (a more complex one)
                //
                // 00.31%  [2]        0xf4ae3 @ /lib/libc.so.7
                //  50.00%  [1]         0x53c2f @ /usr/lib/libprivatessh.so.5
                //  50.00%  [1]         0x3b25deb @ /usr/bin/clang-cpp
                //   100.0%  [1]          0x48c282e
                //    100.0%  [1]           0x48c73c8
                if self.indent.is_some() && indent <= self.indent.unwrap_or(0) {
                    // allocate a string that is long enough to hold the entire stack string
                    let mut stack_str = String::with_capacity(
                        self.stack.iter().fold(0, |a, s| a + s.len() + 1),
                    );

                    // add the stack entries
                    let mut first = true;
                    for e in self.stack.iter() {
                        if !first {
                            stack_str.push(';');
                        } else {
                            first = false;
                        }
                        stack_str.push_str(e);
                    }

                    // count it!
                    assert!(self.count.is_some());
                    occurrences.insert_or_add(stack_str, self.count.expect("count not found on previous line"));

                    // pop as many element as needed to prepare for the next shared stack
                    self.stack.drain(.. self.stack.len() - indent);
                }

                self.indent = Some(indent);
                self.count = Some(count);

                // TODO annotate kernel functions with a `_[k]` suffix.
                // TODO filter raw addresses
                // TODO demangle C++ / Rust symbols
                self.stack.push_front(function.to_string());
            },
            None => {
                logging::weird_stack_line(line);
            },
        }
    }

    fn after_stack(&mut self, occurrences: &mut Occurrences) {
        // end of stack, so emit stack entry
        if !self.stack.is_empty() {
            // allocate a string that is long enough to hold the entire stack string
            let mut stack_str = String::with_capacity(
                self.stack.iter().fold(0, |a, s| a + s.len() + 1),
            );

            // add the stack entries (if any)
            let mut first = true;
            for e in self.stack.drain(..) {
                if !first {
                    stack_str.push(';');
                } else {
                    first = false;
                }
                stack_str.push_str(&e);
            }

            // count it!
            assert!(self.count.is_some());
            occurrences.insert_or_add(stack_str, self.count.expect("count not found on previous line"));
        }

        // reset for next stack
        self.stack.clear();
        self.indent = None;
        self.count = None;
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::io::Read;
    use std::path::PathBuf;

    use lazy_static::lazy_static;
    use pretty_assertions::assert_eq;
    use rand::prelude::*;

    use super::*;
    use crate::collapse::common;
    use crate::collapse::Collapse;

    lazy_static! {
        static ref INPUT: Vec<PathBuf> = {
            [
                "./tests/data/collapse-pmc/simple.txt",
                "./tests/data/collapse-pmc/shared.txt",
                "./tests/data/collapse-pmc/shared2.txt",
                "./tests/data/collapse-pmc/dd.txt",
                "./tests/data/collapse-pmc/iperf3.txt",
                // "./tests/data/collapse-pmc/large.txt.gz",
            ]
            .iter()
            .map(PathBuf::from)
            .collect::<Vec<_>>()
        };
    }

    #[test]
    fn test_collapse_multi_pmc() -> io::Result<()> {
        let mut folder = Folder::default();
        common::testing::test_collapse_multi(&mut folder, &INPUT)
    }

    #[test]
    fn test_collapse_multi_pmc_simple() -> io::Result<()> {
        let path = "./tests/data/collapse-pmc/simple.txt";
        let mut file = fs::File::open(path)?;
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes)?;
        let mut folder = Folder::default();
        <Folder as Collapse>::collapse(&mut folder, &bytes[..], io::sink())
    }

    /// Varies the nstacks_per_job parameter and outputs the 10 fastests configurations by file.
    ///
    /// Command: `cargo test bench_nstacks_pmc --release -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn bench_nstacks_pmc() -> io::Result<()> {
        let mut folder = Folder::default();
        common::testing::bench_nstacks(&mut folder, &INPUT)
    }

    #[test]
    #[ignore]
    /// Fuzz test the multithreaded collapser.
    ///
    /// Command: `cargo test fuzz_collapse_pmc --release -- --ignored --nocapture`
    fn fuzz_collapse_pmc() -> io::Result<()> {
        let seed = thread_rng().gen::<u64>();
        println!("Random seed: {}", seed);
        let mut rng = SmallRng::seed_from_u64(seed);

        let mut buf_actual = Vec::new();
        let mut buf_expected = Vec::new();
        let mut count = 0;

        let inputs = common::testing::read_inputs(&INPUT)?;

        loop {
            let nstacks_per_job = rng.gen_range(1..=500);
            let options = Options {
                nthreads: rng.gen_range(2..=32),
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
                <Folder as Collapse>::collapse(&mut folder, &input[..], &mut buf_expected)?;
                let expected = std::str::from_utf8(&buf_expected[..]).unwrap();

                let mut folder = Folder::from(options.clone());
                folder.nstacks_per_job = nstacks_per_job;
                <Folder as Collapse>::collapse(&mut folder, &input[..], &mut buf_actual)?;
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
