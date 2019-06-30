use std::collections::VecDeque;
use std::io::{self, prelude::*, BufRead};

use crate::collapse::{Collapse, Input, Occurrences, CAPACITY_INPUT_BUFFER};

const TIDY_GENERIC: bool = true;
const TIDY_JAVA: bool = true;

mod logging {
    use log::{info, warn};

    pub(super) fn filtering_for_events_of_type(ty: &str) {
        info!("Filtering for events of type: {}", ty);
    }

    pub(super) fn weird_event_line(line: &str) {
        warn!("Weird event line: {}", line);
    }

    pub(super) fn weird_stack_line(line: &str) {
        warn!("Weird stack line: {}", line);
    }
}

/// Settings that change how frames are named from the incoming stack traces.
///
/// All options default to off, expect nthreads, which defaults to the number
/// of logical cores on your machine.
#[derive(Clone, Debug)]
pub struct Options {
    /// Annotate JIT functions with a `_[j]` suffix.
    pub annotate_jit: bool,

    /// Annotate kernel functions with a `_[k]` suffix.
    pub annotate_kernel: bool,

    /// Only consider samples of the given event type (see `perf list`).
    ///
    /// If this option is set to `None`, it will be set to the first encountered event type.
    pub event_filter: Option<String>,

    /// Include raw addresses (e.g., `0xbfff0836`) where symbols can't be found.
    pub include_addrs: bool,

    /// Include PID in the root frame.
    ///
    /// If disabled, the root frame is given the name of the profiled process.
    pub include_pid: bool,

    /// Include TID and PID in the root frame.
    ///
    /// Implies `include_pid`.
    pub include_tid: bool,

    /// The number of threads to use.
    pub nthreads: usize,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            annotate_jit: false,
            annotate_kernel: false,
            event_filter: None,
            include_addrs: false,
            include_pid: false,
            include_tid: false,
            nthreads: num_cpus::get(),
        }
    }
}

/// A stack collapser for the output of `perf script`.
///
/// To construct one, either use `perf::Folder::default()` or create an [`Options`] and use
/// `perf::Folder::from(options)`.
pub struct Folder {
    // State...
    /// General String cache that can be used while processing lines.
    /// Currently only used to keep track of functions for Java inlining.
    cache_line: Vec<String>,

    /// Similar to, but different from, the `event_filter` field on `Options`.
    ///
    /// * The `event_filter` field on `Options` represents the user-provided configuration and
    ///   thus never changes.
    /// * This field, however, is part of the state of the Folder and may change as the Folder
    ///   processes data. More specifically, if this field starts as `None`, it **will** change to
    ///   `Some(...)` when the Folder encounters it's first event.
    event_filter: Option<String>,

    /// All lines until the next empty line are stack lines.
    in_event: bool,

    /// Number of times each call stack has been seen.
    occurrences: Occurrences,

    /// Current comm name.
    ///
    /// Called pname after original stackcollapse-perf source.
    pname: String,

    /// Skip all stack lines in this event.
    skip_stack: bool,

    /// Function entries on the stack in this entry thus far.
    stack: VecDeque<String>,

    // Options...
    opt: Options,
}

impl From<Options> for Folder {
    fn from(mut opt: Options) -> Self {
        opt.include_pid = opt.include_pid || opt.include_tid;
        Self {
            cache_line: Vec::default(),
            event_filter: opt.event_filter.clone(),
            in_event: false,
            occurrences: Occurrences::new(opt.nthreads),
            skip_stack: false,
            stack: VecDeque::default(),
            pname: String::default(),
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
    fn collapse<R, W>(&mut self, reader: R, writer: W) -> io::Result<()>
    where
        R: BufRead,
        W: Write,
    {
        if self.opt.nthreads <= 1 {
            self.collapse_single_threaded(reader)?;
        } else {
            self.collapse_multi_threaded(reader)?;
        }

        self.occurrences.write(writer)
    }

    // Check if the input has an event line followed by a stack line.
    fn is_applicable(&mut self, input: &str) -> Option<bool> {
        let mut last_line_was_event_line = false;
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
            // Skip comments
            if line.starts_with('#') {
                continue;
            }

            if line.is_empty() {
                last_line_was_event_line = false;
                continue;
            }

            if last_line_was_event_line {
                // If this is valid input this line should be a stack line.
                return Some(Self::stack_line_parts(line).is_some());
            } else {
                if Self::event_line_parts(line).is_none() {
                    // The first line that's not empty or a comment should be an event line.
                    return Some(false);
                }
                last_line_was_event_line = true;
            }
        }
        None
    }
}

impl Folder {
    fn collapse_single_threaded<R>(&mut self, mut reader: R) -> io::Result<()>
    where
        R: BufRead,
    {
        let mut line = String::new();
        loop {
            line.clear();
            if reader.read_line(&mut line)? == 0 {
                return Ok(());
            }
            if line.starts_with('#') {
                continue;
            }
            let line = line.trim_end();
            if line.is_empty() {
                self.after_event();
            } else if self.in_event {
                self.on_stack_line(line);
            } else {
                self.on_event_line(line);
            }
        }
    }

    fn collapse_multi_threaded<R>(&mut self, mut reader: R) -> io::Result<()>
    where
        R: io::BufRead,
    {
        assert!(self.occurrences.is_concurrent());
        assert!(self.opt.nthreads > 1);

        let mut buf = Vec::with_capacity(CAPACITY_INPUT_BUFFER);
        reader.read_to_end(&mut buf)?;

        let mut input = Input::new(
            buf,
            self.opt.nthreads,
            Self::identify_stack_locations(&mut self.event_filter),
        )?;

        crossbeam::thread::scope(|scope| {
            let mut handles = Vec::with_capacity(input.nthreads());
            let (sender, receiver) = crossbeam::channel::bounded(input.nthreads());
            for chunk in input.chunks() {
                let event_filter = self.event_filter.clone();
                let occurrences = self.occurrences.clone();
                let opt = self.opt.clone();

                let sender = sender.clone();

                let handle = scope.spawn(move |_| {
                    let mut folder = Folder {
                        // state
                        cache_line: Vec::default(),
                        event_filter,
                        in_event: false,
                        occurrences,
                        skip_stack: false,
                        stack: VecDeque::default(),
                        pname: String::new(),
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
            Ok(())
        })
        .unwrap()
    }

    fn event_line_parts(line: &str) -> Option<(&str, &str, &str)> {
        let mut word_start = 0;
        let mut all_digits = false;
        let mut last_was_space = false;
        let mut contains_slash_at = None;
        for (idx, c) in line.char_indices() {
            if c == ' ' {
                if all_digits && !last_was_space {
                    // found an all-digit word
                    let (pid, tid) = if let Some(slash) = contains_slash_at {
                        // found PID + TID
                        (&line[word_start..slash], &line[(slash + 1)..idx])
                    } else {
                        // found TID
                        ("?", &line[word_start..idx])
                    };
                    // also trim comm in case multiple spaces were used to separate
                    let comm = line[..(word_start - 1)].trim();
                    return Some((comm, pid, tid));
                }
                word_start = idx + 1;
                all_digits = true;
            } else if c == '/' {
                if all_digits {
                    contains_slash_at = Some(idx);
                }
            } else if c.is_ascii_digit() {
                // we're still all digits if we were all digits
            } else {
                all_digits = false;
                contains_slash_at = None;
            }
            last_was_space = c == ' ';
        }
        None
    }

    // we have an event line, like:
    //
    //     java 25607 4794564.109216: cycles:
    //     java 12688 [002] 6544038.708352: cpu-clock:
    //     V8 WorkerThread 25607 4794564.109216: cycles:
    //     java 24636/25607 [000] 4794564.109216: cycles:
    //     java 12688/12764 6544038.708352: cpu-clock:
    //     V8 WorkerThread 24636/25607 [000] 94564.109216: cycles:
    //     vote   913    72.176760:     257597 cycles:uppp:
    fn on_event_line(&mut self, line: &str) {
        self.in_event = true;

        if let Some((comm, pid, tid)) = Self::event_line_parts(line) {
            if let Some(event) = line.rsplitn(2, ' ').next() {
                if event.ends_with(':') {
                    let event = &event[..(event.len() - 1)];

                    match self.event_filter {
                        None => {
                            // By default only show events of the first encountered
                            // event type. Merging together different types, such as
                            // instructions and cycles, produces misleading results.
                            logging::filtering_for_events_of_type(event);
                            self.event_filter = Some(event.to_string());
                        }
                        Some(ref s) => {
                            if event != s {
                                self.skip_stack = true;
                                return;
                            }
                        }
                    }
                }
            }

            // XXX: re-use existing memory in pname if possible
            self.pname = comm.replace(' ', "_");
            if self.opt.include_tid {
                self.pname.push_str("-");
                self.pname.push_str(pid);
                self.pname.push_str("/");
                self.pname.push_str(tid);
            } else if self.opt.include_pid {
                self.pname.push_str("-");
                self.pname.push_str(pid);
            }
        } else {
            logging::weird_event_line(line);
            self.in_event = false;
        }
    }

    fn stack_line_parts(line: &str) -> Option<(&str, &str, &str)> {
        let mut line = line.trim_start().splitn(2, ' ');
        let pc = line.next()?.trim_end();
        let mut line = line.next()?.rsplitn(2, ' ');
        let mut module = line.next()?;
        // module is always wrapped in (), so remove those
        module = &module[1..(module.len() - 1)];
        let rawfunc = match line.next()?.trim() {
            // Sometimes there are two spaces betwen the pc and the (, like:
            //     7f1e2215d058  (/lib/x86_64-linux-gnu/libc-2.15.so)
            // In order to match the perl version, the rawfunc should be " ", and not "".
            "" => " ",
            s => s,
        };
        Some((pc, rawfunc, module))
    }

    // we have a stack line that shows one stack entry from the preceeding event, like:
    //
    //     ffffffff8103ce3b native_safe_halt ([kernel.kallsyms])
    //     ffffffff8101c6a3 default_idle ([kernel.kallsyms])
    //     ffffffff81013236 cpu_idle ([kernel.kallsyms])
    //     ffffffff815bf03e rest_init ([kernel.kallsyms])
    //     ffffffff81aebbfe start_kernel ([kernel.kallsyms].init.text)
    //     7f533952bc77 _dl_check_map_versions+0x597 (/usr/lib/ld-2.28.so)
    //     7f53389994d0 [unknown] ([unknown])
    //                0 [unknown] ([unknown])
    fn on_stack_line(&mut self, line: &str) {
        if self.skip_stack {
            return;
        }

        if let Some((pc, mut rawfunc, module)) = Self::stack_line_parts(line) {
            // Strip off symbol offsets
            if let Some(offset) = rawfunc.rfind("+0x") {
                let end = &rawfunc[(offset + 3)..];
                if end.chars().all(|c| char::is_ascii_hexdigit(&c)) {
                    // it's a symbol offset!
                    rawfunc = &rawfunc[..offset];
                }
            }

            // skip process names?
            // see https://github.com/brendangregg/FlameGraph/blob/f857ebc94bfe2a9bfdc4f1536ebacfb7466f69ba/stackcollapse-perf.pl#L269
            if rawfunc.starts_with('(') {
                return;
            }

            // Support Java inlining by splitting on "->". After the first func, the
            // rest are annotated with "_[i]" to mark them as inlined.
            // See https://github.com/brendangregg/FlameGraph/pull/89.
            for func in rawfunc.split("->") {
                let mut func = with_module_fallback(module, func, pc, self.opt.include_addrs);
                if TIDY_GENERIC {
                    func = tidy_generic(func);
                }

                if TIDY_JAVA && self.pname == "java" {
                    func = tidy_java(func);
                }

                // Annotations
                //
                // detect inlined when self.cache_line has funcs
                // detect kernel from the module name; eg, frames to parse include:
                //
                //     ffffffff8103ce3b native_safe_halt ([kernel.kallsyms])
                //     8c3453 tcp_sendmsg (/lib/modules/4.3.0-rc1-virtual/build/vmlinux)
                //     7d8 ipv4_conntrack_local+0x7f8f80b8 ([nf_conntrack_ipv4])
                //
                // detect jit from the module name; eg:
                //
                //     7f722d142778 Ljava/io/PrintStream;::print (/tmp/perf-19982.map)
                if !self.cache_line.is_empty() {
                    func.push_str("_[i]"); // inlined
                } else if self.opt.annotate_kernel
                    && (module.starts_with('[') || module.ends_with("vmlinux"))
                    && module != "[unknown]"
                {
                    func.push_str("_[k]"); // kernel
                } else if self.opt.annotate_jit
                    && module.starts_with("/tmp/perf-")
                    && module.ends_with(".map")
                {
                    func.push_str("_[j]"); // jitted
                }

                self.cache_line.push(func);
            }

            while let Some(func) = self.cache_line.pop() {
                self.stack.push_front(func);
            }
        } else {
            logging::weird_stack_line(line);
        }
    }

    fn after_event(&mut self) {
        // end of stack, so emit stack entry
        if !self.skip_stack {
            // allocate a string that is long enough to hold the entire stack string
            let mut stack_str = String::with_capacity(
                self.pname.len() + self.stack.iter().fold(0, |a, s| a + s.len() + 1),
            );

            // add the comm name
            stack_str.push_str(&self.pname);
            // add the other stack entries (if any)
            for e in self.stack.drain(..) {
                stack_str.push_str(";");
                stack_str.push_str(&e);
            }

            // count it!
            self.occurrences.add(stack_str, 1);
        }

        // reset for the next event
        self.in_event = false;
        self.skip_stack = false;
        self.stack.clear();
    }

    /// The closure returned by this static method is what runs upfront when multiple cores are used
    /// in order to identify how the input data can be cut up into multiple pieces in order to be
    /// spread accross the multiple cores. It should return a vector with the locations (byte
    /// indices) of the start of each stack. See `crate::collapse::Input::new`.
    fn identify_stack_locations<'a>(
        event_filter: &'a mut Option<String>,
    ) -> impl FnOnce(io::BufReader<&[u8]>) -> io::Result<Vec<usize>> + 'a {
        move |mut reader: io::BufReader<&[u8]>| -> io::Result<Vec<usize>> {
            let mut byte_index = 0;
            let mut line = String::new();
            let mut stack_indices = vec![0];
            loop {
                line.clear();
                let n = reader.read_line(&mut line).unwrap();
                if n == 0 {
                    break;
                }
                byte_index += n;
                let line = line.trim_end();
                if line.is_empty() {
                    stack_indices.push(byte_index);
                    continue;
                }
                if event_filter.is_none() {
                    if let Some(event) = line.rsplitn(2, ' ').next() {
                        if event.ends_with(':') {
                            let event = &event[..(event.len() - 1)];
                            logging::filtering_for_events_of_type(event);
                            *event_filter = Some(event.to_string());
                        }
                    }
                }
            }
            Ok(stack_indices)
        }
    }
}

// massage function name to be nicer
// NOTE: ignoring https://github.com/jvm-profiling-tools/perf-map-agent/pull/35
fn with_module_fallback(module: &str, func: &str, pc: &str, include_addrs: bool) -> String {
    if func != "[unknown]" {
        return func.to_string();
    }

    // try to use part of module name as function if unknown
    let func = match (module, include_addrs) {
        ("[unknown]", true) => "unknown",
        ("[unknown]", false) => {
            // no need to process this further
            return func.to_string();
        }
        (module, _) => {
            // use everything following last / of module as function name
            &module[module.rfind('/').map(|i| i + 1).unwrap_or(0)..]
        }
    };

    // output string is a bit longer than rawfunc but not much
    let mut res = String::with_capacity(func.len() + 12);

    if include_addrs {
        res.push_str("[");
        res.push_str(func);
        res.push_str(" <");
        res.push_str(pc);
        res.push_str(">]");
    } else {
        res.push_str("[");
        res.push_str(func);
        res.push_str("]");
    }

    res
}

fn tidy_generic(mut func: String) -> String {
    func = func.replace(';', ":");
    // remove argument list from function name, but _don't_ remove:
    //
    //  - Go method names like "net/http.(*Client).Do".
    //    see https://github.com/brendangregg/FlameGraph/pull/72
    //  - C++ anonymous namespace annotations.
    //    see https://github.com/brendangregg/FlameGraph/pull/93
    if let Some(first_paren) = func.find('(') {
        if func[first_paren..].starts_with("anonymous namespace)") {
            // C++ anonymous namespace
        } else {
            let mut is_go = false;
            if let Some(c) = func.get((first_paren - 1)..first_paren) {
                // if .get(-1) is None, can't be a dot
                if c == "." {
                    // assume it's a Go method name, so do nothing
                    is_go = true;
                }
            }

            if !is_go {
                // kill it with fire!
                func.truncate(first_paren);
            }
        }
    }

    // The perl version here strips ' and "; we don't do that.
    // see https://github.com/brendangregg/FlameGraph/commit/817c6ea3b92417349605e5715fe6a7cb8cbc9776
    func
}

fn tidy_java(mut func: String) -> String {
    // along with tidy_generic converts the following:
    //     Lorg/mozilla/javascript/ContextFactory;.call(Lorg/mozilla/javascript/ContextAction;)Ljava/lang/Object;
    //     Lorg/mozilla/javascript/ContextFactory;.call(Lorg/mozilla/javascript/C
    //     Lorg/mozilla/javascript/MemberBox;.<init>(Ljava/lang/reflect/Method;)V
    // into:
    //     org/mozilla/javascript/ContextFactory:.call
    //     org/mozilla/javascript/ContextFactory:.call
    //     org/mozilla/javascript/MemberBox:.init
    if func.starts_with('L') && func.contains('/') {
        func.remove(0);
    }

    func
}

#[cfg(test)]
mod tests {
    use std::fs::File;
    use std::io::Read;
    use std::path::{Path, PathBuf};

    use lazy_static::lazy_static;
    use pretty_assertions::assert_eq;

    use super::*;

    const MAX_THREADS: usize = 16;

    lazy_static! {
        static ref INPUT: Vec<PathBuf> = {
            let mut input = [
                "perf-cycles-instructions-01.txt",
                "perf-dd-stacks-01.txt",
                "perf-funcab-cmd-01.txt",
                "perf-funcab-pid-01.txt",
                "perf-iperf-stacks-pidtid-01.txt",
                "perf-java-faults-01.txt",
                "perf-java-stacks-01.txt",
                "perf-java-stacks-02.txt",
                "perf-js-stacks-01.txt",
                "perf-mirageos-stacks-01.txt",
                "perf-numa-stacks-01.txt",
                "perf-rust-Yamakaky-dcpu.txt",
                "perf-vertx-stacks-01.txt",
            ]
            .into_iter()
            .map(|s| Path::new("./flamegraph/test").join(s))
            .collect::<Vec<_>>();

            input.extend(
                [
                    "empty-line.txt",
                    "go-stacks.txt",
                    "java-inline.txt",
                    "weird-stack-line.txt",
                ]
                .into_iter()
                .map(|s| Path::new("./tests/data/collapse-perf").join(s)),
            );

            input
        };
    }

    #[test]
    fn test_input_indices() -> io::Result<()> {
        for path in INPUT.iter() {
            let mut infile = File::open(path)?;
            let mut expected = Vec::new();
            infile.read_to_end(&mut expected)?;

            for n in 0..MAX_THREADS {
                let mut event_filter = None;
                let input = Input::new(
                    expected.clone(),
                    n,
                    Folder::identify_stack_locations(&mut event_filter),
                )?;
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
    fn test_collapse_perf_multi_threaded() -> io::Result<()> {
        for path in INPUT.iter() {
            let mut infile = File::open(path)?;
            let mut s = String::new();
            infile.read_to_string(&mut s)?;

            let mut options = Options::default();
            options.nthreads = 1;
            let mut folder = Folder::from(options);
            let mut writer = Vec::new();
            folder.collapse(io::BufReader::new(s.as_bytes()), &mut writer)?;
            let expected = std::str::from_utf8(&writer[..]).unwrap();

            for n in 0..MAX_THREADS {
                let mut options = Options::default();
                options.nthreads = n;
                let mut folder = Folder::from(options);
                let mut writer = Vec::new();
                folder.collapse(io::BufReader::new(s.as_bytes()), &mut writer)?;
                let actual = std::str::from_utf8(&writer[..]).unwrap();

                assert_eq!(actual, expected);
            }
        }

        Ok(())
    }
}
