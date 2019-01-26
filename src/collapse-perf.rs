use std::collections::{HashMap, VecDeque};
use std::io::prelude::*;
use std::io::{self, BufReader};
use structopt::StructOpt;

#[derive(Debug, StructOpt)]
#[structopt(
    name = "inferno-collapse-perf",
    author = "",
    after_help = "\
[1] perf script must emit both PID and TIDs for these to work; eg, Linux < 4.1:
        perf script -f comm,pid,tid,cpu,time,event,ip,sym,dso,trace
    for Linux >= 4.1:
        perf script -F comm,pid,tid,cpu,time,event,ip,sym,dso,trace
    If you save this output add --header on Linux >= 3.14 to include perf info."
)]
struct Opt {
    /// include PID with process names [1]
    #[structopt(long = "pid")]
    include_pid: bool,

    /// include TID and PID with process names [1]
    #[structopt(long = "tid")]
    include_tid: bool,

    /// include raw addresses where symbols can't be found
    #[structopt(long = "addrs")]
    include_addrs: bool,

    /// annotate jit functions with a _[j]
    #[structopt(long = "jit")]
    annotate_jit: bool,

    /// annotate kernel functions with a _[k]
    #[structopt(long = "kernel")]
    annotate_kernel: bool,

    /// all annotations (--kernel --jit)
    #[structopt(long = "all")]
    annotate_all: bool,

    /// perf script output file, or STDIN if not specified
    infile: Option<String>,
}

fn main() {
    let opt = Opt::from_args();

    let stdin = io::stdin();
    let stdin = stdin.lock();
    let mut reader = BufReader::new(stdin);
    let mut line = String::new();

    let match_event_line = regex::Regex::new(r#"^(\S.+?)\s+(\d+)/*(\d+)*\s+"#).unwrap();
    let match_event_line_event = regex::Regex::new(r#"(\S+):\s*$"#).unwrap();
    let match_stack_line = regex::Regex::new(r#"^\s*(\w+)\s*(.+) \((\S*)\)"#).unwrap();

    let include_pname = true;
    let tidy_generic = true;

    let mut in_event = false;
    let mut skip_event = false;
    let mut stack = VecDeque::new();
    let mut occurrences = HashMap::new();
    let mut pname = String::new();
    loop {
        line.clear();

        if reader.read_line(&mut line).unwrap() == 0 {
            break;
        }

        if line.starts_with('#') {
            continue;
        }

        if line.trim_end().is_empty() {
            // end of stack, so emit stack entry
            if include_pname {
                stack.push_front(pname.clone());
            }
            if !stack.is_empty() {
                let mut s = stack.pop_front().unwrap();
                for e in stack.drain(..) {
                    s.push_str(";");
                    s.push_str(&e);
                }
                *occurrences.entry(s).or_insert(0) += 1;
            }
            in_event = false;
            skip_event = false;
            stack.clear();
            continue;
        }

        // one of two types of line are possible.
        if !in_event {
            // either, we have an event line, like:
            //
            //     java 25607 4794564.109216: cycles:
            //     java 12688 [002] 6544038.708352: cpu-clock:
            //     V8 WorkerThread 25607 4794564.109216: cycles:
            //     java 24636/25607 [000] 4794564.109216: cycles:
            //     java 12688/12764 6544038.708352: cpu-clock:
            //     V8 WorkerThread 24636/25607 [000] 94564.109216: cycles:
            //     vote   913    72.176760:     257597 cycles:uppp:
            in_event = true;
            match match_event_line.captures(line.trim_end()) {
                Some(fields) => {
                    let comm = &fields[1];
                    let (pid, tid) = match fields.get(3) {
                        Some(tid) => (&fields[2], tid.as_str()),
                        None => ("?", &fields[2]),
                    };

                    if let Some(captures) = match_event_line_event.captures(line.trim_end()) {
                        let event = &captures[1];
                        // TODO: filter by event
                        if false {
                            skip_event = true;
                        }
                    }

                    // XXX: re-use existing memory in pname if possible
                    pname = comm.replace(' ', "_");
                    if opt.include_tid {
                        pname.push_str("-");
                        pname.push_str(pid);
                        pname.push_str("/");
                        pname.push_str(tid);
                    } else if opt.include_pid {
                        pname.push_str("-");
                        pname.push_str(pid);
                    }
                }
                None => {
                    eprint!("weird event line: {}", line);
                    in_event = false;
                    continue;
                }
            }
        } else {
            // or, we have a stack line that shows one stack entry from the preceeding event, like:
            //
            //     ffffffff8103ce3b native_safe_halt ([kernel.kallsyms])
            //     ffffffff8101c6a3 default_idle ([kernel.kallsyms])
            //     ffffffff81013236 cpu_idle ([kernel.kallsyms])
            //     ffffffff815bf03e rest_init ([kernel.kallsyms])
            //     ffffffff81aebbfe start_kernel ([kernel.kallsyms].init.text)
            //     7f533952bc77 _dl_check_map_versions+0x597 (/usr/lib/ld-2.28.so)
            //     7f53389994d0 [unknown] ([unknown])
            //                0 [unknown] ([unknown])
            if skip_event {
                continue;
            }

            match match_stack_line.captures(line.trim_end()) {
                Some(fields) => {
                    let pc = &fields[1];
                    let mut rawfunc = &fields[2];
                    let module = &fields[3];

                    // Strip off symbol offsets
                    if let Some(offset) = rawfunc.rfind("+0x") {
                        let end = &rawfunc[(offset + 3)..];
                        if end.chars().all(|c| char::is_ascii_hexdigit(&c)) {
                            // it's a symbol offset!
                            rawfunc = &rawfunc[..offset];
                        }
                    }

                    // TODO: show_inline

                    // XXX: skip process names?
                    if rawfunc.starts_with('(') {
                        continue;
                    }

                    // massage function name to be nicer
                    // NOTE: ignoring https://github.com/jvm-profiling-tools/perf-map-agent/pull/35
                    let mut func = if rawfunc == "[unknown]" {
                        // try to use part of module name as function if unknown
                        let rawfunc = if module != "[unknown]" {
                            // use everything following last / of module as function name
                            &module[module.rfind('/').map(|i| i + 1).unwrap_or(0)..]
                        } else {
                            "unknown"
                        };

                        if opt.include_addrs {
                            format!("[{} <{}>]", rawfunc, pc)
                        } else {
                            format!("[{}]", rawfunc)
                        }
                    } else {
                        rawfunc.to_string()
                    };

                    if tidy_generic {
                        func = func.replace(';', ":");
                        // remove argument list from function name, but _don't_ remove:
                        //
                        //  - Go method names like "net/http.(*Client).Do".
                        //    see https://github.com/brendangregg/FlameGraph/pull/72
                        //  - C++ anonymous namespace annotations.
                        //    see https://github.com/brendangregg/FlameGraph/pull/93
                        //
                        // TODO: turn this into a function
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
                    }

                    // TODO: tidy_java

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
                    // TODO: annotate_kernel
                    if opt.annotate_kernel
                        && (module.starts_with('[') || module.ends_with("vmlinux"))
                        && module != "[unknown]"
                    {
                        func.push_str("_[k]");
                    }
                    // TODO: annotate_jit
                    if opt.annotate_jit
                        && module.starts_with("/tmp/perf-")
                        && module.ends_with(".map")
                    {
                        func.push_str("_[j]");
                    }

                    stack.push_front(func);
                }
                None => {
                    eprint!("weird stack line: {}", line);
                    continue;
                }
            }
        }
    }

    let mut keys: Vec<_> = occurrences.keys().collect();
    keys.sort();
    for key in keys {
        println!("{} {}", key, occurrences[key]);
    }
}
