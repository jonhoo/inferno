use std::collections::{HashMap, VecDeque};
use std::io;
use std::io::prelude::*;

const TIDY_GENERIC: bool = true;
const TIDY_JAVA: bool = true;

#[derive(Debug, Default)]
pub struct Options {
    /// include PID with process names [1]
    pub include_pid: bool,

    /// include TID and PID with process names [1]
    pub include_tid: bool,

    /// include raw addresses where symbols can't be found
    pub include_addrs: bool,

    /// annotate jit functions with a _[j]
    pub annotate_jit: bool,

    /// annotate kernel functions with a _[k]
    pub annotate_kernel: bool,

    /// event type filter, defaults to first encountered event
    pub event_filter: Option<String>,
}

pub fn handle_file<R: BufRead, W: Write>(opt: Options, mut reader: R, writer: W) -> io::Result<()> {
    let mut line = String::new();
    let mut state = PerfState::from(opt);
    loop {
        line.clear();

        if reader.read_line(&mut line)? == 0 {
            break;
        }

        if line.starts_with('#') {
            continue;
        }

        let line = line.trim_end();
        if line.is_empty() {
            state.after_event();
        } else {
            state.on_line(line.trim_end());
        }
    }

    state.finish(writer)
}

#[derive(Debug)]
enum EventFilterState {
    None,
    Defaulted,
    Warned,
}

#[derive(Debug)]
struct PerfState {
    /// All lines until the next empty line are stack lines.
    in_event: bool,

    /// Skip all stack lines in this event.
    skip_stack: bool,

    /// Function entries on the stack in this entry thus far.
    stack: VecDeque<String>,

    /// Number of times each call stack has been seen.
    occurrences: HashMap<String, usize>,

    /// Current comm name.
    ///
    /// Called pname after original stackcollapse-perf source.
    pname: String,

    /// The options for the current run.
    opt: Options,

    event_filtering: EventFilterState,
}

impl From<Options> for PerfState {
    fn from(opt: Options) -> Self {
        PerfState {
            in_event: false,
            skip_stack: false,
            stack: VecDeque::default(),
            occurrences: HashMap::default(),
            pname: String::new(),
            event_filtering: EventFilterState::None,
            opt,
        }
    }
}

impl PerfState {
    fn on_line(&mut self, line: &str) {
        if !self.in_event {
            self.on_event_line(line)
        } else {
            self.on_stack_line(line)
        }
    }

    fn event_line_parts(line: &str) -> Option<(&str, &str, &str)> {
        let mut word_start = 0;
        let mut all_digits = false;
        let mut contains_slash_at = None;
        for (idx, c) in line.char_indices() {
            if c == ' ' {
                if all_digits {
                    // found an all-digit word
                    let (pid, tid) = if let Some(slash) = contains_slash_at {
                        // found PID + TID
                        (&line[word_start..slash], &line[(slash + 1)..idx])
                    } else {
                        // found TID
                        ("?", &line[word_start..idx])
                    };
                    let comm = &line[..(word_start - 1)];

                    // XXX: two spaces in a row would see all_digits = true erroneously
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

                    if let Some(ref filter) = self.opt.event_filter {
                        if event != filter {
                            if let EventFilterState::Defaulted = self.event_filtering {
                                // only print this warning if necessary:
                                // when we defaulted and there was
                                // multiple event types.
                                eprintln!("Filtering for events of type: {}", event);
                                self.event_filtering = EventFilterState::Warned;
                            }
                            self.skip_stack = true;
                            return;
                        }
                    } else {
                        // By default only show events of the first encountered
                        // event type. Merging together different types, such as
                        // instructions and cycles, produces misleading results.
                        self.opt.event_filter = Some(event.to_string());
                        self.event_filtering = EventFilterState::Defaulted;
                    }
                    // TODO: filter by event
                    if false {
                        self.skip_stack = true;
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
            eprintln!("weird event line: {}", line);
            self.in_event = false;
        }
    }

    fn stack_line_parts(line: &str) -> Option<(&str, &str, &str)> {
        let mut line = line.trim_start().splitn(2, ' ');
        let pc = line.next()?;
        let mut line = line.next()?.rsplitn(2, ' ');
        let mut module = line.next()?;
        // module is always wrapped in (), so remove those
        module = &module[1..(module.len() - 1)];
        let rawfunc = match line.next()? {
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

            // TODO: show_inline

            // skip process names?
            // see https://github.com/brendangregg/FlameGraph/blob/f857ebc94bfe2a9bfdc4f1536ebacfb7466f69ba/stackcollapse-perf.pl#L269
            if rawfunc.starts_with('(') {
                return;
            }

            let mut func = with_module_fallback(module, rawfunc, pc, self.opt.include_addrs);
            if TIDY_GENERIC {
                func = tidy_generic(func);
            }

            if TIDY_JAVA && self.pname == "java" {
                func = tidy_java(func);
            }

            // Annotations
            //
            // detect kernel from the module name; eg, frames to parse include:
            //
            //     ffffffff8103ce3b native_safe_halt ([kernel.kallsyms])
            //     8c3453 tcp_sendmsg (/lib/modules/4.3.0-rc1-virtual/build/vmlinux)
            //     7d8 ipv4_conntrack_local+0x7f8f80b8 ([nf_conntrack_ipv4])
            //
            // detect jit from the module name; eg:
            //
            //     7f722d142778 Ljava/io/PrintStream;::print (/tmp/perf-19982.map)
            if self.opt.annotate_kernel
                && (module.starts_with('[') || module.ends_with("vmlinux"))
                && module != "[unknown]"
            {
                func.push_str("_[k]");
            }
            if self.opt.annotate_jit && module.starts_with("/tmp/perf-") && module.ends_with(".map")
            {
                func.push_str("_[j]");
            }

            self.stack.push_front(func);
        } else {
            eprint!("weird stack line: {}", line);
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
            *self.occurrences.entry(stack_str).or_insert(0) += 1;
        }

        // reset for the next event
        self.in_event = false;
        self.skip_stack = false;
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

// massage function name to be nicer
// NOTE: ignoring https://github.com/jvm-profiling-tools/perf-map-agent/pull/35
fn with_module_fallback(module: &str, rawfunc: &str, pc: &str, include_addrs: bool) -> String {
    if rawfunc != "[unknown]" {
        return rawfunc.to_string();
    }

    // try to use part of module name as function if unknown
    let rawfunc = match (module, include_addrs) {
        ("[unknown]", true) => "unknown",
        ("[unknown]", false) => {
            // no need to process this further
            return rawfunc.to_string();
        }
        (module, _) => {
            // use everything following last / of module as function name
            &module[module.rfind('/').map(|i| i + 1).unwrap_or(0)..]
        }
    };

    // output string is a bit longer than rawfunc but not much
    let mut res = String::with_capacity(rawfunc.len() + 12);

    if include_addrs {
        res.push_str("[");
        res.push_str(rawfunc);
        res.push_str(" <");
        res.push_str(pc);
        res.push_str(">]");
    } else {
        res.push_str("[");
        res.push_str(rawfunc);
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
