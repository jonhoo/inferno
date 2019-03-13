#[macro_use]
extern crate pretty_assertions;

extern crate inferno;

mod collapse_common;

use collapse_common::*;
use inferno::collapse::perf::{Folder, Options};
use log::Level;
use std::io;
use std::path::Path;

fn test_collapse_perf(test_file: &str, expected_file: &str, options: Options) -> io::Result<()> {
    test_collapse(Folder::from(options), test_file, expected_file)
}

fn test_collapse_perf_logs<F>(input_file: &str, asserter: F)
where
    F: Fn(&Vec<testing_logger::CapturedLog>),
{
    test_collapse_perf_logs_with_options(input_file, asserter, Options::default());
}

fn test_collapse_perf_logs_with_options<F>(input_file: &str, asserter: F, options: Options)
where
    F: Fn(&Vec<testing_logger::CapturedLog>),
{
    test_collapse_logs(Folder::from(options), input_file, asserter);
}

fn options_from_vec(opt_vec: Vec<&str>) -> Options {
    let mut options = Options::default();
    for option in opt_vec {
        match option {
            "pid" => options.include_pid = true,
            "tid" => options.include_tid = true,
            "addrs" => options.include_addrs = true,
            "jit" => options.annotate_jit = true,
            "kernel" => options.annotate_kernel = true,
            "all" => {
                options.annotate_jit = true;
                options.annotate_kernel = true;
            }
            opt => panic!("invalid option: {}", opt),
        }
    }
    options
}

// Create tests for test files in $dir. The test files are used as input
// and the results are compared to result files in the results sub directory.
// The test and result file names are derived from $name.
// The part after the double underscore are underscore separated options.
// For example, perf_cycles_instructions_01_pid will use the following:
//     test file: perf-cycles-instructions-01.txt
//     result file: perf-cycles-instructions-01-collapsed-pid.txt
//     flag: pid
macro_rules! collapse_perf_tests_inner {
    ($($name:ident),*; $dir:expr; $strip_prefix:expr) => {
    $(
        #[test]
        #[allow(non_snake_case)]
        fn $name() {
            let mut test_name = stringify!($name);
            if test_name.starts_with($strip_prefix) {
                test_name = &test_name[$strip_prefix.len()..];
            }
            let mut split_name = test_name.split("__");
            let test_file_stem = split_name.next().unwrap().replace("_", "-");
            let options: Vec<_> = split_name.next().map(|s| s.split('_').collect()).unwrap_or_default();

            let test_file = format!("{}.txt", test_file_stem);
            let result_file = format!(
                "{}-collapsed{}.txt",
                test_file_stem,
                if options.is_empty() {
                    "".to_string()
                } else {
                    format!("-{}", options.join("+"))
                }
            );

            let test_path = Path::new($dir);
            let results_path = test_path.join("results");

            test_collapse_perf(
                test_path.join(test_file).to_str().unwrap(),
                results_path.join(result_file).to_str().unwrap(),
                options_from_vec(options),
            )

                .unwrap()
        }
    )*
    }
}

macro_rules! collapse_perf_tests_upstream {
    ($($name:ident),*) => {
        collapse_perf_tests_inner!($($name),*; "./flamegraph/test"; "collapse_");
    }
}

collapse_perf_tests_upstream! {
    collapse_perf_cycles_instructions_01__pid,
    collapse_perf_cycles_instructions_01__tid,
    collapse_perf_cycles_instructions_01__kernel,
    collapse_perf_cycles_instructions_01__jit,
    collapse_perf_cycles_instructions_01__all,
    collapse_perf_cycles_instructions_01__addrs,

    collapse_perf_dd_stacks_01__pid,
    collapse_perf_dd_stacks_01__tid,
    collapse_perf_dd_stacks_01__kernel,
    collapse_perf_dd_stacks_01__jit,
    collapse_perf_dd_stacks_01__all,
    collapse_perf_dd_stacks_01__addrs,

    collapse_perf_funcab_cmd_01__pid,
    collapse_perf_funcab_cmd_01__tid,
    collapse_perf_funcab_cmd_01__kernel,
    collapse_perf_funcab_cmd_01__jit,
    collapse_perf_funcab_cmd_01__all,
    collapse_perf_funcab_cmd_01__addrs,

    collapse_perf_funcab_pid_01__pid,
    collapse_perf_funcab_pid_01__tid,
    collapse_perf_funcab_pid_01__kernel,
    collapse_perf_funcab_pid_01__jit,
    collapse_perf_funcab_pid_01__all,
    collapse_perf_funcab_pid_01__addrs,

    collapse_perf_iperf_stacks_pidtid_01__pid,
    collapse_perf_iperf_stacks_pidtid_01__tid,
    collapse_perf_iperf_stacks_pidtid_01__kernel,
    collapse_perf_iperf_stacks_pidtid_01__jit,
    collapse_perf_iperf_stacks_pidtid_01__all,
    collapse_perf_iperf_stacks_pidtid_01__addrs,

    collapse_perf_java_faults_01__pid,
    collapse_perf_java_faults_01__tid,
    collapse_perf_java_faults_01__kernel,
    collapse_perf_java_faults_01__jit,
    collapse_perf_java_faults_01__all,
    collapse_perf_java_faults_01__addrs,

    collapse_perf_java_stacks_01__pid,
    collapse_perf_java_stacks_01__tid,
    collapse_perf_java_stacks_01__kernel,
    collapse_perf_java_stacks_01__jit,
    collapse_perf_java_stacks_01__all,
    collapse_perf_java_stacks_01__addrs,

    collapse_perf_java_stacks_02__pid,
    collapse_perf_java_stacks_02__tid,
    collapse_perf_java_stacks_02__kernel,
    collapse_perf_java_stacks_02__jit,
    collapse_perf_java_stacks_02__all,
    collapse_perf_java_stacks_02__addrs,

    collapse_perf_js_stacks_01__pid,
    collapse_perf_js_stacks_01__tid,
    collapse_perf_js_stacks_01__kernel,
    collapse_perf_js_stacks_01__jit,
    collapse_perf_js_stacks_01__all,
    collapse_perf_js_stacks_01__addrs,

    collapse_perf_mirageos_stacks_01__pid,
    collapse_perf_mirageos_stacks_01__tid,
    collapse_perf_mirageos_stacks_01__kernel,
    collapse_perf_mirageos_stacks_01__jit,
    collapse_perf_mirageos_stacks_01__all,
    collapse_perf_mirageos_stacks_01__addrs,

    collapse_perf_numa_stacks_01__pid,
    collapse_perf_numa_stacks_01__tid,
    collapse_perf_numa_stacks_01__kernel,
    collapse_perf_numa_stacks_01__jit,
    collapse_perf_numa_stacks_01__all,
    collapse_perf_numa_stacks_01__addrs,

    collapse_perf_rust_Yamakaky_dcpu__pid,
    collapse_perf_rust_Yamakaky_dcpu__tid,
    collapse_perf_rust_Yamakaky_dcpu__kernel,
    collapse_perf_rust_Yamakaky_dcpu__jit,
    collapse_perf_rust_Yamakaky_dcpu__all,
    collapse_perf_rust_Yamakaky_dcpu__addrs,

    collapse_perf_vertx_stacks_01__pid,
    collapse_perf_vertx_stacks_01__tid,
    collapse_perf_vertx_stacks_01__kernel,
    collapse_perf_vertx_stacks_01__jit,
    collapse_perf_vertx_stacks_01__all,
    collapse_perf_vertx_stacks_01__addrs
}

macro_rules! collapse_perf_tests {
    ($($name:ident),*) => {
        collapse_perf_tests_inner!($($name),*; "./tests/data/collapse-perf"; "collapse_perf_");
    }
}

collapse_perf_tests! {
    collapse_perf_go_stacks,
    collapse_perf_java_inline
}

#[test]
fn collapse_perf_example_perf_stacks() {
    test_collapse_perf(
        "./flamegraph/example-perf-stacks.txt.gz",
        "./tests/data/collapse-perf/results/example-perf-stacks-collapsed.txt",
        Default::default(),
    )
    .unwrap();
}

#[test]
fn collapse_perf_should_warn_about_empty_input_lines() {
    test_collapse_perf_logs(
        "./tests/data/collapse-perf/empty-line.txt",
        |captured_logs| {
            let nwarnings = captured_logs
                .into_iter()
                .filter(|log| {
                    log.body.starts_with("weird event line: ") && log.level == Level::Warn
                })
                .count();
            assert_eq!(
                nwarnings, 1,
                "bad lines warning logged {} times, but should be logged exactly once",
                nwarnings
            );
        },
    );
}

#[test]
fn collapse_perf_should_warn_about_weird_input_lines() {
    test_collapse_perf_logs(
        "./tests/data/collapse-perf/weird-stack-line.txt",
        |captured_logs| {
            let nwarnings = captured_logs
                .into_iter()
                .filter(|log| {
                    log.body.starts_with("weird stack line: ") && log.level == Level::Warn
                })
                .count();
            assert_eq!(
                nwarnings, 1,
                "bad lines warning logged {} times, but should be logged exactly once",
                nwarnings
            );
        },
    );
}
