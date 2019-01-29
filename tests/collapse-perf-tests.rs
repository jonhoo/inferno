#[macro_use]
extern crate pretty_assertions;

extern crate inferno_collapse_perf;

use inferno_collapse_perf::{handle_file, Options};
use std::fmt;
use std::fs::File;
use std::io::{self, BufRead, BufReader, Cursor};
use std::path::Path;

// Create tests for test files in the flamegraph/test directory.
// The test and result file names are derived from the test name.
// The part after the last underscore is the flag name to use.
// For example, perf_cycles_instructions_01_pid will use the following:
//     test file: perf-cycles-instructions-01.txt
//     result file: perf-cycles-instructions-01-collapsed-pid.txt
//     flag: pid
macro_rules! collapse_perf_tests {
    ($($name:ident,)*) => {
    $(
        #[test]
        #[allow(non_snake_case)]
        fn $name() {
            let test_name = stringify!($name);
            let last_underscore = test_name
                .rfind('_')
                .expect("test name must have underscore");
            let test_file_stem = (&test_name[0..last_underscore]).replace("_", "-");
            let flag: Flag = (&test_name[last_underscore + 1..test_name.len()]).into();
            let test_file = format!("{}.txt", test_file_stem);
            let result_file = format!("{}-collapsed-{}.txt", test_file_stem, flag.to_string());

            let test_path = Path::new("./flamegraph/test");
            let results_path = test_path.join("results");

            test_collapse_perf(
                test_path.join(test_file).to_str().unwrap(),
                results_path.join(result_file).to_str().unwrap(),
                flag.into(),
            ).unwrap()
        }
    )*
    }
}

collapse_perf_tests! {
    perf_cycles_instructions_01_pid,
    perf_cycles_instructions_01_tid,
    perf_cycles_instructions_01_kernel,
    perf_cycles_instructions_01_jit,
    perf_cycles_instructions_01_all,
    perf_cycles_instructions_01_addrs,

    perf_dd_stacks_01_pid,
    perf_dd_stacks_01_tid,
    perf_dd_stacks_01_kernel,
    perf_dd_stacks_01_jit,
    perf_dd_stacks_01_all,
    perf_dd_stacks_01_addrs,

    perf_funcab_cmd_01_pid,
    perf_funcab_cmd_01_tid,
    perf_funcab_cmd_01_kernel,
    perf_funcab_cmd_01_jit,
    perf_funcab_cmd_01_all,
    perf_funcab_cmd_01_addrs,

    perf_funcab_pid_01_pid,
    perf_funcab_pid_01_tid,
    perf_funcab_pid_01_kernel,
    perf_funcab_pid_01_jit,
    perf_funcab_pid_01_all,
    perf_funcab_pid_01_addrs,

    perf_iperf_stacks_pidtid_01_pid,
    perf_iperf_stacks_pidtid_01_tid,
    perf_iperf_stacks_pidtid_01_kernel,
    perf_iperf_stacks_pidtid_01_jit,
    perf_iperf_stacks_pidtid_01_all,
    perf_iperf_stacks_pidtid_01_addrs,

    perf_java_faults_01_pid,
    perf_java_faults_01_tid,
    perf_java_faults_01_kernel,
    perf_java_faults_01_jit,
    perf_java_faults_01_all,
    perf_java_faults_01_addrs,

    perf_java_stacks_01_pid,
    perf_java_stacks_01_tid,
    perf_java_stacks_01_kernel,
    perf_java_stacks_01_jit,
    perf_java_stacks_01_all,
    perf_java_stacks_01_addrs,

    perf_java_stacks_02_pid,
    perf_java_stacks_02_tid,
    perf_java_stacks_02_kernel,
    perf_java_stacks_02_jit,
    perf_java_stacks_02_all,
    perf_java_stacks_02_addrs,

    perf_js_stacks_01_pid,
    perf_js_stacks_01_tid,
    perf_js_stacks_01_kernel,
    perf_js_stacks_01_jit,
    perf_js_stacks_01_all,
    perf_js_stacks_01_addrs,

    perf_mirageos_stacks_01_pid,
    perf_mirageos_stacks_01_tid,
    perf_mirageos_stacks_01_kernel,
    perf_mirageos_stacks_01_jit,
    perf_mirageos_stacks_01_all,
    perf_mirageos_stacks_01_addrs,

    perf_numa_stacks_01_pid,
    perf_numa_stacks_01_tid,
    perf_numa_stacks_01_kernel,
    perf_numa_stacks_01_jit,
    perf_numa_stacks_01_all,
    perf_numa_stacks_01_addrs,

    perf_rust_Yamakaky_dcpu_pid,
    perf_rust_Yamakaky_dcpu_tid,
    perf_rust_Yamakaky_dcpu_kernel,
    perf_rust_Yamakaky_dcpu_jit,
    perf_rust_Yamakaky_dcpu_all,
    perf_rust_Yamakaky_dcpu_addrs,

    perf_vertx_stacks_01_pid,
    perf_vertx_stacks_01_tid,
    perf_vertx_stacks_01_kernel,
    perf_vertx_stacks_01_jit,
    perf_vertx_stacks_01_all,
    perf_vertx_stacks_01_addrs,
}

fn test_collapse_perf(test_file: &str, expected_file: &str, options: Options) -> io::Result<()> {
    let r = BufReader::new(File::open(test_file)?);

    let mut result = Cursor::new(Vec::with_capacity(expected_file.len()));
    handle_file(options, r, &mut result)?;
    let mut expected = BufReader::new(File::open(expected_file)?);

    result.set_position(0);

    let mut buf = String::new();
    for (idx, line) in result.lines().enumerate() {
        // Strip out " and ' since perl version does.
        let line = line?.replace("\"", "").replace("'", "");
        expected.read_line(&mut buf)?;
        assert_eq!(line, buf.trim_end(), "\n{}:{}", expected_file, idx + 1);
        buf.clear();
    }

    Ok(())
}

#[derive(Copy, Clone)]
enum Flag {
    PID,
    TID,
    KERNEL,
    JIT,
    ALL,
    ADDRS,
}

impl From<&str> for Flag {
    fn from(s: &str) -> Flag {
        match s {
            "pid" => Flag::PID,
            "tid" => Flag::TID,
            "kernel" => Flag::KERNEL,
            "jit" => Flag::JIT,
            "all" => Flag::ALL,
            "addrs" => Flag::ADDRS,
            flag => panic!("unknown flag: {}", flag),
        }
    }
}

impl fmt::Display for Flag {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str(match self {
            Flag::PID => "pid",
            Flag::TID => "tid",
            Flag::KERNEL => "kernel",
            Flag::JIT => "jit",
            Flag::ALL => "all",
            Flag::ADDRS => "addrs",
        })
    }
}

impl Into<Options> for Flag {
    fn into(self) -> Options {
        let mut options = Options::default();
        match self {
            Flag::PID => {
                options.include_pid = true;
            }
            Flag::TID => {
                options.include_tid = true;
            }
            Flag::KERNEL => {
                options.annotate_kernel = true;
            }
            Flag::JIT => {
                options.annotate_jit = true;
            }
            Flag::ALL => {
                options.annotate_kernel = true;
                options.annotate_jit = true;
            }
            Flag::ADDRS => {
                options.include_addrs = true;
            }
        };

        options
    }
}
